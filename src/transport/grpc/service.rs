use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::backends::vmm::grpc_client::VmmClient;
use crate::backends::vmm::tool_metadata::vmm_memory_tool_descriptors;
use crate::host_core::HostRuntime;
use crate::host_core::host_adapter::{
    HostAdapterIdentityMode, HostAdapterRuntimeInput, ToolRefreshMode, ToolRefreshNoticeSeverity,
    ToolRegistryDiffOptions, ToolRegistrySnapshot, WorkmemIdSource, diff_tool_registry_snapshots,
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
    mcp_error_to_status, optional_str, optional_string, parse_json_arguments,
    require_luaskill_context, text_response, tool_call_result_to_call_response,
    tool_call_result_to_text_response,
};
use pb::host_adapter_service_server::{HostAdapterService, HostAdapterServiceServer};
use pb::lua_skills_service_server::{LuaSkillsService, LuaSkillsServiceServer};
use pb::mcp_service_server::{McpService, McpServiceServer};
use pb::{
    ConnectEvent, ConnectRequest, HealthzResponse, HeartbeatEvent,
    HostAdapterDiffToolRegistryRequest, HostAdapterDiffToolRegistryResponse,
    HostAdapterListVmmMemoryToolsRequest, HostAdapterListVmmMemoryToolsResponse,
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

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn healthz(&self, _request: Request<()>) -> Result<Response<HealthzResponse>, Status> {
        let _uptime = self.start_time.elapsed().as_secs();
        Ok(Response::new(HealthzResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
        }))
    }

    async fn call(
        &self,
        request: Request<McpCallRequest>,
    ) -> Result<Response<McpCallResponse>, Status> {
        let req = request.into_inner();
        eprintln!(
            "[gRPC] Call: method={} project_id={} user_id={} client_name={}",
            req.method, req.project_id, req.user_id, req.client_name
        );

        let request_context = build_mcp_call_request_context(&req);
        let (result, is_error, message) = self
            .dispatch_method(&req.method, &req.arguments, request_context)
            .await;

        Ok(Response::new(McpCallResponse {
            result,
            is_error,
            message,
        }))
    }

    type ConnectStream =
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<ConnectEvent, Status>> + Send>>;

    async fn connect(
        &self,
        request: Request<ConnectRequest>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let req = request.into_inner();
        let heartbeat_ms = if req.heartbeat_interval_ms > 0 {
            req.heartbeat_interval_ms as u64
        } else {
            30000
        };

        let (session_id, mut rx) = self.manager.register(&req.client_name).await;
        let manager = self.manager.clone();

        // Send welcome event
        let welcome = ConnectEvent {
            event: Some(ConnectEventType::Welcome(WelcomeEvent {
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                session_id: session_id.clone(),
                protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
            })),
        };

        let heartbeat_interval = std::time::Duration::from_millis(heartbeat_ms);
        let mut heartbeat_stream =
            tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(heartbeat_interval));

        let output = async_stream::stream! {
            // Send welcome first
            yield Ok(welcome);

            loop {
                tokio::select! {
                    // Heartbeat tick
                    _ = heartbeat_stream.next() => {
                        yield Ok(ConnectEvent {
                            event: Some(ConnectEventType::Heartbeat(HeartbeatEvent {
                                timestamp_ms: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as i64,
                                uptime_sec: 0,
                            })),
                        });
                    }
                    // Incoming call event from client (via separate channel)
                    event = rx.recv() => {
                        match event {
                            Some(evt) => yield Ok(evt),
                            None => {
                                eprintln!("[gRPC] Stream receiver dropped for {}", session_id);
                                break;
                            }
                        }
                    }
                }
            }

            // Cleanup on disconnect
            manager.unregister(&session_id).await;
        };

        Ok(Response::new(Box::pin(output) as Self::ConnectStream))
    }
}

