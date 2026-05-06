use serde_json::Value;

use crate::host_core::{HostRuntime, RuntimeSurfaceSummary};
use crate::transport::mcp::protocol::{
    InitializeRequest, InitializeResult, PROTOCOL_VERSION_COMPATIBLE, PROTOCOL_VERSION_LATEST,
    ServerCapabilities, ServerInfo, ToolCapability, negotiate_version,
};

/// Mark the MCP session initialized through the host runtime boundary.
/// 通过宿主运行时边界标记 MCP 会话已初始化。
pub(super) async fn mark_initialized(runtime: &HostRuntime) {
    runtime.mark_client_initialized().await;
}

/// Build the initialize response value used by the MCP dispatcher.
/// 构建 MCP dispatcher 使用的 initialize 响应值。
pub(super) fn initialize_value(
    runtime: &HostRuntime,
    params: Option<Value>,
) -> Result<Value, (i64, String)> {
    let req: InitializeRequest = serde_json::from_value(params.unwrap_or_default())
        .map_err(|error| (-32602, format!("Invalid initialize params: {}", error)))?;

    let negotiated = negotiate_version(&req.protocol_version).ok_or_else(|| {
        (
            -32602,
            format!(
                "Unsupported protocol version: {}. Supported: {}, {}",
                req.protocol_version,
                PROTOCOL_VERSION_LATEST,
                PROTOCOL_VERSION_COMPATIBLE.join(", ")
            ),
        )
    })?;

    let client_name = req
        .client_info
        .as_ref()
        .map(|client| client.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let surface = runtime.update_client_session(negotiated, req.capabilities)?;

    eprintln!("[MCP] Client: {} ({})", client_name, negotiated);
    let capabilities = mcp_capabilities(&surface, negotiated);
    eprintln!(
        "[MCP] Features: tools={}, tools_dynamic_notifications=false",
        surface.has_tools,
    );

    serde_json::to_value(InitializeResult {
        protocol_version: negotiated.to_string(),
        capabilities,
        server_info: ServerInfo {
            name: "vulcan-agent-service".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        instructions: Some(
            "Vulcan agent service MCP adapter supporting 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05. \
             This runtime currently exposes tools. \
             Prompts, resources, resource templates, roots, sampling, elicitation, logging, \
             progress, completions, and multi-modal content blocks are not advertised."
                .to_string(),
        ),
    })
    .map_err(|error| (-32603, format!("Initialize serialization error: {}", error)))
}

/// Convert the runtime surface snapshot into typed MCP server capabilities.
/// 将运行时能力快照转换为类型化 MCP 服务端 capabilities。
fn mcp_capabilities(surface: &RuntimeSurfaceSummary, _negotiated: &str) -> ServerCapabilities {
    ServerCapabilities {
        tools: surface.has_tools.then_some(ToolCapability {
            list_changed: Some(false),
        }),
    }
}
