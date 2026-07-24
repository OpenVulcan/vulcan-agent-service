use crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_HEADER;
use crate::host_core::HostRuntime;
use crate::transport::http::helpers::McpQuery;
use crate::transport::http::helpers::{
    client_match_name_override_header_value, is_allowed_local_origin,
    merge_header_client_match_name_override,
};
use crate::transport::http::session::{SessionManager, SseSessionManager};
use crate::transport::mcp::protocol::RequestContext;
use axum::{
    body::{Bytes, to_bytes},
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Sse},
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::collections::HashMap;

use super::{
    AppState,
    legacy_sse::handle_sse_post,
    streamable::{handle_streamable_get, handle_streamable_post},
};

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

/// Local origin validation should allow absent or loopback origins and reject remote origins.
/// 本地 Origin 校验应允许缺失或回环来源，并拒绝远程来源。
#[test]
fn is_allowed_local_origin_rejects_non_loopback_origin() {
    let mut headers = HeaderMap::new();
    assert!(is_allowed_local_origin(&headers));

    headers.insert("origin", HeaderValue::from_static("http://localhost:3000"));
    assert!(is_allowed_local_origin(&headers));

    headers.insert("origin", HeaderValue::from_static("https://example.com"));
    assert!(!is_allowed_local_origin(&headers));
}

/// Legacy SSE POST should reject a session id that is not registered.
/// Legacy SSE POST 应拒绝未注册的会话 ID。
#[tokio::test]
async fn legacy_sse_post_rejects_unknown_session() {
    // Build HTTP app state without any legacy SSE sessions.
    // 构造不包含任何旧版 SSE 会话的 HTTP 应用状态。
    let state = AppState {
        dispatcher: crate::transport::mcp::McpDispatcher::new(HostRuntime::new()),
        sessions: SessionManager::new(),
        sse_sessions: SseSessionManager::new(),
    };
    // Build a POST query pointing at a missing session.
    // 构造指向缺失会话的 POST 查询参数。
    let query = HashMap::from([("sessionId".to_string(), "sse-missing".to_string())]);
    // Build a valid JSON-RPC notification body so the failure is tied to session lookup.
    // 构造合法 JSON-RPC notification 请求体，确保失败来源是会话查询。
    let body = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
    .to_string();

    let status = handle_sse_post(State(state), HeaderMap::new(), Query(query), body).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Legacy SSE POST should reject delivery when the receiver side has already closed.
/// Legacy SSE POST 应在接收端已经关闭时拒绝投递。
#[tokio::test]
async fn legacy_sse_post_rejects_closed_receiver_delivery() {
    // Build a legacy SSE manager with one registered session.
    // 构造包含一个已注册会话的旧版 SSE 管理器。
    let sse_sessions = SseSessionManager::new();
    let (session_id, rx) = sse_sessions.create().await;
    // Close the receiver before POST tries to deliver a dispatcher response.
    // 在 POST 尝试投递 dispatcher 响应前关闭接收端。
    drop(rx);
    // Keep one clone so the test can inspect cleanup after the handler consumes AppState.
    // 保留一个克隆，用于 handler 消费 AppState 后检查清理结果。
    let sse_sessions_after_post = sse_sessions.clone();
    // Build HTTP app state using the prepared legacy SSE manager.
    // 使用准备好的旧版 SSE 管理器构造 HTTP 应用状态。
    let state = AppState {
        dispatcher: crate::transport::mcp::McpDispatcher::new(HostRuntime::new()),
        sessions: SessionManager::new(),
        sse_sessions,
    };
    // Build a POST query pointing at the registered session.
    // 构造指向已注册会话的 POST 查询参数。
    let query = HashMap::from([("sessionId".to_string(), session_id.clone())]);
    // Use tools/list so the dispatcher produces a response that must be delivered to SSE.
    // 使用 tools/list，使 dispatcher 产生必须投递到 SSE 的响应。
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    })
    .to_string();

    let status = handle_sse_post(State(state), HeaderMap::new(), Query(query), body).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!sse_sessions_after_post.exists(&session_id).await);
}

