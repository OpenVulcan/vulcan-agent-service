//! Streamable HTTP handlers for the MCP HTTP transport.
//! MCP HTTP 传输的 Streamable HTTP 处理器。

use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::convert::Infallible;

use crate::transport::http::helpers::{
    JsonRpcMessageKind, McpQuery, accepts_sse, classify_jsonrpc_message,
    client_match_name_override_header_value, extract_session_id, forbidden_origin_response,
    is_allowed_local_origin, json_with_status, jsonrpc_error_response,
    merge_header_client_match_name_override, negotiated_protocol_from_initialize, plain_response,
    protocol_header_value,
};
use crate::transport::mcp::McpDispatcher;
use crate::transport::mcp::protocol::{
    InitializeRequest, RequestContext, negotiate_version, parse_required_params,
};

use super::AppState;

// ============================================================
// HTTP Streamable handlers (2025-11-25)
// 新版 Streamable HTTP 处理器
// ============================================================

/// Handle POST /mcp JSON-RPC messages for streamable HTTP sessions.
/// 处理 Streamable HTTP 会话中的 POST /mcp JSON-RPC 消息。
#[axum::debug_handler]
pub(super) async fn handle_streamable_post(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_allowed_local_origin(&headers) {
        return forbidden_origin_response();
    }

    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(e) => {
            return json_with_status(
                StatusCode::BAD_REQUEST,
                McpDispatcher::parse_error(&format!("Invalid UTF-8: {}", e)),
            );
        }
    };

    let msg: Value = match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(e) => {
            return json_with_status(
                StatusCode::BAD_REQUEST,
                McpDispatcher::parse_error(&format!("Parse error: {}", e)),
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
        JsonRpcMessageKind::Request {
            method: "initialize",
        } => handle_initialize_request(state, headers, msg, session_id).await,
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

/// Handle GET
/// mcp as the session-bound SSE listener
/// 将 GET /mcp 作为绑定会话的 SSE 监听流。
pub(super) async fn handle_streamable_get(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
) -> Response {
    if !is_allowed_local_origin(&headers) {
        return forbidden_origin_response();
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

    if let Err(rejection) = validate_session_protocol(&state, &headers, &session_id).await {
        return rejection.into_response();
    }

    // Send a comment-only prelude so the stream flushes without emitting an empty default `message` event.
    // 发送仅注释的前导帧，在刷新流的同时避免产出空的默认 `message` 事件。
    let priming_stream =
        stream::once(async move { Ok::<_, Infallible>(Event::default().comment("stream-ready")) });

    // Emit comment keepalives because this transport currently returns request responses on POST rather than this GET stream.
    // 发送注释保活帧，因为当前传输会在 POST 上返回请求响应，而不是通过该 GET 流投递响应。
    let keepalive_stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(30),
    ))
    .map(|_| Ok::<_, Infallible>(Event::default().comment("stream-keepalive")));

    // Chain the priming frame with long-lived keepalives so clients can keep one session-bound listener open.
    // 将启动帧与长连接保活帧串联，使客户端可以保持一个绑定会话的监听连接。
    let event_stream = priming_stream.chain(keepalive_stream);
    let mut resp = Sse::new(event_stream).into_response();
    if !insert_session_id_header(&mut resp, &session_id) {
        return invalid_session_id_header_response();
    }
    resp.headers_mut()
        .insert("Cache-Control", HeaderValue::from_static("no-cache"));

    resp
}

/// Insert one validated MCP session id header into a streamable HTTP response.
/// 向 streamable HTTP 响应写入经过校验的 MCP session id 请求头。
fn insert_session_id_header(resp: &mut Response, session_id: &str) -> bool {
    let Ok(header_value) = HeaderValue::from_str(session_id) else {
        return false;
    };
    resp.headers_mut().insert("Mcp-Session-Id", header_value);
    true
}

/// Build the response used when a stored MCP session id cannot be represented as a response header.
/// 构造存储的 MCP session id 无法表示为响应头时使用的响应。
fn invalid_session_id_header_response() -> Response {
    plain_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "MCP session id could not be encoded as a response header.",
    )
}

/// Handle DELETE
/// mcp as session shutdown
/// 处理 DELETE /mcp 以关闭会话。
pub(super) async fn handle_streamable_delete(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
) -> Response {
    if !is_allowed_local_origin(&headers) {
        return forbidden_origin_response();
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

/// Handle the initialize request
/// 处理 initialize 初始化请求。
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

    if let Some(version) = protocol_header_value(&headers)
        && negotiate_version(version).is_none()
    {
        return plain_response(
            StatusCode::BAD_REQUEST,
            &format!("Unsupported MCP-Protocol-Version header: {}", version),
        );
    }

    let Some(response) = state.dispatcher.handle_message(&msg).await else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "initialize did not produce a JSON-RPC response.",
        );
    };

    if response.get("error").is_some() {
        return json_with_status(jsonrpc_initialize_error_status(&response), response);
    }

    let Some(protocol_version) = negotiated_protocol_from_initialize(&response) else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "initialize response did not include result.protocolVersion.",
        );
    };

    let initialize_request: InitializeRequest =
        match parse_required_params("initialize", msg.get("params").cloned()) {
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

    let new_session_id = match state
        .sessions
        .create(RequestContext {
            transport: Some("streamable_http".to_string()),
            session_id: None,
            protocol_version: Some(protocol_version),
            client_info: initialize_request.client_info,
            client_match_name_override: client_match_name_override_header_value(&headers),
            exact_client_name: None,
            disable_client_match_overrides: false,
            client_capabilities: initialize_request.capabilities,
        })
        .await
    {
        Ok(session_id) => session_id,
        Err(error) => return plain_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    let mut resp = Json(response).into_response();
    if !insert_session_id_header(&mut resp, &new_session_id) {
        return invalid_session_id_header_response();
    }
    resp
}

/// Map an initialize JSON-RPC error response to the streamable HTTP status code.
/// 将 initialize 的 JSON-RPC 错误响应映射为 streamable HTTP 状态码。
///
/// Parameters: `response` is the dispatcher-produced JSON-RPC error response.
/// 参数：`response` 是 dispatcher 产出的 JSON-RPC 错误响应。
///
/// Returns: `500` for internal JSON-RPC errors and `400` for client-side request errors.
/// 返回：内部 JSON-RPC 错误返回 `500`，客户端请求错误返回 `400`。
fn jsonrpc_initialize_error_status(response: &Value) -> StatusCode {
    match response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64)
    {
        Some(-32603) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    }
}

