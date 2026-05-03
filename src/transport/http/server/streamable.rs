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

use crate::host_core::McpServer;
use crate::transport::http::helpers::{
    JsonRpcMessageKind, McpQuery, accepts_sse, classify_jsonrpc_message,
    client_match_name_override_header_value, extract_session_id, json_with_status,
    jsonrpc_error_response, merge_header_client_match_name_override,
    negotiated_protocol_from_initialize, plain_response, protocol_header_value, validate_origin,
};
use crate::transport::mcp::protocol::{InitializeRequest, RequestContext, negotiate_version};

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

/// Handle GET
/// mcp as the session-bound SSE listener
/// 将 GET /mcp 作为绑定会话的 SSE 监听流。
pub(super) async fn handle_streamable_get(
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

/// Handle DELETE
/// mcp as session shutdown
/// 处理 DELETE /mcp 以关闭会话。
pub(super) async fn handle_streamable_delete(
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

    if let Some(version) = protocol_header_value(&headers) {
        if negotiate_version(version).is_none() {
            return plain_response(
                StatusCode::BAD_REQUEST,
                &format!("Unsupported MCP-Protocol-Version header: {}", version),
            );
        }
    }

    let Some(response) = state.dispatcher.handle_message(&msg).await else {
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
            client_match_name_override: client_match_name_override_header_value(&headers),
            exact_client_name: None,
            disable_client_match_overrides: false,
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
    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
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
    resp.headers_mut().insert(
        "Mcp-Session-Id",
        HeaderValue::from_str(&session_id).unwrap(),
    );
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
    if let Err(resp) = validate_session_protocol(&state, &headers, &session_id).await {
        return resp;
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

/// Validate protocol version for an existing session.
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