/// Streamable GET should reject a forbidden Origin before session lookup.
/// Streamable GET 应在会话查询前拒绝非法 Origin。
#[tokio::test]
async fn streamable_get_rejects_forbidden_origin_before_session_lookup() {
    let state = AppState {
        dispatcher: crate::transport::mcp::McpDispatcher::new(HostRuntime::new()),
        sessions: SessionManager::new(),
        sse_sessions: SseSessionManager::new(),
    };
    let query: McpQuery =
        serde_urlencoded::from_str("").expect("empty streamable query should deserialize");
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("origin", HeaderValue::from_static("https://example.com"));

    let response = handle_streamable_get(State(state), Query(query), headers).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Streamable GET should reject unsupported MCP protocol headers before opening the SSE stream.
/// Streamable GET 应在打开 SSE 流前拒绝不支持的 MCP 协议请求头。
#[tokio::test]
async fn streamable_get_rejects_unsupported_protocol_header() {
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
        .await
        .expect("valid streamable session should be created");

    let query: McpQuery = serde_urlencoded::from_str(&format!("sessionId={session_id}"))
        .expect("streamable query should deserialize");
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert(
        "MCP-Protocol-Version",
        HeaderValue::from_static("1900-01-01"),
    );

    let response = handle_streamable_get(State(state), Query(query), headers).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
        .await
        .expect("valid streamable session should be created");

    let query: McpQuery = serde_urlencoded::from_str(&format!("sessionId={session_id}"))
        .expect("streamable query should deserialize");
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));

    let response = handle_streamable_get(State(state), Query(query), headers).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok()),
        Some(session_id.as_str())
    );

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

/// Streamable initialize should forward dispatcher JSON-RPC errors without creating a session response.
/// Streamable initialize 应直接转发 dispatcher 的 JSON-RPC 错误，而不是创建会话响应。
#[tokio::test]
async fn streamable_initialize_missing_params_returns_jsonrpc_bad_request() {
    // Build HTTP app state without existing sessions so initialize owns the entire session lifecycle.
    // 构造无既有会话的 HTTP 应用状态，使 initialize 独占完整会话生命周期。
    let state = AppState {
        dispatcher: crate::transport::mcp::McpDispatcher::new(HostRuntime::new()),
        sessions: SessionManager::new(),
        sse_sessions: SseSessionManager::new(),
    };
    let query: McpQuery =
        serde_urlencoded::from_str("").expect("empty streamable query should deserialize");
    let body = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "initialize"
    });

    let response = handle_streamable_post(
        State(state),
        Query(query),
        HeaderMap::new(),
        Bytes::from(body.to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get("Mcp-Session-Id").is_none());

    let body_bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("streamable initialize error body should be readable");
    let body: Value =
        serde_json::from_slice(&body_bytes).expect("streamable initialize error should be JSON");

    assert_eq!(body["id"], json!(7));
    assert_eq!(body["error"]["code"], json!(-32602));
    assert_eq!(
        body["error"]["message"],
        json!("Invalid initialize params: initialize params are required.")
    );
}

/// Streamable client responses should only validate the session and then acknowledge the payload.
/// Streamable 客户端 response 应只完成会话校验，然后确认接收载荷。
#[tokio::test]
async fn streamable_client_response_returns_accepted_for_valid_session() {
    // Build HTTP app state with one initialized streamable session.
    // 构造包含一个已初始化 streamable 会话的 HTTP 应用状态。
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
        .await
        .expect("valid streamable session should be created");
    let query: McpQuery = serde_urlencoded::from_str(&format!("sessionId={session_id}"))
        .expect("streamable query should deserialize");
    let body = json!({
        "jsonrpc": "2.0",
        "id": 8,
        "result": {
            "ok": true
        }
    });

    let response = handle_streamable_post(
        State(state),
        Query(query),
        HeaderMap::new(),
        Bytes::from(body.to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
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
