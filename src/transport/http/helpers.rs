use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::Event},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;

use crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_HEADER;
use crate::transport::mcp::protocol::RequestContext;

/// Query parameters accepted by the streamable HTTP MCP endpoint.
/// streamable HTTP MCP 端点接受的查询参数。
#[derive(Deserialize)]
pub(super) struct McpQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

/// JSON-RPC payload kind.
/// JSON-RPC 负载分类。
pub(super) enum JsonRpcMessageKind<'a> {
    Request { method: &'a str },
    Notification { method: &'a str },
    Response,
}

/// Validate Origin header according to the local-server deployment model.
/// 按本地服务部署模型校验 Origin 请求头。
pub(super) fn validate_origin(headers: &HeaderMap) -> Result<(), Response> {
    if let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) {
        if origin != "null"
            && !origin.starts_with("http://localhost")
            && !origin.starts_with("http://127.0.0.1")
            && !origin.starts_with("http://[::1]")
        {
            return Err(plain_response(
                StatusCode::FORBIDDEN,
                "Forbidden origin for local MCP server",
            ));
        }
    }
    Ok(())
}

/// Extract the negotiated protocol version from initialize response.
/// 从 initialize 响应中提取协商后的协议版本。
pub(super) fn negotiated_protocol_from_initialize(response: &Value) -> Option<String> {
    response
        .get("result")
        .and_then(|result| result.get("protocolVersion"))
        .and_then(|version| version.as_str())
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

/// Detect the JSON-RPC message kind.
/// 判断 JSON-RPC 消息类型。
pub(super) fn classify_jsonrpc_message(msg: &Value) -> Option<JsonRpcMessageKind<'_>> {
    if let Some(method) = msg.get("method").and_then(|value| value.as_str()) {
        if msg.get("id").is_some() {
            return Some(JsonRpcMessageKind::Request { method });
        }
        return Some(JsonRpcMessageKind::Notification { method });
    }
    if msg.get("id").is_some() && (msg.get("result").is_some() || msg.get("error").is_some()) {
        return Some(JsonRpcMessageKind::Response);
    }
    None
}

/// Read MCP-Protocol-Version header.
/// 读取 MCP-Protocol-Version 请求头。
pub(super) fn protocol_header_value<'a>(headers: &'a HeaderMap) -> Option<&'a str> {
    headers
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok())
}

/// Check whether the client accepts SSE.
/// 判断客户端是否接受 SSE。
pub(super) fn accepts_sse(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|item| item.trim() == "text/event-stream")
        })
        .unwrap_or(false)
}

/// Extract session id from query or header.
/// 从查询参数或请求头中提取 session id。
pub(super) fn extract_session_id(query: &McpQuery, headers: &HeaderMap) -> Option<String> {
    query.session_id.clone().or_else(|| {
        headers
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    })
}

/// Read the optional client-match-name override header used by Vulcan host policy matching.
/// 读取 Vulcan 宿主策略匹配使用的可选客户端匹配名覆盖请求头。
pub(super) fn client_match_name_override_header_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CLIENT_MATCH_NAME_OVERRIDE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Merge a request-level header override into the stored request context.
/// 将请求级请求头覆盖值合并到已保存的请求上下文。
pub(super) fn merge_header_client_match_name_override(
    mut request_context: RequestContext,
    client_match_name_override: Option<String>,
) -> RequestContext {
    if let Some(override_name) = client_match_name_override {
        request_context.client_match_name_override = Some(override_name);
    }
    request_context
}

/// Build a plain-text HTTP response.
/// 构造纯文本 HTTP 响应。
pub(super) fn plain_response(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

/// Build a JSON response with explicit status.
/// 构造带明确状态码的 JSON 响应。
pub(super) fn json_with_status(status: StatusCode, payload: Value) -> Response {
    (status, axum::Json(payload)).into_response()
}

/// Build a JSON-RPC error HTTP response.
/// 构造 JSON-RPC 错误 HTTP 响应。
pub(super) fn jsonrpc_error_response(
    status: StatusCode,
    id: Value,
    code: i64,
    message: &str,
) -> Response {
    json_with_status(
        status,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            }
        }),
    )
}

/// Build a legacy SSE event stream with one endpoint event, periodic pings, and queued session messages.
/// 构造旧版 SSE 事件流，包含 endpoint 事件、周期 ping 与排队的会话消息。
pub(super) fn sse_event_stream(
    session_id: String,
    rx: tokio::sync::mpsc::Receiver<Value>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/message?sessionId={}", session_id));

    // Keep the legacy named ping event for backwards compatibility, but avoid an empty `data:` field.
    // 为兼容旧客户端保留具名 ping 事件，同时避免发送空的 `data:` 字段。
    let ping_stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(30),
    ))
    .map(|_| Ok::<_, Infallible>(Event::default().event("ping")));

    let message_stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|val| {
        let data = serde_json::to_string(&val).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("message").data(data))
    });

    stream::once(async move { Ok(endpoint_event) })
        .chain(stream::select(ping_stream, message_stream))
}
