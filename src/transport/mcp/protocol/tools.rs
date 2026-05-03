//! MCP tool DTOs and tool helper constructors.
//! MCP 工具 DTO 与工具辅助构造函数。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::content::Meta;

// Tools
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Tool annotations (2025-11-25)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// If true, the tool does not modify the user's system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// If true, the tool should be considered destructive / irreversible
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// If true, the tool must be confirmed by the user before calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_confirmation_required: Option<bool>,
    /// Unique ID for the tool (used for streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: InputSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _meta: Option<Meta>, // progress_token (2025-03-26+)
}

// ============================================================

impl Tool {
    pub fn new(name: &str, description: &str, properties: Value, required: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: InputSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(required),
            },
            annotations: None,
        }
    }

    pub fn with_annotations(
        name: &str,
        description: &str,
        properties: Value,
        required: Vec<String>,
        annotations: ToolAnnotations,
    ) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: InputSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(required),
            },
            annotations: Some(annotations),
        }
    }
}
