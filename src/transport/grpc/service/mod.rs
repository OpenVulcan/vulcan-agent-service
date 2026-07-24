use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, watch};
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::backends::vmm::grpc_client::VmmClient;
use crate::backends::vmm::tool_metadata::{
    vmm_binding_tool_descriptors, vmm_memory_tool_descriptors, vmm_profile_tool_descriptors,
};
use crate::host_core::HostRuntime;
use crate::host_core::host_adapter::{
    HostAdapterIdentityMode, HostAdapterRuntimeInput, ToolRefreshMode, ToolRefreshNoticeSeverity,
    ToolRegistryDiffOptions, ToolRegistrySnapshot, WorkmemIdSource, diff_tool_registry_snapshots,
};
use crate::luaskills_adapter::{
    LuaSkillToolProjectionOptions, inject_managed_luaskill_sid_argument,
    project_runtime_tool_descriptor,
};
use crate::pb_vmm as vmm_pb;
use crate::transport::mcp::McpDispatcher;
use crate::transport::mcp::protocol::PROTOCOL_VERSION_LATEST;
use crate::transport::mcp::protocol::RequestContext;

pub mod pb {
    tonic::include_proto!("vulcan.mcp.v1");
}

mod helpers;

use helpers::{
    build_mcp_call_request_context, lua_skill_package_to_pb, lua_skill_tool_to_pb,
    mcp_error_to_status, normalize_luaskill_projection, optional_str, optional_string,
    parse_json_arguments, require_luaskill_context, text_response,
    tool_call_result_to_call_response, tool_call_result_to_text_response,
};
use pb::host_adapter_service_server::{HostAdapterService, HostAdapterServiceServer};
use pb::lua_skills_service_server::{LuaSkillsService, LuaSkillsServiceServer};
use pb::mcp_service_server::{McpService, McpServiceServer};
use pb::{
    ConnectEvent, ConnectRequest, HealthzResponse, HeartbeatEvent,
    HostAdapterDiffToolRegistryRequest, HostAdapterDiffToolRegistryResponse,
    HostAdapterListVmmBindingToolsRequest, HostAdapterListVmmBindingToolsResponse,
    HostAdapterListVmmMemoryToolsRequest, HostAdapterListVmmMemoryToolsResponse,
    HostAdapterListVmmProfileToolsRequest, HostAdapterListVmmProfileToolsResponse,
    HostAdapterProfileRequest, HostAdapterProfileResponse, HostAdapterRuntimeRequest,
    HostAdapterRuntimeResponse, HostAdapterToolDescriptor, HostAdapterToolRefreshNoticeRequest,
    HostAdapterToolRefreshNoticeResponse, HostAdapterVmmStatusRequest,
    HostAdapterVmmStatusResponse, LuaSkillCallToolRequest, LuaSkillCallToolResponse,
    LuaSkillGetHelpRequest, LuaSkillGetToolRequest, LuaSkillGetToolResponse,
    LuaSkillInstallRequest, LuaSkillListHelpRequest, LuaSkillListInstalledSkillsRequest,
    LuaSkillListSkillsRequest, LuaSkillListSkillsResponse, LuaSkillListToolsRequest,
    LuaSkillListToolsResponse, LuaSkillReloadRuntimeConfigsRequest, LuaSkillRuntimeConfigRequest,
    LuaSkillRuntimeConfigResponse, LuaSkillTextResponse, LuaSkillUninstallRequest,
    LuaSkillUpdateRequest, McpCallRequest, McpCallResponse, WelcomeEvent,
    connect_event::Event as ConnectEventType,
};
use vmm_pb::vmm_service_server::{VmmService, VmmServiceServer};

mod host_adapter;
mod luaskills;
mod mcp;
mod server_runner;
mod vmm;

pub use server_runner::{run_grpc, run_grpc_with_shutdown};

// ============================================================
// Connection manager for long-lived streaming connections
// ============================================================

#[derive(Clone)]
pub struct ConnectionManager {
    connections: Arc<Mutex<HashMap<String, mpsc::Sender<ConnectEvent>>>>,
    next_id: Arc<Mutex<u64>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn register(&self, client_name: &str) -> (String, mpsc::Receiver<ConnectEvent>) {
        let (tx, rx) = mpsc::channel::<ConnectEvent>(256);
        let mut id = self.next_id.lock().await;
        *id += 1;
        let session_id = format!("grpc-{}-{}", client_name, id);
        self.connections.lock().await.insert(session_id.clone(), tx);
        eprintln!(
            "[gRPC] Client connected: {} (session: {})",
            client_name, session_id
        );
        (session_id, rx)
    }

