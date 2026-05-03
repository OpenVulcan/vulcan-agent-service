//! MCP server and client capability DTOs.
//! MCP 服务端与客户端能力 DTO。

use serde::{Deserialize, Serialize};

/// MCP tools capability advertised by the server.
/// 服务端声明的 MCP tools 能力。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapability {
    /// Whether clients should expect tools/list change notifications.
    /// 客户端是否应预期 tools/list 变更通知。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// MCP capabilities currently advertised by this host adapter.
/// 当前宿主适配器对外声明的 MCP 能力集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Optional tools capability when at least one tool is available.
    /// 至少存在一个工具时声明的可选 tools 能力。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapability>,
}
