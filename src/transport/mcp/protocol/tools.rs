//! MCP tool DTOs and tool helper constructors.
//! MCP 工具 DTO 与工具辅助构造函数。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP input schema for one exposed tool.
/// 单个对外工具的 MCP 输入 schema。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    /// JSON schema type name.
    /// JSON schema 类型名称。
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Optional JSON schema properties.
    /// 可选 JSON schema 属性。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    /// Optional required property names.
    /// 可选必填属性名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// MCP tool annotations used to communicate safety and behavior hints.
/// 用于传达安全与行为提示的 MCP 工具注解。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Whether the tool does not modify the user's system.
    /// 该工具是否不修改用户系统。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// Whether the tool should be considered destructive or irreversible.
    /// 该工具是否应被视为破坏性或不可逆。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// Whether the user must confirm before calling the tool.
    /// 调用该工具前是否必须获得用户确认。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_confirmation_required: Option<bool>,
    /// Whether identical repeated calls are expected to be idempotent.
    /// 相同重复调用是否预期幂等。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
}

/// MCP tool descriptor returned by tools/list.
/// tools/list 返回的 MCP 工具描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Canonical tool name.
    /// 标准工具名称。
    pub name: String,
    /// Optional human-readable tool description.
    /// 可选的人类可读工具描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tool input schema.
    /// 工具输入 schema。
    pub input_schema: InputSchema,
    /// Optional MCP tool annotations.
    /// 可选的 MCP 工具注解。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// MCP tool call request accepted by tools/call.
/// tools/call 接受的 MCP 工具调用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRequest {
    /// Tool name requested by the caller.
    /// 调用方请求的工具名称。
    pub name: String,
    /// Optional tool arguments payload.
    /// 可选工具参数载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}
