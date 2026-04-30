use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::protocol::PROTOCOL_VERSION_LATEST;
use crate::protocol::{ClientInfo, RequestContext, ToolCallResult};
use crate::server::{
    LuaSkillPackageDescriptor, LuaSkillToolDescriptor as RuntimeLuaSkillToolDescriptor, McpServer,
};

pub mod pb {
    tonic::include_proto!("vulcan.mcp.v1");
}

use pb::lua_skills_service_server::{LuaSkillsService, LuaSkillsServiceServer};
use pb::mcp_service_server::{McpService, McpServiceServer};
use pb::{
    ConnectEvent, ConnectRequest, HealthzResponse, HeartbeatEvent, LuaSkillCallToolRequest,
    LuaSkillCallToolResponse, LuaSkillClientContext, LuaSkillConfigDeleteRequest,
    LuaSkillConfigGetRequest, LuaSkillConfigListRequest, LuaSkillConfigSetRequest,
    LuaSkillDescriptor, LuaSkillGetHelpRequest, LuaSkillGetToolRequest, LuaSkillGetToolResponse,
    LuaSkillInstallRequest, LuaSkillListHelpRequest, LuaSkillListInstalledSkillsRequest,
    LuaSkillListSkillsRequest, LuaSkillListSkillsResponse, LuaSkillListToolsRequest,
    LuaSkillListToolsResponse, LuaSkillReloadRuntimeConfigsRequest, LuaSkillTextResponse,
    LuaSkillToolDescriptor, LuaSkillUninstallRequest, LuaSkillUpdateRequest, McpCallRequest,
    McpCallResponse, WelcomeEvent, connect_event::Event as ConnectEventType,
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
    server: McpServer,
    manager: ConnectionManager,
    start_time: std::time::Instant,
}

