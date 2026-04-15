use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

use crate::protocol::{
    InitializeRequest, PROTOCOL_VERSION_LATEST, RequestContext, negotiate_version,
};
use crate::server::McpServer;
use crate::session::{SessionManager, SseSessionManager};

// ============================================================
// Application state shared with axum handlers / Axum 处理器共享应用状态
// ============================================================

#[derive(Clone)]
pub struct AppState {
    pub server: McpServer,
    pub sessions: SessionManager,
    pub sse_sessions: SseSessionManager,
}

// ============================================================
// HTTP transport runner / HTTP 传输入口
// ============================================================

pub async fn run_http(server: McpServer, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        server,
        sessions: SessionManager::new(),
        sse_sessions: SseSessionManager::new(),
    };

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        // Streamable HTTP (2025-11-25)
        .route("/mcp", get(handle_streamable_get))
        .route("/mcp", post(handle_streamable_post))
        .route("/mcp", delete(handle_streamable_delete))
        // Legacy SSE (2024-11-05 / 2025-03-26)
        .route("/sse", get(handle_sse_get))
        .route("/message", post(handle_sse_post))
        // Health check
        .route("/health", get(|| async { "OK" }))
        .with_state(state)
        .layer(cors);

    let addr: SocketAddr = addr.parse()?;
    eprintln!("[MCP] Starting HTTP transport on http://{} ...", addr);
    eprintln!("[MCP]   GET    /mcp       (HTTP Streamable SSE, 2025-11-25)");
    eprintln!("[MCP]   POST   /mcp       (HTTP Streamable JSON-RPC, 2025-11-25)");
    eprintln!("[MCP]   DELETE /mcp       (Close session, 2025-11-25)");
    eprintln!("[MCP]   GET    /sse       (Legacy SSE, 2024-11-05 / 2025-03-26)");
    eprintln!("[MCP]   POST   /message   (Legacy POST, 2024-11-05 / 2025-03-26)");
    eprintln!("[MCP]   GET    /health    (Health check)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ============================================================
// HTTP Streamable handlers (2025-11-25) / 新版 Streamable HTTP 处理器
// ============================================================

#[derive(Deserialize)]
struct McpQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

/// JSON-RPC payload kind / JSON-RPC 负载分类。
enum JsonRpcMessageKind<'a> {
    Request { method: &'a str },
    Notification { method: &'a str },
    Response,
}

#[axum::debug_handler]
async fn handle_streamable_post(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = validate_origin(&headers) {
        return resp;
    }

    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(e) => {
            return json_with_status(
                StatusCode::BAD_REQUEST,
                McpServer::parse_error(&format!("Invalid UTF-8: {}", e)),
            );
        }
    };

    let msg: Value = match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_with_status(
                StatusCode::BAD_REQUEST,
                McpServer::parse_error(&format!("Parse error: {}", e)),
            );
        }
    };

    if msg.is_array() {
        return jsonrpc_error_response(
            StatusCode::BAD_REQUEST,
            Value::Null,
            -32600,
            "Batch requests are not supported on the streamable HTTP endpoint.",
        );
    }

    let kind = match classify_jsonrpc_message(&msg) {
        Some(kind) => kind,
        None => {
            return jsonrpc_error_response(
                StatusCode::BAD_REQUEST,
                msg.get("id").cloned().unwrap_or(Value::Null),
                -32600,
                "Invalid JSON-RPC message.",
            );
        }
    };

    let session_id = extract_session_id(&query, &headers);

    match kind {
        JsonRpcMessageKind::Request { method } if method == "initialize" => {
            handle_initialize_request(state, headers, msg, session_id).await
        }
        JsonRpcMessageKind::Request { .. } => {
            handle_streamable_request(state, headers, msg, session_id).await
        }
        JsonRpcMessageKind::Notification { method } => {
            if method == "initialize" {
                return jsonrpc_error_response(
                    StatusCode::BAD_REQUEST,
                    Value::Null,
                    -32600,
                    "initialize must be sent as a request with an id.",
                );
            }
            handle_streamable_notification(state, headers, msg, session_id).await
        }
        JsonRpcMessageKind::Response => {
            handle_streamable_client_response(state, headers, msg, session_id).await
        }
    }
}

