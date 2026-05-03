//! MCP text content DTOs used by the current tools surface.
//! 当前 tools 能力面使用的 MCP 文本内容 DTO。

use serde::{Deserialize, Serialize};

/// Optional MCP annotations attached to text content.
/// 附加到文本内容上的可选 MCP 注解。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    /// Optional intended audience labels.
    /// 可选的目标受众标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    /// Optional priority value.
    /// 可选的优先级数值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
    /// Optional timestamp string.
    /// 可选的时间戳字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// MCP text content item used by tool results.
/// 工具结果使用的 MCP 文本内容项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    /// MCP content type; defaults to text when clients omit it.
    /// MCP 内容类型；客户端省略时默认为 text。
    #[serde(default = "text_type_default")]
    pub r#type: String,
    /// Text payload returned by the tool.
    /// 工具返回的文本载荷。
    pub text: String,
    /// Optional MCP content annotations.
    /// 可选的 MCP 内容注解。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// MCP tool result containing text content only.
/// 仅包含文本内容的 MCP 工具结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// Ordered text content blocks returned by the tool.
    /// 工具返回的有序文本内容块。
    pub content: Vec<TextContent>,
    /// Optional error flag required by MCP tool results.
    /// MCP 工具结果要求的可选错误标志。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl TextContent {
    /// Build one text content item without annotations.
    /// 构建一个不带注解的文本内容项。
    pub fn text(text: &str) -> Self {
        Self {
            r#type: "text".to_string(),
            text: text.to_string(),
            annotations: None,
        }
    }
}

/// Return the default MCP content type for text content.
/// 返回文本内容的默认 MCP 内容类型。
fn text_type_default() -> String {
    "text".to_string()
}