    pub async fn unregister(&self, session_id: &str) {
        self.connections.lock().await.remove(session_id);
        eprintln!("[gRPC] Client disconnected: {}", session_id);
    }
}

// ============================================================
// gRPC service implementation
// ============================================================

#[derive(Clone)]
pub struct McpServiceImpl {
    /// Host runtime used by stable LuaSkills gRPC methods.
    /// 稳定 LuaSkills gRPC 方法使用的宿主运行时。
    runtime: HostRuntime,
    /// MCP JSON-RPC dispatcher used by the generic gRPC MCP compatibility method.
    /// 通用 gRPC MCP 兼容方法使用的 MCP JSON-RPC dispatcher。
    dispatcher: McpDispatcher,
    /// Connection manager for long-lived gRPC streams.
    /// 长连接 gRPC 流使用的连接管理器。
    manager: ConnectionManager,
    /// Service start timestamp used by health probes.
    /// 健康探测使用的服务启动时间戳。
    start_time: std::time::Instant,
}

impl McpServiceImpl {
    /// Build one gRPC service implementation from the host runtime and stream manager.
    /// 基于宿主运行时与流管理器构建一个 gRPC 服务实现。
    pub fn new(runtime: HostRuntime, manager: ConnectionManager) -> Self {
        // Build the MCP dispatcher beside the runtime so only generic MCP calls use JSON-RPC routing.
        // 在运行时旁构建 MCP dispatcher，使只有通用 MCP 调用走 JSON-RPC 路由。
        let dispatcher = McpDispatcher::new(runtime.clone());
        Self {
            runtime,
            dispatcher,
            manager,
            start_time: std::time::Instant::now(),
        }
    }

    async fn dispatch_method(
        &self,
        method: &str,
        arguments: &str,
        request_context: RequestContext,
    ) -> (String, bool, String) {
        // Parse generic gRPC Call arguments into one JSON value used by the routing layer.
        // 将通用 gRPC Call 参数解析为路由层使用的 JSON 值。
        let args = match parse_grpc_call_arguments(arguments) {
            Ok(args) => args,
            Err(message) => return grpc_call_error_response(-32602, message),
        };

        // Normalize method-specific params before building the synthetic MCP message.
        // 在构造模拟 MCP 消息前规范化特定方法的参数。
        let params = if method == "tools/call" {
            match build_grpc_tools_call_params(&args) {
                Ok(params) => params,
                Err(message) => return grpc_call_error_response(-32602, message),
            }
        } else {
            args.clone()
        };

        // Build a fake MCP message and route it.
        // 构造一条模拟 MCP 消息并路由到统一处理链。
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": method,
            "params": params
        });

        let result = self
            .dispatcher
            .handle_message_with_context(&msg, request_context)
            .await;
        match result {
            Some(resp) => {
                let is_error = resp.get("error").is_some();
                let message = if is_error {
                    resp.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error")
                        .to_string()
                } else {
                    String::new()
                };
                let result_str = resp.to_string();
                (result_str, is_error, message)
            }
            None => ("{}".to_string(), false, String::new()),
        }
    }

    /// Resolve the VMM backend client for one relay call.
    /// 为单次 VMM 中转调用解析后端客户端。
    fn require_vmm_backend(&self) -> Result<VmmClient, Status> {
        self.runtime
            .resolve_vmm_backend()
            .map_err(|(_, message)| Status::failed_precondition(message))
    }
}

/// Parse the JSON argument string supplied to the generic gRPC MCP `Call` method.
/// 解析传给通用 gRPC MCP `Call` 方法的 JSON 参数字符串。
fn parse_grpc_call_arguments(arguments: &str) -> Result<Value, String> {
    if arguments.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(arguments)
        .map_err(|error| format!("Invalid McpCallRequest.arguments JSON: {error}"))
}