impl McpServiceImpl {
    pub fn new(server: McpServer, manager: ConnectionManager) -> Self {
        Self {
            server,
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
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
            .server
            .reload_luaskill_runtime_configs()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }
}

/// Build the request context used by the legacy generic gRPC `Call` method.
/// 构造旧版通用 gRPC `Call` 方法使用的请求上下文。
fn build_mcp_call_request_context(req: &McpCallRequest) -> RequestContext {
    let client_name = optional_string(req.client_name.clone());
    let client_version = optional_string(req.client_version.clone()).unwrap_or_default();
    RequestContext {
        transport: Some("grpc_unary".to_string()),
        session_id: optional_string(req.session_id.clone()),
        client_info: client_name.as_ref().map(|name| ClientInfo {
            name: name.clone(),
            version: client_version,
        }),
        client_match_name_override: None,
        exact_client_name: client_name,
        disable_client_match_overrides: true,
        ..RequestContext::default()
    }
}

/// Require a LuaSkills gRPC client context with a non-empty exact client name.
/// 要求 LuaSkills gRPC 客户端上下文存在且包含非空精确客户端名称。
fn require_luaskill_context(
    context: Option<&LuaSkillClientContext>,
) -> Result<LuaSkillClientContext, Status> {
    let context = context
        .cloned()
        .ok_or_else(|| Status::invalid_argument("LuaSkills gRPC request requires context"))?;
    let client_name = optional_string(context.client_name.clone()).ok_or_else(|| {
        Status::invalid_argument("LuaSkills gRPC request requires context.client_name")
    })?;
    Ok(LuaSkillClientContext {
        client_name,
        client_version: optional_string(context.client_version).unwrap_or_default(),
        request_id: optional_string(context.request_id).unwrap_or_default(),
    })
}

/// Parse one JSON argument string for a dynamic LuaSkills tool call.
/// 解析动态 LuaSkills 工具调用的一段 JSON 参数字符串。
fn parse_json_arguments(arguments_json: &str) -> Result<Value, Status> {
    if arguments_json.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(arguments_json).map_err(|error| {
        Status::invalid_argument(format!("Invalid arguments_json payload: {}", error))
    })
}

/// Convert an optional protobuf string field represented as a plain string into an owned option.
/// 将以普通字符串表示的可选 protobuf 字段转换为自有 Option。
fn optional_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Convert a string option into a borrowed option for internal server calls.
/// 将字符串选项转换为内部服务调用使用的借用选项。
fn optional_str(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() { None } else { Some(value) }
}

/// Convert one loaded LuaSkill package descriptor into the protobuf response type.
/// 将一个已加载 LuaSkill 包描述转换为 protobuf 响应类型。
fn lua_skill_package_to_pb(descriptor: &LuaSkillPackageDescriptor) -> LuaSkillDescriptor {
    LuaSkillDescriptor {
        skill_id: descriptor.skill_id.clone(),
        root_name: descriptor.root_name.clone(),
        skill_dir: descriptor.skill_dir.clone(),
        tool_names: descriptor.tool_names.clone(),
    }
}

/// Convert one dynamic LuaSkill tool descriptor into the protobuf response type.
/// 将一个动态 LuaSkill 工具描述转换为 protobuf 响应类型。
fn lua_skill_tool_to_pb(descriptor: &RuntimeLuaSkillToolDescriptor) -> LuaSkillToolDescriptor {
    LuaSkillToolDescriptor {
        name: descriptor.tool.name.clone(),
        description: descriptor.tool.description.clone().unwrap_or_default(),
        input_schema_json: serde_json::to_string(&descriptor.tool.input_schema)
            .unwrap_or_else(|_| "{}".to_string()),
        annotations_json: serde_json::to_string(&descriptor.tool.annotations)
            .unwrap_or_else(|_| "null".to_string()),
        skill_id: descriptor.skill_id.clone(),
        entry_name: descriptor.entry_name.clone(),
        root_name: descriptor.root_name.clone(),
        skill_dir: descriptor.skill_dir.clone(),
    }
}

/// Convert one tool-call result into the dynamic gRPC CallTool response.
/// 将一个工具调用结果转换为动态 gRPC CallTool 响应。
fn tool_call_result_to_call_response(result: &ToolCallResult) -> LuaSkillCallToolResponse {
    let text = tool_call_result_text(result);
    let is_error = result.is_error.unwrap_or(false);
    LuaSkillCallToolResponse {
        result_json: serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string()),
        text: text.clone(),
        is_error,
        message: if is_error { text } else { String::new() },
    }
}

/// Convert one tool-call result into a stable text response.
/// 将一个工具调用结果转换为稳定文本响应。
fn tool_call_result_to_text_response(result: &ToolCallResult) -> LuaSkillTextResponse {
    let text = tool_call_result_text(result);
    let is_error = result.is_error.unwrap_or(false);
    LuaSkillTextResponse {
        text: text.clone(),
        is_error,
        message: if is_error { text } else { String::new() },
    }
}

/// Convert a successful string payload into a stable text response.
/// 将成功字符串载荷转换为稳定文本响应。
fn text_response(text: String) -> LuaSkillTextResponse {
    LuaSkillTextResponse {
        text,
        is_error: false,
        message: String::new(),
    }
}

/// Join the text blocks inside one MCP-compatible tool result.
/// 拼接一个 MCP 兼容工具结果内的文本块。
fn tool_call_result_text(result: &ToolCallResult) -> String {
    result
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Map an internal JSON-RPC style error tuple to an idiomatic gRPC status.
/// 将内部 JSON-RPC 风格错误元组映射为惯用 gRPC 状态。
fn mcp_error_to_status(error: (i64, String)) -> Status {
    match error.0 {
        -32601 => Status::not_found(error.1),
        -32602 => Status::invalid_argument(error.1),
        -32603 => Status::internal(error.1),
        _ => Status::unknown(error.1),
    }
}

// ============================================================
// gRPC server runner
// ============================================================

pub async fn run_grpc(server: McpServer, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
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
