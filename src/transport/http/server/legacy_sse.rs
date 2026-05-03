//! Legacy SSE handlers for older MCP HTTP clients.
//! 旧版 MCP HTTP 客户端使用的 SSE 处理器。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, Sse},
};
use futures::Stream;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;

use crate::transport::http::helpers::{client_match_name_override_header_value, sse_event_stream};
use crate::transport::http::session::SseSessionManager;
use crate::transport::mcp::protocol::RequestContext;

use super::AppState;

/// Open a legacy SSE stream and register the generated session.
/// 打开旧版 SSE 流并注册生成的会话。
pub(super) async fn handle_sse_get(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (session_id, rx) = state.sse_sessions.create().await;
    eprintln!("[SSE] New SSE connection: {}", session_id);
    Sse::new(sse_event_stream(session_id, rx))
}

/// Accept a legacy SSE message POST and forward any response back to the SSE session.
/// 接收旧版 SSE message POST，并将可能产生的响应转发回 SSE 会话。
pub(super) async fn handle_sse_post(
    State(state): State<AppState>,
    headers: HeaderMap,
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

    let request_context = RequestContext {
        transport: Some("legacy_sse".to_string()),
        session_id: Some(session_id.clone()),
        client_match_name_override: client_match_name_override_header_value(&headers),
        ..RequestContext::default()
    };

    let response = state
        .dispatcher
        .handle_message_with_context(&msg, request_context)
        .await;
    if let Some(resp) = response {
        let _ = state.sse_sessions.send(&session_id, resp).await;
    }

    StatusCode::ACCEPTED
}
