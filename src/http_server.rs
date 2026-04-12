use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};

use crate::server::McpServer;
use crate::session::{SessionManager, SseSessionManager};

// ============================================================
// Application state shared with axum handlers
// ============================================================

#[derive(Clone)]
pub struct AppState {
    pub server: McpServer,
    pub sessions: SessionManager,
    pub sse_sessions: SseSessionManager,
}

// ============================================================
// HTTP transport runner
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
        // HTTP Streamable (2025-11-25)
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
    eprintln!("[MCP]   POST   /mcp       (HTTP Streamable, 2025-11-25)");
    eprintln!("[MCP]   DELETE /mcp       (Close session, 2025-11-25)");
    eprintln!("[MCP]   GET    /sse       (Legacy SSE, 2024-11-05 / 2025-03-26)");
    eprintln!("[MCP]   POST   /message   (Legacy POST, 2024-11-05 / 2025-03-26)");
    eprintln!("[MCP]   GET    /health    (Health check)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ============================================================
// HTTP Streamable handlers (2025-11-25)
// ============================================================

#[derive(Deserialize)]
struct McpQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

#[axum::debug_handler]
async fn handle_streamable_post(
    State(state): State<AppState>,
    query: Query<McpQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(e) => {
            return Json(McpServer::parse_error(&format!("Invalid UTF-8: {}", e))).into_response();
        }
    };
    let wants_sse = headers
        .get("Accept")
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/event-stream"))
        .unwrap_or(false);

    let msg: Value = match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(e) => {
            return Json(McpServer::parse_error(&format!("Parse error: {}", e))).into_response();
        }
    };

    // Get session ID from query param or Mcp-Session-Id header
    let session_id = query.session_id.clone().or_else(|| {
        headers
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    });

    // Determine which session to use
    let (final_session_id, response) = match session_id {
        Some(sid) if state.sessions.exists(&sid).await => {
            let resp = state.server.handle_message(&msg).await;
            (sid, resp)
        }
        Some(_) => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": msg.get("id"),
                "error": {
                    "code": -32001,
                    "message": "Session not found"
                }
            }))
            .into_response();
        }
        None => {
            let method = msg.get("method").and_then(|v| v.as_str());
            if method != Some("initialize") {
                return Json(json!({
                    "jsonrpc": "2.0",
                    "id": msg.get("id"),
                    "error": {
                        "code": -32002,
                        "message": "Not initialized. Call initialize first."
                    }
                }))
                .into_response();
            }
            let resp = state.server.handle_message(&msg).await;
            let (new_id, _rx) = state.sessions.create().await;
            (new_id, resp)
        }
    };

    if wants_sse {
        let (stream_id, rx) = state.sessions.create().await;

        if let Some(r) = response.clone() {
            let _ = state.sessions.send(&stream_id, r).await;
        }

        let sid_for_stream = stream_id.clone();
        let event_stream = ReceiverStream::new(rx).map(move |val| {
            let data = serde_json::to_string(&val).unwrap_or_default();
            Ok::<_, Infallible>(Event::default().id(sid_for_stream.clone()).data(data))
        });

        let mut resp = Sse::new(event_stream).into_response();
        resp.headers_mut().insert(
            "Mcp-Session-Id",
            HeaderValue::from_str(&stream_id).unwrap(),
        );
        resp.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_static("text/event-stream"),
        );
        resp
    } else {
        let mut resp = match response {
            Some(r) => Json(r).into_response(),
            None => Json(json!({})).into_response(),
        };
        resp.headers_mut().insert(
            "Mcp-Session-Id",
            HeaderValue::from_str(&final_session_id).unwrap(),
        );
        resp
    }
}

async fn handle_streamable_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session_id = headers
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if let Some(sid) = session_id {
        state.sessions.remove(&sid).await;
        eprintln!("[HTTP] Session deleted: {}", sid);
    }

    StatusCode::NO_CONTENT
}

// ============================================================
// Legacy SSE handlers (2024-11-05 / 2025-03-26)
// ============================================================

fn sse_event_stream(
    session_id: String,
    rx: mpsc::Receiver<Value>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/message?sessionId={}", session_id));

    let ping_stream = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(std::time::Duration::from_secs(30)),
    )
    .map(|_| Ok::<_, Infallible>(Event::default().event("ping").data("")));

    let message_stream = ReceiverStream::new(rx).map(|val| {
        let data = serde_json::to_string(&val).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("message").data(data))
    });

    // endpoint first, then merge ping and message streams concurrently
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