/// Build dispatcher-compatible params for the generic gRPC `tools/call` compatibility method.
/// 为通用 gRPC `tools/call` 兼容方法构造 dispatcher 可消费的 params。
fn build_grpc_tools_call_params(args: &Value) -> Result<Value, String> {
    // Extract the required tool name so invalid requests fail before runtime lookup.
    // 提取必填工具名称，使无效请求在运行时工具查找前失败。
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            "McpCallRequest.arguments for tools/call requires string field: name".to_string()
        })?;
    // Preserve caller-supplied tool arguments while defaulting the optional arguments field.
    // 保留调用方提供的工具参数，仅对可选 arguments 字段使用默认值。
    let arguments = args.get("arguments").cloned().unwrap_or_else(|| json!({}));
    Ok(json!({
        "name": name,
        "arguments": arguments,
    }))
}

/// Build one gRPC `Call` response tuple for JSON-RPC level request errors.
/// 为 JSON-RPC 层请求错误构造一个 gRPC `Call` 响应元组。
fn grpc_call_error_response(code: i64, message: String) -> (String, bool, String) {
    let result = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "error": {
            "code": code,
            "message": message,
        }
    })
    .to_string();
    (result, true, message)
}

/// Serialize one transport payload into a JSON string for public gRPC responses.
/// 将一个传输载荷序列化为公开 gRPC 响应使用的 JSON 字符串。
fn serialize_grpc_json<T: serde::Serialize>(value: &T, field_name: &str) -> Result<String, Status> {
    serde_json::to_string(value)
        .map_err(|error| Status::internal(format!("Failed to serialize {field_name}: {error}")))
}

/// Parse a JSON-encoded tool registry snapshot.
/// 解析 JSON 编码的 tool 注册表快照。
fn parse_tool_registry_snapshot(
    payload: &str,
    field_name: &str,
) -> Result<ToolRegistrySnapshot, Status> {
    serde_json::from_str(payload)
        .map_err(|error| Status::invalid_argument(format!("Invalid {field_name} payload: {error}")))
}

/// Parse an optional refresh-mode string used by public gRPC callers.
/// 解析公开 gRPC 调用方传入的可选刷新模式字符串。
fn parse_optional_refresh_mode(value: &str) -> Result<Option<ToolRefreshMode>, Status> {
    let Some(value) = optional_string(value.to_string()) else {
        return Ok(None);
    };
    match value.as_str() {
        "dynamic" => Ok(Some(ToolRefreshMode::Dynamic)),
        "restart-required" => Ok(Some(ToolRefreshMode::RestartRequired)),
        "unsupported" => Ok(Some(ToolRefreshMode::Unsupported)),
        other => Err(Status::invalid_argument(format!(
            "Unknown refresh_mode: {other}"
        ))),
    }
}

/// Convert one refresh mode into its public gRPC string token.
/// 将刷新模式转换为公开 gRPC 字符串标记。
fn tool_refresh_mode_to_grpc(mode: ToolRefreshMode) -> &'static str {
    match mode {
        ToolRefreshMode::Dynamic => "dynamic",
        ToolRefreshMode::RestartRequired => "restart-required",
        ToolRefreshMode::Unsupported => "unsupported",
    }
}

/// Convert one identity mode into its public gRPC string token.
/// 将身份模式转换为公开 gRPC 字符串标记。
fn identity_mode_to_grpc(mode: HostAdapterIdentityMode) -> &'static str {
    match mode {
        HostAdapterIdentityMode::NativeSession => "native-session",
        HostAdapterIdentityMode::SessionOrWorkmem => "session-or-workmem",
        HostAdapterIdentityMode::WorkmemOnly => "workmem-only",
    }
}

/// Convert one WorkMem source into its public gRPC string token.
/// 将 WorkMem 来源转换为公开 gRPC 字符串标记。
fn workmem_source_to_grpc(source: WorkmemIdSource) -> &'static str {
    match source {
        WorkmemIdSource::SessionId => "session-id",
        WorkmemIdSource::ProvidedWorkmemId => "provided-workmem-id",
        WorkmemIdSource::GeneratedFromWorkspace => "generated-from-workspace",
        WorkmemIdSource::Missing => "missing",
    }
}

/// Convert one notice severity into its public gRPC string token.
/// 将提示严重级别转换为公开 gRPC 字符串标记。
fn notice_severity_to_grpc(severity: ToolRefreshNoticeSeverity) -> &'static str {
    match severity {
        ToolRefreshNoticeSeverity::None => "none",
        ToolRefreshNoticeSeverity::Info => "info",
        ToolRefreshNoticeSeverity::Warning => "warning",
        ToolRefreshNoticeSeverity::Error => "error",
    }
}

