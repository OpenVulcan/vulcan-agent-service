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
    // Read the already-decoded legacy SSE session id from Axum's query extractor.
    // 从 Axum 查询提取器已解码的参数中读取旧版 SSE 会话 ID。
    let Some(session_id) = legacy_sse_session_id_from_params(&query.0) else {
        return StatusCode::BAD_REQUEST;
    };
    if !state.sse_sessions.exists(&session_id).await {
        return StatusCode::NOT_FOUND;
    }

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
    if let Some(resp) = response
        && state.sse_sessions.send(&session_id, resp).await.is_err()
    {
        return StatusCode::NOT_FOUND;
    }

    StatusCode::ACCEPTED
}

/// Extract the legacy SSE session id from already-decoded query parameters.
/// 从已经解码的查询参数中提取旧版 SSE 会话 ID。
/// Parameters: `query` is Axum's parsed query map for POST `/message`.
/// 参数：`query` 是 Axum 为 POST `/message` 解析出的查询参数映射。
/// Returns the non-empty session id when the `sessionId` parameter exists.
/// 返回存在 `sessionId` 参数且非空时的会话 ID。
fn legacy_sse_session_id_from_params(query: &HashMap<String, String>) -> Option<String> {
    query
        .get("sessionId")
        .map(String::as_str)
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy SSE query extraction should reject missing session ids.
    /// 旧版 SSE 查询提取应拒绝缺失的会话 ID。
    #[test]
    fn legacy_sse_session_id_from_params_rejects_missing_session_id() {
        // Build a query map without the required sessionId parameter.
        // 构造一份缺少必需 sessionId 参数的查询映射。
        let query = HashMap::new();

        assert_eq!(legacy_sse_session_id_from_params(&query), None);
    }

    /// Legacy SSE query extraction should reject blank session ids.
    /// 旧版 SSE 查询提取应拒绝空白会话 ID。
    #[test]
    fn legacy_sse_session_id_from_params_rejects_blank_session_id() {
        // Build a query map whose sessionId value only contains whitespace.
        // 构造一份 sessionId 仅包含空白字符的查询映射。
        let query = HashMap::from([("sessionId".to_string(), "   ".to_string())]);

        assert_eq!(legacy_sse_session_id_from_params(&query), None);
    }

    /// Legacy SSE query extraction should return the decoded session id directly.
    /// 旧版 SSE 查询提取应直接返回已解码的会话 ID。
    #[test]
    fn legacy_sse_session_id_from_params_returns_decoded_session_id() {
        // Build a query map with a padded decoded session id.
        // 构造一份带有首尾空白的已解码会话 ID 查询映射。
        let query = HashMap::from([("sessionId".to_string(), " sse-42 ".to_string())]);

        assert_eq!(
            legacy_sse_session_id_from_params(&query).as_deref(),
            Some("sse-42")
        );
    }
}
