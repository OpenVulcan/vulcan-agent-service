use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Transport-neutral summary of capabilities currently exposed by the host runtime.
/// 宿主运行时当前暴露能力的传输无关摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSurfaceSummary {
    /// Whether at least one runtime tool is available.
    /// 当前是否至少有一个运行时工具可用。
    pub has_tools: bool,
}

/// Transport-neutral schema for one runtime tool input object.
/// 单个运行时工具输入对象的传输无关 schema。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInputSchema {
    /// JSON schema type name.
    /// JSON schema 类型名称。
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Optional JSON schema properties.
    /// 可选 JSON schema 属性。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    /// Optional list of required property names.
    /// 可选必填属性名称列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Transport-neutral tool annotations used by host policy and protocol mappers.
/// 宿主策略与协议映射器使用的传输无关工具注解。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeToolAnnotations {
    /// Whether the tool is expected to avoid modifying user state.
    /// 工具是否预期不修改用户状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// Whether the tool may perform destructive or irreversible changes.
    /// 工具是否可能执行破坏性或不可逆更改。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// Whether a user confirmation is required before invocation.
    /// 调用前是否需要用户确认。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_confirmation_required: Option<bool>,
    /// Whether repeated identical calls are intended to be idempotent.
    /// 相同重复调用是否预期幂等。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
}

/// Transport-neutral runtime tool descriptor.
/// 传输无关的运行时工具描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeToolDescriptor {
    /// Canonical tool name exposed by the runtime.
    /// 运行时暴露的标准工具名称。
    pub name: String,
    /// Optional human-readable tool description.
    /// 可选的人类可读工具描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Input schema used by adapters to expose the tool.
    /// 适配器暴露工具时使用的输入 schema。
    pub input_schema: RuntimeInputSchema,
    /// Optional runtime-neutral tool annotations.
    /// 可选的运行时中立工具注解。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<RuntimeToolAnnotations>,
}

impl RuntimeToolDescriptor {
    /// Build one descriptor with an object input schema and explicit annotations.
    /// 构建一个带 object 输入 schema 与显式注解的工具描述。
    pub fn with_annotations(
        name: &str,
        description: &str,
        properties: Value,
        required: Vec<String>,
        annotations: RuntimeToolAnnotations,
    ) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: RuntimeInputSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(required),
            },
            annotations: Some(annotations),
        }
    }
}

/// Transport-neutral text content produced by runtime tool calls.
/// 运行时工具调用产生的传输无关文本内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTextContent {
    /// Text payload rendered by the runtime.
    /// 运行时渲染出的文本载荷。
    pub text: String,
}

impl RuntimeTextContent {
    /// Build one text content item.
    /// 构建一个文本内容项。
    pub fn text(text: &str) -> Self {
        Self {
            text: text.to_string(),
        }
    }
}

/// Transport-neutral runtime tool-call result.
/// 传输无关的运行时工具调用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolCallResult {
    /// Ordered text content blocks returned by the tool.
    /// 工具返回的有序文本内容块。
    pub content: Vec<RuntimeTextContent>,
    /// Optional error flag preserved for protocol mappers.
    /// 为协议映射器保留的可选错误标志。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Transport-neutral parsed runtime tool-call request.
/// 传输无关的已解析运行时工具调用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeToolCallRequest {
    /// Tool name requested by the caller.
    /// 调用方请求的工具名称。
    pub name: String,
    /// Optional tool arguments payload.
    /// 可选工具参数载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}