/// Handle post-initialize JSON-RPC requests
/// 处理初始化后的普通 JSON-RPC 请求。
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
    if let Err(rejection) = validate_session_protocol(&state, &headers, &session_id).await {
        return rejection.into_response();
    }

    let request_context = match state.sessions.request_context(&session_id).await {
        Some(context) => context,
        None => return plain_response(StatusCode::NOT_FOUND, "Session not found."),
    };
    let request_context = merge_header_client_match_name_override(
        request_context,
        client_match_name_override_header_value(&headers),
    );

    let Some(response) = state
        .dispatcher
        .handle_message_with_context(&msg, request_context)
        .await
    else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Request did not produce a JSON-RPC response.",
        );
    };

    let mut resp = Json(response).into_response();
    if !insert_session_id_header(&mut resp, &session_id) {
        return invalid_session_id_header_response();
    }
    resp
}

/// Handle notifications sent by the client
/// 处理客户端发送的 notification。
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
    if let Err(rejection) = validate_session_protocol(&state, &headers, &session_id).await {
        return rejection.into_response();
    }

    if let Some(request_context) = state.sessions.request_context(&session_id).await {
        let request_context = merge_header_client_match_name_override(
            request_context,
            client_match_name_override_header_value(&headers),
        );
        let _ = state
            .dispatcher
            .handle_message_with_context(&msg, request_context)
            .await;
    }
    StatusCode::ACCEPTED.into_response()
}

/// Handle client-originated JSON-RPC responses
/// 处理客户端回传的 JSON-RPC response。
async fn handle_streamable_client_response(
    state: AppState,
    headers: HeaderMap,
    _msg: Value,
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
    if let Err(rejection) = validate_session_protocol(&state, &headers, &session_id).await {
        return rejection.into_response();
    }

    // JSON-RPC response bodies terminate at the HTTP transport boundary because this server has no outstanding server-originated requests.
    // JSON-RPC response body 终止在 HTTP 传输边界，因为当前服务没有待完成的服务端发起请求。
    StatusCode::ACCEPTED.into_response()
}

/// Session protocol validation rejection.
/// 会话协议版本校验拒绝原因。
enum SessionProtocolRejection {
    /// The session id is no longer known.
    /// 会话 ID 已不存在。
    NotFound,
    /// The header carries a protocol version outside the supported MCP set.
    /// 请求头携带了不在 MCP 支持集合内的协议版本。
    UnsupportedVersion(String),
    /// The header version differs from the version negotiated during initialize.
    /// 请求头版本与 initialize 阶段协商出的版本不一致。
    VersionMismatch { expected: String, actual: String },
}

impl SessionProtocolRejection {
    /// Convert a protocol validation rejection into the streamable HTTP response contract.
    /// 将协议校验拒绝原因转换为 streamable HTTP 响应契约。
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => plain_response(StatusCode::NOT_FOUND, "Session not found."),
            Self::UnsupportedVersion(version) => plain_response(
                StatusCode::BAD_REQUEST,
                &format!("Unsupported MCP-Protocol-Version header: {}", version),
            ),
            Self::VersionMismatch { expected, actual } => plain_response(
                StatusCode::BAD_REQUEST,
                &format!(
                    "MCP-Protocol-Version header mismatch. Expected {}, got {}.",
                    expected, actual
                ),
            ),
        }
    }
}

/// Validate protocol version for an existing session.
/// 校验已建立会话在后续请求中携带的协议版本头是否匹配。
async fn validate_session_protocol(
    state: &AppState,
    headers: &HeaderMap,
    session_id: &str,
) -> Result<(), SessionProtocolRejection> {
    let Some(session_version) = state.sessions.protocol_version(session_id).await else {
        return Err(SessionProtocolRejection::NotFound);
    };

    let Some(header_version) = protocol_header_value(headers) else {
        return Ok(());
    };

    if negotiate_version(header_version).is_none() {
        return Err(SessionProtocolRejection::UnsupportedVersion(
            header_version.to_string(),
        ));
    }
    if header_version != session_version {
        return Err(SessionProtocolRejection::VersionMismatch {
            expected: session_version,
            actual: header_version.to_string(),
        });
    }

    Ok(())
}