/// Handle GET /mcp as the session-bound SSE listener / 将 GET /mcp 作为绑定会话的 SSE 监听流。
async fn handle_streamable_get(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = validate_origin(&headers) {
        return resp;
    }
    if !accepts_sse(&headers) {
        return plain_response(
            StatusCode::NOT_ACCEPTABLE,
            "GET /mcp requires Accept: text/event-stream",
        );
    }

    let Some(session_id) = extract_session_id(&query, &headers) else {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "Missing MCP session id for GET /mcp.",
        );
    };

    if !state.sessions.exists(&session_id).await {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    }

    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
    }

    let Some(rx) = state.sessions.attach_stream(&session_id).await else {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    };

    // Prime the stream with an empty event so strict clients can confirm the stream is live.
    // 先发送一个空事件，帮助严格客户端确认 SSE 流已成功建立。
    let priming_stream = stream::once(async move {
        Ok::<_, Infallible>(
            Event::default()
                .id(uuid::Uuid::new_v4().to_string())
                .data(""),
        )
    });

    let message_stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|value| {
        let data = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, Infallible>(
            Event::default()
                .id(uuid::Uuid::new_v4().to_string())
                .data(data),
        )
    });

    let event_stream = priming_stream.chain(message_stream);
    let mut resp = Sse::new(event_stream).into_response();
    resp.headers_mut().insert(
        "Mcp-Session-Id",
        HeaderValue::from_str(&session_id).unwrap(),
    );
    resp.headers_mut()
        .insert("Cache-Control", HeaderValue::from_static("no-cache"));

    resp
}

/// Handle DELETE /mcp as session shutdown / 处理 DELETE /mcp 以关闭会话。
async fn handle_streamable_delete(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = validate_origin(&headers) {
        return resp;
    }

    let Some(session_id) = extract_session_id(&query, &headers) else {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "Missing MCP session id for DELETE /mcp.",
        );
    };

    if !state.sessions.exists(&session_id).await {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    }

    state.sessions.remove(&session_id).await;
    eprintln!("[HTTP] Session deleted: {}", session_id);
    StatusCode::NO_CONTENT.into_response()
}

/// Handle the initialize request / 处理 initialize 初始化请求。
async fn handle_initialize_request(
    state: AppState,
    headers: HeaderMap,
    msg: Value,
    session_id: Option<String>,
) -> Response {
    if session_id.is_some() {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "initialize must not include an existing MCP session id.",
        );
    }

    if let Some(version) = protocol_header_value(&headers) {
        if negotiate_version(version).is_none() {
            return plain_response(
                StatusCode::BAD_REQUEST,
                &format!("Unsupported MCP-Protocol-Version header: {}", version),
            );
        }
    }

    let Some(response) = state.server.handle_message(&msg).await else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "initialize did not produce a JSON-RPC response.",
        );
    };

    let Some(protocol_version) = negotiated_protocol_from_initialize(&response) else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "initialize response did not include result.protocolVersion.",
        );
    };

    let initialize_request: InitializeRequest =
        match serde_json::from_value(msg.get("params").cloned().unwrap_or_default()) {
            Ok(request) => request,
            Err(error) => {
                return plain_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!(
                        "initialize params could not be reconstructed after success: {}",
                        error
                    ),
                );
            }
        };

    let new_session_id = state
        .sessions
        .create(RequestContext {
            transport: Some("streamable_http".to_string()),
            session_id: None,
            protocol_version: Some(protocol_version),
            client_info: initialize_request.client_info,
            client_capabilities: initialize_request.capabilities,
        })
        .await;
    let mut resp = Json(response).into_response();
    resp.headers_mut().insert(
        "Mcp-Session-Id",
        HeaderValue::from_str(&new_session_id).unwrap(),
    );
    resp
}