/// Extract sorted tool ids from one host-core diff bucket.
/// 从一个 host-core diff 分组中提取排序后的 tool id。
fn grpc_tool_ids(tools: &[crate::host_core::host_adapter::ToolDescriptorSnapshot]) -> Vec<String> {
    let mut ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
    ids.sort();
    ids
}

#[cfg(test)]
mod mcp_grpc_tests {
    use super::*;

    /// Empty gRPC Call arguments should preserve the proto-default empty object behavior.
    /// 验证空 gRPC Call 参数会保留 proto 默认的空对象行为。
    #[test]
    fn parse_grpc_call_arguments_accepts_empty_string_as_empty_object() {
        // Parse the default proto string value used by callers that omit arguments.
        // 解析调用方省略 arguments 时产生的 proto 默认字符串值。
        let arguments = parse_grpc_call_arguments("").expect("empty arguments should parse");

        assert_eq!(arguments, json!({}));
    }

    /// Generic gRPC tools/call params should require the same name field as MCP ToolCallRequest.
    /// 验证通用 gRPC tools/call 参数应要求与 MCP ToolCallRequest 相同的 name 字段。
    #[test]
    fn build_grpc_tools_call_params_rejects_missing_name() {
        // Build arguments without the required tool name field.
        // 构造缺少必填工具名称字段的参数。
        let arguments = json!({
            "arguments": {
                "topic": "x"
            }
        });

        // Capture the explicit params validation error.
        // 捕获显式参数校验错误。
        let error = build_grpc_tools_call_params(&arguments).expect_err("missing name should fail");

        assert!(error.contains("requires string field: name"));
    }

    /// Generic gRPC tools/call params should preserve the requested tool name and arguments.
    /// 验证通用 gRPC tools/call 参数应保留请求的工具名称与参数。
    #[test]
    fn build_grpc_tools_call_params_preserves_name_and_arguments() {
        // Build one valid tools/call argument payload.
        // 构造一份合法的 tools/call 参数载荷。
        let arguments = json!({
            "name": "vulcan-help-list",
            "arguments": {
                "topic": "runtime"
            }
        });

        // Build the normalized dispatcher params for a valid tools/call request.
        // 为合法 tools/call 请求构造规范化后的 dispatcher 参数。
        let params =
            build_grpc_tools_call_params(&arguments).expect("valid tools/call params should build");

        assert_eq!(params["name"].as_str(), Some("vulcan-help-list"));
        assert_eq!(params["arguments"]["topic"].as_str(), Some("runtime"));
    }

    /// Invalid non-empty gRPC Call arguments should become a structured JSON-RPC error.
    /// 验证非空无效 gRPC Call 参数会转换为结构化 JSON-RPC 错误。
    #[tokio::test]
    async fn dispatch_method_rejects_invalid_arguments_json() {
        // Build the generic gRPC MCP service around an empty host runtime.
        // 基于空宿主运行时构造通用 gRPC MCP 服务。
        let service = McpServiceImpl::new(HostRuntime::new(), ConnectionManager::new());

        // Dispatch a non-empty malformed JSON arguments payload.
        // 分发一段非空且格式错误的 JSON 参数载荷。
        let (result, is_error, message) = service
            .dispatch_method("tools/list", "{", RequestContext::default())
            .await;
        let parsed: Value = serde_json::from_str(&result).expect("result should be valid JSON");

        assert!(is_error);
        assert!(message.contains("Invalid McpCallRequest.arguments JSON"));
        assert_eq!(parsed["error"]["code"].as_i64(), Some(-32602));
        assert_eq!(parsed["error"]["message"].as_str(), Some(message.as_str()));
    }

