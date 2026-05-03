//! MCP content block DTOs shared by tools, resources, prompts, and sampling.
//! 工具、资源、提示词与采样共享的 MCP 内容块 DTO。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::resources::ResourceContents;

// ============================================================
// Annotations (2025-11-25)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

// ============================================================
// Content types
// ============================================================

/// Content block used in tool results, resource reads, and prompt messages.
/// 2025-11-25 adds image and audio content types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(TextContent),
    #[serde(skip_serializing, skip_deserializing)] // only sent by server in 2025-11-25
    Image(ImageContent),
    #[serde(skip_serializing, skip_deserializing)] // only sent by server in 2025-11-25
    Audio(AudioContent),
    Resource(EmbeddedResource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    #[serde(default = "text_type_default")]
    pub r#type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub data: String,      // base64-encoded
    pub mime_type: String, // e.g. "image/png"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioContent {
    pub data: String,      // base64-encoded
    pub mime_type: String, // e.g. "audio/wav"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub resource: ResourceContents,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

// For tool results that only emit text content (simpler path)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<TextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ============================================================
// Meta (progress tokens — 2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<Value>, // string or number
}

// ============================================================

impl TextContent {
    pub fn text(text: &str) -> Self {
        Self {
            r#type: "text".to_string(),
            text: text.to_string(),
            annotations: None,
        }
    }
}

fn text_type_default() -> String {
    "text".to_string()
}