/// Handle post-initialize JSON-RPC requests / 处理初始化后的普通 JSON-RPC 请求。
async fn handle_streamable_request(
    state: AppState,
    headers: HeaderMap,
    msg: Value,
    session_id: Option<String>,
) -> Response {
    let Some(session_id) = session_id else {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "Missing MCP session id. Call initialize first.",
        );
    };

    if !state.sessions.exists(&session_id).await {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    }
    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
    }

    let request_context = match state.sessions.request_context(&session_id).await {
        Some(context) => context,
        None => return plain_response(StatusCode::NOT_FOUND, "Session not found."),
    };

    let Some(response) = state
        .server
        .handle_message_with_context(&msg, request_context)
        .await
    else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Request did not produce a JSON-RPC response.",
        );
    };

    let mut resp = Json(response).into_response();
    resp.headers_mut().insert(
        "Mcp-Session-Id",
        HeaderValue::from_str(&session_id).unwrap(),
    );
    resp
}

/// Handle notifications sent by the client / 处理客户端发送的 notification。
async fn handle_streamable_notification(
    state: AppState,
    headers: HeaderMap,
    msg: Value,
    session_id: Option<String>,
) -> Response {
    let Some(session_id) = session_id else {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "Missing MCP session id for notification.",
        );
    };

    if !state.sessions.exists(&session_id).await {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    }
    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
    }

    if let Some(request_context) = state.sessions.request_context(&session_id).await {
        let _ = state
            .server
            .handle_message_with_context(&msg, request_context)
            .await;
    }
    StatusCode::ACCEPTED.into_response()
}

/// Handle client-originated JSON-RPC responses / 处理客户端回传的 JSON-RPC response。
async fn handle_streamable_client_response(
    state: AppState,
    headers: HeaderMap,
    msg: Value,
    session_id: Option<String>,
) -> Response {
    let Some(session_id) = session_id else {
        return plain_response(
            StatusCode::BAD_REQUEST,
            "Missing MCP session id for response.",
        );
    };

    if !state.sessions.exists(&session_id).await {
        return plain_response(StatusCode::NOT_FOUND, "Session not found.");
    }
    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
    }

    if let Some(request_context) = state.sessions.request_context(&session_id).await {
        let _ = state
            .server
            .handle_message_with_context(&msg, request_context)
            .await;
    }
    StatusCode::ACCEPTED.into_response()
}

/// Validate Origin header according to the local-server deployment model /
/// 按本地服务部署模型校验 Origin，请拒绝明显非法来源。
fn validate_origin(headers: &HeaderMap) -> Result<(), Response> {
    let Some(origin) = headers.get("Origin").and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };

    if origin == "null"
        || origin.starts_with("http://127.0.0.1")
        || origin.starts_with("https://127.0.0.1")
        || origin.starts_with("http://localhost")
        || origin.starts_with("https://localhost")
        || origin.starts_with("http://[::1]")
        || origin.starts_with("https://[::1]")
    {
        return Ok(());
    }

    Err(plain_response(
        StatusCode::FORBIDDEN,
        "Forbidden Origin for MCP HTTP transport.",
    ))
}

/// Validate protocol version for an existing session /
/// 校验已建立会话在后续请求中携带的协议版本头是否匹配。
async fn validate_session_protocol(
    state: &AppState,
    headers: &HeaderMap,
    session_id: &str,
) -> Result<(), Response> {
    let Some(session_version) = state.sessions.protocol_version(session_id).await else {
        return Err(plain_response(StatusCode::NOT_FOUND, "Session not found."));
    };

    if let Some(header_version) = protocol_header_value(headers) {
        if negotiate_version(header_version).is_none() {
            return Err(plain_response(
                StatusCode::BAD_REQUEST,
                &format!(
                    "Unsupported MCP-Protocol-Version header: {}",
                    header_version
                ),
            ));
        }
        if header_version != session_version {
            return Err(plain_response(
                StatusCode::BAD_REQUEST,
                &format!(
                    "MCP-Protocol-Version header mismatch. Expected {}, got {}.",
                    session_version, header_version
                ),
            ));
        }
    }

    Ok(())
}

