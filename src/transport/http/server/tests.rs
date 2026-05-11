use crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_HEADER;
use crate::host_core::HostRuntime;
use crate::transport::http::helpers::McpQuery;
use crate::transport::http::helpers::{
    client_match_name_override_header_value, merge_header_client_match_name_override,
};
use crate::transport::http::session::{SessionManager, SseSessionManager};
use crate::transport::mcp::protocol::RequestContext;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Sse},
};
use futures::StreamExt;

use super::{AppState, streamable::handle_streamable_get};

/// Header parsing should read the exact override value and ignore absent headers.
/// 请求头解析应读取明确覆盖值，并在缺失时返回空结果。
#[test]
fn client_match_name_override_header_value_reads_override_when_present() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CLIENT_MATCH_NAME_OVERRIDE_HEADER,
        HeaderValue::from_static("qoder"),
    );
    assert_eq!(
        client_match_name_override_header_value(&headers).as_deref(),
        Some("qoder")
    );

    let empty_headers = HeaderMap::new();
    assert_eq!(
        client_match_name_override_header_value(&empty_headers),
        None
    );
}

/// Request-level header overrides should replace any stored session override for the current request.
/// 请求级请求头覆盖值应替换当前请求使用的已保存会话覆盖值。
#[test]
fn merge_header_client_match_name_override_replaces_stored_override() {
    let request_context = RequestContext {
        client_match_name_override: Some("mcphost".to_string()),
        ..RequestContext::default()
    };

    let merged =
        merge_header_client_match_name_override(request_context, Some("qoder".to_string()));
    assert_eq!(merged.client_match_name_override.as_deref(), Some("qoder"));
}

/// Streamable GET should start with a comment frame instead of an empty default message event.
/// Streamable GET 应以注释帧开头，而不是空的默认 message 事件。
#[tokio::test]
async fn streamable_get_primes_with_comment_frame_instead_of_empty_message_event() {
    let state = AppState {
        dispatcher: crate::transport::mcp::McpDispatcher::new(HostRuntime::new()),
        sessions: SessionManager::new(),
        sse_sessions: SseSessionManager::new(),
    };
    let session_id = state
        .sessions
        .create(RequestContext {
            transport: Some("streamable_http".to_string()),
            protocol_version: Some("2025-06-18".to_string()),
            ..RequestContext::default()
        })
        .await;

    let query: McpQuery = serde_urlencoded::from_str(&format!("sessionId={session_id}"))
        .expect("streamable query should deserialize");
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));

    let response = handle_streamable_get(State(state), Query(query), headers).await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut body_stream = response.into_body().into_data_stream();
    let first_chunk = body_stream
        .next()
        .await
        .expect("streamable response should emit a priming chunk")
        .expect("priming chunk should be readable");
    let first_text = std::str::from_utf8(&first_chunk).expect("priming chunk should be UTF-8");

    assert_eq!(first_text, ": stream-ready\n\n");
    assert!(!first_text.contains("data:"));
}

/// Legacy SSE keepalive should remain a named ping event, but must not carry an empty JSON payload.
/// Legacy SSE 保活应继续使用具名 ping 事件，但不能携带空 JSON 负载。
#[tokio::test]
async fn legacy_sse_keepalive_uses_ping_event_without_data_payload() {
    let (_tx, rx) = tokio::sync::mpsc::channel(4);
    let response = Sse::new(crate::transport::http::helpers::sse_event_stream(
        "sse-test".to_string(),
        rx,
    ))
    .into_response();

    let mut body_stream = response.into_body().into_data_stream();
    let endpoint_chunk = body_stream
        .next()
        .await
        .expect("legacy SSE should emit the endpoint event first")
        .expect("endpoint chunk should be readable");
    let endpoint_text =
        std::str::from_utf8(&endpoint_chunk).expect("endpoint chunk should be UTF-8");
    assert!(endpoint_text.contains("event: endpoint"));
    assert!(endpoint_text.contains("data: /message?sessionId=sse-test"));

    let ping_chunk = body_stream
        .next()
        .await
        .expect("legacy SSE should emit a keepalive ping event")
        .expect("ping chunk should be readable");
    let ping_text = std::str::from_utf8(&ping_chunk).expect("ping chunk should be UTF-8");
    assert_eq!(ping_text, "event: ping\n\n");
    assert!(!ping_text.contains("data:"));
}