#[tonic::async_trait]
impl VmmService for McpServiceImpl {
    async fn healthz(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::HealthzResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_healthz().await?))
    }

    async fn list_projects(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::ListProjectsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_list_projects().await?))
    }

    async fn resolve_project(
        &self,
        request: Request<vmm_pb::ResolveProjectRequest>,
    ) -> Result<Response<vmm_pb::ResolveProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_resolve_project(request.into_inner()).await?,
        ))
    }

    async fn ensure_project(
        &self,
        request: Request<vmm_pb::EnsureProjectRequest>,
    ) -> Result<Response<vmm_pb::EnsureProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_ensure_project(request.into_inner()).await?,
        ))
    }

    async fn delete_project(
        &self,
        request: Request<vmm_pb::DeleteProjectRequest>,
    ) -> Result<Response<vmm_pb::DeleteProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_delete_project(request.into_inner()).await?,
        ))
    }

    async fn migrate_project(
        &self,
        request: Request<vmm_pb::MigrateProjectRequest>,
    ) -> Result<Response<vmm_pb::MigrateProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_migrate_project(request.into_inner()).await?,
        ))
    }

    async fn resolve_user(
        &self,
        request: Request<vmm_pb::ResolveUserRequest>,
    ) -> Result<Response<vmm_pb::ResolveUserResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_resolve_user(request.into_inner()).await?,
        ))
    }

    async fn list_users(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::ListUsersResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_list_users().await?))
    }

    async fn delete_user(
        &self,
        request: Request<vmm_pb::DeleteUserRequest>,
    ) -> Result<Response<vmm_pb::DeleteUserResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_delete_user(request.into_inner()).await?,
        ))
    }

    async fn get_profile_nodes(
        &self,
        request: Request<vmm_pb::GetProfileNodesRequest>,
    ) -> Result<Response<vmm_pb::GetProfileNodesResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_profile_nodes(request.into_inner())
                .await?,
        ))
    }

    async fn get_profile_bundle(
        &self,
        request: Request<vmm_pb::GetProfileBundleRequest>,
    ) -> Result<Response<vmm_pb::GetProfileBundleResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_profile_bundle(request.into_inner())
                .await?,
        ))
    }

    async fn apply_profile_instruction(
        &self,
        request: Request<vmm_pb::ApplyProfileInstructionRequest>,
    ) -> Result<Response<vmm_pb::ApplyProfileInstructionResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_apply_profile_instruction(request.into_inner())
                .await?,
        ))
    }

    async fn search_memory_events(
        &self,
        request: Request<vmm_pb::SearchMemoryEventsRequest>,
    ) -> Result<Response<vmm_pb::SearchMemoryEventsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_search_memory_events(request.into_inner())
                .await?,
        ))
    }

    async fn get_turn_details(
        &self,
        request: Request<vmm_pb::GetTurnDetailsRequest>,
    ) -> Result<Response<vmm_pb::GetTurnDetailsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_turn_details(request.into_inner())
                .await?,
        ))
    }

    async fn write_memories(
        &self,
        request: Request<vmm_pb::WriteMemoriesRequest>,
    ) -> Result<Response<vmm_pb::WriteMemoriesResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_write_memories(request.into_inner()).await?,
        ))
    }

    async fn chat_compact(
        &self,
        request: Request<vmm_pb::ChatCompactRequest>,
    ) -> Result<Response<vmm_pb::ChatCompactResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_chat_compact(request.into_inner()).await?,
        ))
    }

    async fn pre_check(
        &self,
        request: Request<vmm_pb::PreCheckRequest>,
    ) -> Result<Response<vmm_pb::PreCheckResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_pre_check(request.into_inner()).await?,
        ))
    }

    async fn post_action(
        &self,
        request: Request<vmm_pb::PostActionRequest>,
    ) -> Result<Response<vmm_pb::PostActionResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_post_action(request.into_inner()).await?,
        ))
    }
}