/// Extract the negotiated protocol version from initialize response /
/// 从 initialize 响应中提取协商后的协议版本。
fn negotiated_protocol_from_initialize(response: &Value) -> Option<String> {
    let protocol_version = response
        .get("result")
        .and_then(|result| result.get("protocolVersion"))
        .and_then(|value| value.as_str())?;

    if protocol_version == PROTOCOL_VERSION_LATEST || negotiate_version(protocol_version).is_some()
    {
        return Some(protocol_version.to_string());
    }

    None
}

/// Detect the JSON-RPC message kind / 判断 JSON-RPC 消息类型。
fn classify_jsonrpc_message(msg: &Value) -> Option<JsonRpcMessageKind<'_>> {
    let obj = msg.as_object()?;
    let method = obj.get("method").and_then(|value| value.as_str());
    let has_id = obj.contains_key("id");
    let has_result = obj.contains_key("result");
    let has_error = obj.contains_key("error");

    match (method, has_id, has_result, has_error) {
        (Some(method), true, false, false) => Some(JsonRpcMessageKind::Request { method }),
        (Some(method), false, false, false) => Some(JsonRpcMessageKind::Notification { method }),
        (None, true, _, _) if has_result || has_error => Some(JsonRpcMessageKind::Response),
        _ => None,
    }
}

/// Read MCP-Protocol-Version header / 读取 MCP-Protocol-Version 请求头。
fn protocol_header_value<'a>(headers: &'a HeaderMap) -> Option<&'a str> {
    headers
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok())
}

/// Check whether the client accepts SSE / 判断客户端是否接受 SSE。
fn accepts_sse(headers: &HeaderMap) -> bool {
    headers
        .get("Accept")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.contains("text/event-stream"))
        .unwrap_or(false)
}

/// Extract session id from query or header / 从查询参数或请求头中提取 session id。
fn extract_session_id(query: &McpQuery, headers: &HeaderMap) -> Option<String> {
    query.session_id.clone().or_else(|| {
        headers
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(String::from)
    })
}

/// Build a plain-text HTTP response / 构造纯文本 HTTP 响应。
fn plain_response(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

/// Build a JSON response with explicit status / 构造带明确状态码的 JSON 响应。
fn json_with_status(status: StatusCode, payload: Value) -> Response {
    (status, Json(payload)).into_response()
}

/// Build a JSON-RPC error HTTP response / 构造 JSON-RPC 错误 HTTP 响应。
fn jsonrpc_error_response(status: StatusCode, id: Value, code: i64, message: &str) -> Response {
    json_with_status(
        status,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message
            }
        }),
    )
}

// ============================================================
// Legacy SSE handlers (2024-11-05 / 2025-03-26) / 旧版 SSE 处理器
// ============================================================

fn sse_event_stream(
    session_id: String,
    rx: tokio::sync::mpsc::Receiver<Value>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/message?sessionId={}", session_id));

    let ping_stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(30),
    ))
    .map(|_| Ok::<_, Infallible>(Event::default().event("ping").data("")));

    let message_stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|val| {
        let data = serde_json::to_string(&val).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("message").data(data))
    });

    futures::stream::once(async move { Ok(endpoint_event) })
        .chain(futures::stream::select(ping_stream, message_stream))
}

async fn handle_sse_get(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (session_id, rx) = state.sse_sessions.create().await;
    eprintln!("[SSE] New SSE connection: {}", session_id);
    Sse::new(sse_event_stream(session_id, rx))
}

async fn handle_sse_post(
    State(state): State<AppState>,
    query: Query<HashMap<String, String>>,
    body: String,
) -> StatusCode {
    let query_str = serde_urlencoded::to_string(&query.0).unwrap_or_default();
    let session_id = SseSessionManager::session_id_from_query(&format!("?{}", query_str));

    let session_id = match session_id {
        Some(sid) => sid,
        None => return StatusCode::BAD_REQUEST,
    };

    let msg: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let response = state.server.handle_message(&msg).await;
    if let Some(resp) = response {
        let _ = state.sse_sessions.send(&session_id, resp).await;
    }

    StatusCode::ACCEPTED
}
