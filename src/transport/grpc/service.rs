use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::host_core::HostRuntime;
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
use pb::lua_skills_service_server::{LuaSkillsService, LuaSkillsServiceServer};
use pb::mcp_service_server::{McpService, McpServiceServer};
use pb::{
    ConnectEvent, ConnectRequest, HealthzResponse, HeartbeatEvent, LuaSkillCallToolRequest,
    LuaSkillCallToolResponse, LuaSkillConfigDeleteRequest, LuaSkillConfigGetRequest,
    LuaSkillConfigListRequest, LuaSkillConfigSetRequest, LuaSkillGetHelpRequest,
    LuaSkillGetToolRequest, LuaSkillGetToolResponse, LuaSkillInstallRequest,
    LuaSkillListHelpRequest, LuaSkillListInstalledSkillsRequest, LuaSkillListSkillsRequest,
    LuaSkillListSkillsResponse, LuaSkillListToolsRequest, LuaSkillListToolsResponse,
    LuaSkillReloadRuntimeConfigsRequest, LuaSkillTextResponse, LuaSkillUninstallRequest,
    LuaSkillUpdateRequest, McpCallRequest, McpCallResponse, WelcomeEvent,
    connect_event::Event as ConnectEventType,
};

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

    Server::builder()
        .add_service(McpServiceServer::new(service.clone()))
        .add_service(LuaSkillsServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