#[tonic::async_trait]
impl LuaSkillsService for McpServiceImpl {
    async fn list_skills(
        &self,
        request: Request<LuaSkillListSkillsRequest>,
    ) -> Result<Response<LuaSkillListSkillsResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let skills = self
            .runtime
            .list_luaskill_packages()
            .await
            .map_err(mcp_error_to_status)?
            .iter()
            .map(lua_skill_package_to_pb)
            .collect();
        Ok(Response::new(LuaSkillListSkillsResponse { skills }))
    }

    async fn list_tools(
        &self,
        request: Request<LuaSkillListToolsRequest>,
    ) -> Result<Response<LuaSkillListToolsResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let tools = self
            .runtime
            .list_luaskill_tools()
            .await
            .map_err(mcp_error_to_status)?
            .iter()
            .map(lua_skill_tool_to_pb)
            .collect();
        Ok(Response::new(LuaSkillListToolsResponse { tools }))
    }

    async fn get_tool(
        &self,
        request: Request<LuaSkillGetToolRequest>,
    ) -> Result<Response<LuaSkillGetToolResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let tool = self
            .runtime
            .get_luaskill_tool(&req.tool_name)
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(LuaSkillGetToolResponse {
            tool: Some(lua_skill_tool_to_pb(&tool)),
        }))
    }

    async fn call_tool(
        &self,
        request: Request<LuaSkillCallToolRequest>,
    ) -> Result<Response<LuaSkillCallToolResponse>, Status> {
        let req = request.into_inner();
        let context = require_luaskill_context(req.context.as_ref())?;
        let arguments = parse_json_arguments(&req.arguments_json)?;
        let result = self
            .runtime
            .call_luaskill_tool(
                &req.tool_name,
                arguments,
                &context.client_name,
                optional_str(&context.client_version),
                optional_str(&context.request_id),
            )
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_call_response(&result)))
    }

    async fn list_help(
        &self,
        request: Request<LuaSkillListHelpRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .list_luaskill_help()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn get_help(
        &self,
        request: Request<LuaSkillGetHelpRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .get_luaskill_help(
                &req.skill_id,
                &req.flow,
                &context.client_name,
                optional_str(&context.client_version),
                optional_str(&context.request_id),
            )
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn list_skill_config(
        &self,
        request: Request<LuaSkillConfigListRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let skill_id = optional_string(req.skill_id);
        let text = self
            .runtime
            .list_luaskill_config(skill_id)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn get_skill_config(
        &self,
        request: Request<LuaSkillConfigGetRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .get_luaskill_config(req.skill_id, req.key)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn set_skill_config(
        &self,
        request: Request<LuaSkillConfigSetRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .set_luaskill_config(req.skill_id, req.key, req.value)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn delete_skill_config(
        &self,
        request: Request<LuaSkillConfigDeleteRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .delete_luaskill_config(req.skill_id, req.key)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn list_installed_skills(
        &self,
        request: Request<LuaSkillListInstalledSkillsRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .list_installed_luaskills()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn install_skill(
        &self,
        request: Request<LuaSkillInstallRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .install_luaskill(req.source, optional_string(req.source_type))
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn update_skill(
        &self,
        request: Request<LuaSkillUpdateRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .update_luaskill(req.skill_id)
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn uninstall_skill(
        &self,
        request: Request<LuaSkillUninstallRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .uninstall_luaskill(req.skill_id)
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn reload_runtime_configs(
        &self,
        request: Request<LuaSkillReloadRuntimeConfigsRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .reload_luaskill_runtime_configs()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }
}

#[tonic::async_trait]
impl HostAdapterService for McpServiceImpl {
    /// Return one normalized host adapter descriptor and capability profile.
    /// 返回一个归一化宿主适配器描述与能力画像。
    async fn get_host_adapter_profile(
        &self,
        request: Request<HostAdapterProfileRequest>,
    ) -> Result<Response<HostAdapterProfileResponse>, Status> {
        let req = request.into_inner();
        let adapter = self
            .runtime
            .describe_host_adapter(optional_str(&req.host_kind));
        let adapter_json = serialize_grpc_json(&adapter, "adapter_json")?;
        let profile_json = serialize_grpc_json(&adapter.profile, "profile_json")?;

        Ok(Response::new(HostAdapterProfileResponse {
            adapter_json,
            profile_json,
            host_kind: adapter.host_kind.as_str().to_string(),
            display_name: adapter.profile.display_name,
            refresh_mode: tool_refresh_mode_to_grpc(adapter.refresh_mode).to_string(),
            identity_mode: identity_mode_to_grpc(adapter.identity_mode).to_string(),
            is_error: false,
            message: String::new(),
            vmm_enabled: self.runtime.is_vmm_backend_enabled(),
            vmm_status: self.runtime.vmm_backend_status_message().to_string(),
        }))
    }

    /// Normalize one host adapter runtime context.
    /// 归一化一份宿主适配器运行时上下文。
    async fn build_host_adapter_runtime(
        &self,
        request: Request<HostAdapterRuntimeRequest>,
    ) -> Result<Response<HostAdapterRuntimeResponse>, Status> {
        let req = request.into_inner();
        let runtime = self
            .runtime
            .build_host_adapter_runtime(HostAdapterRuntimeInput {
                host_kind: optional_string(req.host_kind),
                adapter_host_kind: optional_string(req.adapter_host_kind),
                session_id: optional_string(req.session_id),
                workmem_id: optional_string(req.workmem_id),
                turn_id: optional_string(req.turn_id),
                workspace: optional_string(req.workspace),
                user_message: optional_string(req.user_message),
                conversation_id: optional_string(req.conversation_id),
                root_session_id: optional_string(req.root_session_id),
            });
        let runtime_json = serialize_grpc_json(&runtime, "runtime_json")?;

        Ok(Response::new(HostAdapterRuntimeResponse {
            runtime_json,
            host_kind: runtime.context.host_kind.as_str().to_string(),
            session_id: runtime.context.session_id.unwrap_or_default(),
            workmem_id: runtime.context.workmem_id.unwrap_or_default(),
            workmem_source: workmem_source_to_grpc(runtime.context.workmem_source).to_string(),
            identity_ready: runtime.identity_ready,
            degraded_reasons: runtime.degraded_reasons,
            is_error: false,
            message: String::new(),
            vmm_enabled: self.runtime.is_vmm_backend_enabled(),
            vmm_status: self.runtime.vmm_backend_status_message().to_string(),
        }))
    }

    /// Compare two tool registry snapshots and return restart guidance.
    /// 对比两份 tool 注册表快照并返回重启提示。
    async fn diff_tool_registry(
        &self,
        request: Request<HostAdapterDiffToolRegistryRequest>,
    ) -> Result<Response<HostAdapterDiffToolRegistryResponse>, Status> {
        let req = request.into_inner();
        let previous =
            parse_tool_registry_snapshot(&req.previous_snapshot_json, "previous_snapshot_json")?;
        let next = parse_tool_registry_snapshot(&req.next_snapshot_json, "next_snapshot_json")?;
        let refresh_mode = parse_optional_refresh_mode(&req.refresh_mode)?;
        let diff = diff_tool_registry_snapshots(
            &previous,
            &next,
            &ToolRegistryDiffOptions {
                refresh_mode,
                dynamic_tool_refresh_supported: req
                    .has_dynamic_tool_refresh_supported
                    .then_some(req.dynamic_tool_refresh_supported),
                host_restart_required: req.host_restart_required,
            },
        )
        .map_err(Status::invalid_argument)?;
        let diff_json = serialize_grpc_json(&diff, "diff_json")?;

        Ok(Response::new(HostAdapterDiffToolRegistryResponse {
            diff_json,
            changed_tool_ids: diff.changed_tool_ids,
            added_tool_ids: grpc_tool_ids(&diff.added),
            removed_tool_ids: grpc_tool_ids(&diff.removed),
            updated_tool_ids: grpc_tool_ids(&diff.updated),
            restart_required: diff.restart_required,
            summary: diff.summary,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Build model-facing and user-facing refresh guidance from tool snapshots.
    /// 根据 tool 快照构建面向模型与用户的刷新提示。
    async fn build_tool_refresh_notice(
        &self,
        request: Request<HostAdapterToolRefreshNoticeRequest>,
    ) -> Result<Response<HostAdapterToolRefreshNoticeResponse>, Status> {
        let req = request.into_inner();
        let previous =
            parse_tool_registry_snapshot(&req.previous_snapshot_json, "previous_snapshot_json")?;
        let next = parse_tool_registry_snapshot(&req.next_snapshot_json, "next_snapshot_json")?;
        let adapter = self
            .runtime
            .describe_host_adapter(optional_str(&req.host_kind));
        let notice = self
            .runtime
            .build_tool_refresh_notice_for_adapter(previous, next, Some(adapter.host_kind.as_str()))
            .map_err(Status::invalid_argument)?;
        let notice_json = serialize_grpc_json(&notice, "notice_json")?;

        Ok(Response::new(HostAdapterToolRefreshNoticeResponse {
            notice_json,
            changed: notice.changed,
            refresh_mode: tool_refresh_mode_to_grpc(notice.refresh_mode).to_string(),
            severity: notice_severity_to_grpc(notice.severity).to_string(),
            restart_required: notice.restart_required,
            changed_tool_ids: notice.changed_tool_ids,
            model_message: notice.model_message,
            user_message: notice.user_message,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Return whether the host runtime has an enabled VMM backend.
    /// 返回当前宿主运行时是否启用了 VMM 后端。
    async fn get_vmm_status(
        &self,
        request: Request<HostAdapterVmmStatusRequest>,
    ) -> Result<Response<HostAdapterVmmStatusResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        Ok(Response::new(HostAdapterVmmStatusResponse {
            vmm_enabled,
            vmm_status,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Return stable VMM memory tool metadata for host plugin registration.
    /// 返回宿主插件注册工具时使用的稳定 VMM 记忆工具元信息。
    async fn list_vmm_memory_tools(
        &self,
        request: Request<HostAdapterListVmmMemoryToolsRequest>,
    ) -> Result<Response<HostAdapterListVmmMemoryToolsResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        // Do not expose VMM tools when the VMM backend is not enabled.
        // 当 VMM 后端未启用时，不向宿主暴露任何 VMM 工具。
        if !vmm_enabled {
            return Ok(Response::new(HostAdapterListVmmMemoryToolsResponse {
                tools: Vec::new(),
                is_error: false,
                message: vmm_status.clone(),
                vmm_enabled,
                vmm_status,
            }));
        }

        // Tool descriptors are available once the VMM backend is enabled; actual
        // health errors are still surfaced by each VMM relay call.
        // VMM 后端启用后即可暴露工具描述；
        // 具体健康错误仍由每次 VMM 中转调用自行返回。
        let tools = vmm_memory_tool_descriptors()
            .into_iter()
            .map(|descriptor| HostAdapterToolDescriptor {
                name: descriptor.name,
                description: descriptor.description,
                input_schema_json: descriptor.input_schema_json,
                annotations_json: descriptor.annotations_json,
                source: descriptor.source,
            })
            .collect();

        Ok(Response::new(HostAdapterListVmmMemoryToolsResponse {
            tools,
            is_error: false,
            message: String::new(),
            vmm_enabled,
            vmm_status,
        }))
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

// gRPC server runner
// ============================================================

pub async fn run_grpc(server: HostRuntime, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = addr.parse()?;
    let manager = ConnectionManager::new();
    let service = McpServiceImpl::new(server, manager);

    eprintln!("[gRPC] Starting gRPC server on http://{} ...", addr);
    eprintln!("[gRPC]   Healthz    Healthz");
    eprintln!("[gRPC]   Call       Unary tool/method invocation");
    eprintln!("[gRPC]   Connect    Long-lived streaming connection with heartbeat");
    eprintln!("[gRPC]   LuaSkills Stable LuaSkills tool and management API");
    eprintln!("[gRPC]   VMM        VMM service relay API");

    Server::builder()
        .add_service(McpServiceServer::new(service.clone()))
        .add_service(LuaSkillsServiceServer::new(service.clone()))
        .add_service(HostAdapterServiceServer::new(service.clone()))
        .add_service(VmmServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