    /// Generic gRPC tools/call should reject missing names before runtime tool lookup.
    /// 验证通用 gRPC tools/call 会在运行时工具查找前拒绝缺失名称。
    #[tokio::test]
    async fn dispatch_method_rejects_tools_call_without_name() {
        // Build the generic gRPC MCP service around an empty host runtime.
        // 基于空宿主运行时构造通用 gRPC MCP 服务。
        let service = McpServiceImpl::new(HostRuntime::new(), ConnectionManager::new());

        // Dispatch a tools/call payload without a tool name.
        // 分发一段没有工具名称的 tools/call 载荷。
        let (result, is_error, message) = service
            .dispatch_method("tools/call", "{}", RequestContext::default())
            .await;
        // Parse the returned JSON-RPC error for status-code assertions.
        // 解析返回的 JSON-RPC 错误以断言状态码。
        let parsed: Value = serde_json::from_str(&result).expect("result should be valid JSON");

        assert!(is_error);
        assert!(message.contains("requires string field: name"));
        assert_eq!(parsed["error"]["code"].as_i64(), Some(-32602));
    }

    /// Valid dispatcher responses should be returned as JSON without an empty-string fallback.
    /// 验证合法 dispatcher 响应会以 JSON 返回，而不是依赖空字符串兜底。
    #[tokio::test]
    async fn dispatch_method_serializes_dispatcher_response_json() {
        // Build the generic gRPC MCP service around an empty host runtime.
        // 基于空宿主运行时构造通用 gRPC MCP 服务。
        let service = McpServiceImpl::new(HostRuntime::new(), ConnectionManager::new());

        // Dispatch a tools/list call with explicit empty arguments.
        // 使用显式空对象参数分发 tools/list 调用。
        let (result, is_error, message) = service
            .dispatch_method("tools/list", "{}", RequestContext::default())
            .await;
        let parsed: Value = serde_json::from_str(&result).expect("result should be valid JSON");

        assert!(!is_error);
        assert!(message.is_empty());
        assert_eq!(parsed["jsonrpc"].as_str(), Some("2.0"));
        assert!(parsed.get("result").is_some());
    }
}

#[cfg(test)]
mod host_adapter_grpc_tests {
    use super::*;

    /// Verify public gRPC refresh-mode parsing accepts every stable token.
    /// 验证公开 gRPC 刷新模式解析接受每个稳定标记。
    #[test]
    fn host_adapter_grpc_parses_refresh_mode_tokens() {
        assert_eq!(
            parse_optional_refresh_mode("dynamic").unwrap(),
            Some(ToolRefreshMode::Dynamic)
        );
        assert_eq!(
            parse_optional_refresh_mode("restart-required").unwrap(),
            Some(ToolRefreshMode::RestartRequired)
        );
        assert_eq!(
            parse_optional_refresh_mode("unsupported").unwrap(),
            Some(ToolRefreshMode::Unsupported)
        );
        assert_eq!(parse_optional_refresh_mode(" ").unwrap(), None);
    }

    /// Verify public gRPC refresh-mode parsing rejects unknown tokens.
    /// 验证公开 gRPC 刷新模式解析会拒绝未知标记。
    #[test]
    fn host_adapter_grpc_rejects_unknown_refresh_mode() {
        assert!(parse_optional_refresh_mode("live-magic").is_err());
    }

    /// Verify enum shortcut fields keep the public string contract stable.
    /// 验证枚举快捷字段保持公开字符串契约稳定。
    #[test]
    fn host_adapter_grpc_keeps_public_string_tokens() {
        assert_eq!(
            tool_refresh_mode_to_grpc(ToolRefreshMode::RestartRequired),
            "restart-required"
        );
        assert_eq!(
            identity_mode_to_grpc(HostAdapterIdentityMode::SessionOrWorkmem),
            "session-or-workmem"
        );
        assert_eq!(
            workmem_source_to_grpc(WorkmemIdSource::GeneratedFromWorkspace),
            "generated-from-workspace"
        );
        assert_eq!(
            notice_severity_to_grpc(ToolRefreshNoticeSeverity::Warning),
            "warning"
        );
    }

    /// Verify VMM status is exposed and memory tools stay hidden when VMM is disabled.
    /// 验证 VMM 状态会被公开，且 VMM 关闭时记忆工具不会暴露。
    #[tokio::test]
    async fn host_adapter_grpc_hides_vmm_tools_when_backend_disabled() {
        let service = McpServiceImpl::new(HostRuntime::new(), ConnectionManager::new());
        let context = Some(pb::HostAdapterClientContext {
            client_name: "opencode".to_string(),
            client_version: "test".to_string(),
            request_id: "vmm-disabled-test".to_string(),
        });

        let status = service
            .get_vmm_status(Request::new(HostAdapterVmmStatusRequest {
                context: context.clone(),
            }))
            .await
            .expect("VMM status request should succeed")
            .into_inner();
        assert!(!status.vmm_enabled);
        assert!(status.vmm_status.contains("not configured"));

        let tools = service
            .list_vmm_memory_tools(Request::new(HostAdapterListVmmMemoryToolsRequest {
                context,
            }))
            .await
            .expect("VMM memory metadata request should succeed")
            .into_inner();
        assert!(!tools.vmm_enabled);
        assert!(tools.tools.is_empty());
    }

