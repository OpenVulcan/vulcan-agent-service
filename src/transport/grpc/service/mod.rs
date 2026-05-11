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
    LuaSkillConfigDeleteRequest, LuaSkillConfigGetRequest, LuaSkillConfigListRequest,
    LuaSkillConfigSetRequest, LuaSkillGetHelpRequest, LuaSkillGetToolRequest,
    LuaSkillGetToolResponse, LuaSkillInstallRequest, LuaSkillListHelpRequest,
    LuaSkillListInstalledSkillsRequest, LuaSkillListSkillsRequest, LuaSkillListSkillsResponse,
    LuaSkillListToolsRequest, LuaSkillListToolsResponse, LuaSkillReloadRuntimeConfigsRequest,
    LuaSkillTextResponse, LuaSkillUninstallRequest, LuaSkillUpdateRequest, McpCallRequest,
    McpCallResponse, WelcomeEvent, connect_event::Event as ConnectEventType,
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
        let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));

        // Build a fake MCP message and route it.
        // 构造一条模拟 MCP 消息并路由到统一处理链。
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": method,
            "params": if method == "tools/call" {
                json!({
                    "name": args.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "arguments": args.get("arguments").cloned().unwrap_or(json!({}))
                })
            } else {
                args.clone()
            }
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
                let result_str = serde_json::to_string(&resp).unwrap_or_default();
                (result_str, is_error, message)
            }
            None => (
                serde_json::to_string(&json!({})).unwrap(),
                false,
                String::new(),
            ),
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
}
