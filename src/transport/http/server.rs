use axum::{
    Router,
    http::Method,
    routing::{delete, get, post},
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

use crate::host_core::McpServer;
use crate::transport::http::session::{SessionManager, SseSessionManager};
use crate::transport::mcp::McpDispatcher;

mod legacy_sse;
mod streamable;
#[cfg(test)]
mod tests;

use legacy_sse::{handle_sse_get, handle_sse_post};
use streamable::{handle_streamable_delete, handle_streamable_get, handle_streamable_post};

// ============================================================
// Application state shared with axum handlers
// Axum 处理器共享应用状态
// ============================================================

#[derive(Clone)]
pub struct AppState {
    /// MCP JSON-RPC dispatcher shared by all HTTP handlers.
    /// 所有 HTTP 处理器共享的 MCP JSON-RPC dispatcher。
    pub dispatcher: McpDispatcher,
    /// Streamable HTTP session manager.
    /// Streamable HTTP 会话管理器。
    pub sessions: SessionManager,
    /// Legacy SSE session manager.
    /// 旧版 SSE 会话管理器。
    pub sse_sessions: SseSessionManager,
}

// ============================================================
// HTTP transport runner
// HTTP 传输入口
// ============================================================

pub async fn run_http(server: McpServer, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Build the MCP dispatcher once so HTTP handlers depend on the transport adapter boundary.
    // 只构建一次 MCP dispatcher，使 HTTP 处理器依赖传输适配边界。
    let dispatcher = McpDispatcher::new(server);
    let state = AppState {
        dispatcher,
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