    /// Build one isolated gRPC fixture with a complete LuaSkills engine and formal ROOT layer.
    /// 构建一个带完整 LuaSkills 引擎和正式 ROOT 层的隔离 gRPC 夹具。
    /// Returns the fixture directory and service; callers must remove the directory after the assertion.
    /// 返回夹具目录与服务；调用方必须在断言后删除该目录。
    fn build_runtime_config_grpc_service() -> (std::path::PathBuf, McpServiceImpl) {
        let unique = format!(
            "vulcan-agent-service-runtime-config-grpc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        let fixture_root = std::env::temp_dir().join(unique);
        let application_root = fixture_root.join("application");
        let root_skills = fixture_root.join("root").join("skills");
        std::fs::create_dir_all(application_root.join("lua_runtime"))
            .expect("gRPC runtime fixture should create the Lua runtime root");
        std::fs::create_dir_all(&root_skills)
            .expect("gRPC runtime fixture should create the formal ROOT skills directory");
        let config = crate::config::Config {
            runtime_root: Some(application_root.to_string_lossy().to_string()),
            skill_config_root: Some(
                fixture_root
                    .join("skill-config")
                    .to_string_lossy()
                    .to_string(),
            ),
            ..crate::config::Config::default()
        };
        let runtime = HostRuntime::new()
            .with_lua_skills(
                &config,
                &[::luaskills::RuntimeSkillRoot {
                    name: "ROOT".to_string(),
                    skills_dir: root_skills,
                }],
                ::luaskills::LuaVmPoolConfig {
                    min_size: 1,
                    max_size: 1,
                    idle_ttl_secs: 60,
                },
                ::luaskills::ToolCacheConfig::default(),
            )
            .expect("gRPC runtime-config fixture should initialize LuaSkills");
        (
            fixture_root,
            McpServiceImpl::new(runtime, ConnectionManager::new()),
        )
    }

    /// RuntimeConfig must reject requests that omit the trusted client context.
    /// RuntimeConfig 必须拒绝缺少受信任客户端上下文的请求。
    #[tokio::test]
    async fn runtime_config_grpc_requires_trusted_client_context() {
        let service = McpServiceImpl::new(HostRuntime::new(), ConnectionManager::new());

        let error = LuaSkillsService::runtime_config(
            &service,
            Request::new(LuaSkillRuntimeConfigRequest {
                context: None,
                request_json: "{}".to_string(),
            }),
        )
        .await
        .expect_err("runtime-config gRPC should reject a missing context");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("requires context"));
    }

    /// RuntimeConfig must preserve the upstream stable response envelope for protocol failures.
    /// RuntimeConfig 必须为协议失败保留上游稳定响应包络。
    #[tokio::test]
    async fn runtime_config_grpc_preserves_upstream_response_envelope() {
        let (fixture_root, service) = build_runtime_config_grpc_service();

        let response = LuaSkillsService::runtime_config(
            &service,
            Request::new(LuaSkillRuntimeConfigRequest {
                context: Some(pb::LuaSkillClientContext {
                    client_name: "trusted-admin-test".to_string(),
                    client_version: "0.5.5".to_string(),
                    request_id: "runtime-config-envelope".to_string(),
                }),
                request_json: "{}".to_string(),
            }),
        )
        .await
        .expect("runtime-config protocol errors should remain in the response envelope")
        .into_inner();
        let envelope: Value = serde_json::from_str(&response.response_json)
            .expect("runtime-config gRPC response should contain valid JSON");

        assert_eq!(envelope["ok"], false);
        assert!(envelope["error"]["code"].is_string());
        assert!(envelope["error"]["message"].is_string());
        let _ = std::fs::remove_dir_all(fixture_root);
    }
}
