use crate::config::model_config::OpenAiCompatibleModelConfig;
use serde::Serialize;
/// Skill caller metadata carried to model invocations for future auditing, accounting, and capability policy.
/// 传递给模型调用的技能调用者元数据，用于后续审计、统计与能力策略。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelInvocationContext {
    /// Skill identifier that initiated the model call.
    /// 发起模型调用的技能标识。
    pub skill_id: Option<String>,
    /// Local entry name inside the skill package.
    /// 技能包内部的本地入口名称。
    pub entry_name: Option<String>,
    /// Canonical MCP tool name mapped from the runtime entry.
    /// 从运行时入口映射得到的标准 MCP 工具名。
    pub canonical_tool_name: Option<String>,
    /// Runtime root name that supplied the calling skill.
    /// 提供调用技能的运行时根名称。
    pub root_name: Option<String>,
    /// Concrete skill directory path for diagnostics.
    /// 用于诊断的具体技能目录路径。
    pub skill_dir: Option<String>,
    /// Request client name supplied by the host transport.
    /// 宿主传输层提供的请求客户端名称。
    pub client_name: Option<String>,
    /// Host-generated request identifier when available.
    /// 可用时由宿主生成的请求标识。
    pub request_id: Option<String>,
}

/// User-facing status snapshot that LuaSkills can mirror through `vulcan.models.status()`.
/// 面向用户的状态快照，LuaSkills 后续可通过 `vulcan.models.status()` 映射。
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    /// Stable provider label.
    /// 稳定供应商标签。
    pub provider: String,
    /// Whether provider-level settings are globally enabled and minimally configured.
    /// 供应商级设置是否全局启用且具备最小可用配置。
    pub provider_ready: bool,
    /// Whether single-text embedding calls are available.
    /// 单文本向量调用是否可用。
    pub embed: bool,
    /// Whether one-shot LLM calls are available.
    /// 单轮 LLM 调用是否可用。
    pub llm: bool,
    /// Source path that contributed the current model configuration.
    /// 提供当前模型配置的来源路径。
    pub source_path: Option<String>,
}

/// Effective provider settings for one concrete model capability call.
/// 单个模型能力调用使用的实际供应商设置。
pub(super) struct OpenAiCapabilityProviderConfig {
    /// Parsed OpenAI-compatible configuration shared by request-body builders.
    /// 请求体构建器使用的已解析 OpenAI-compatible 配置。
    pub(super) config: OpenAiCompatibleModelConfig,
    /// Capability-specific API base URL from the selected capability configuration.
    /// 来自选中能力配置的单项能力 API 基础地址。
    pub(super) base_url: String,
    /// Capability-specific API key from the selected capability configuration.
    /// 来自选中能力配置的单项能力 API key。
    pub(super) api_key: String,
}

/// Token usage metadata normalized from OpenAI-compatible provider responses.
/// 从 OpenAI-compatible 供应商响应中规范化得到的 token 用量元数据。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ModelUsage {
    /// Input token count reported by the provider.
    /// 供应商报告的输入 token 数。
    pub input_tokens: Option<u64>,
    /// Output token count reported by the provider.
    /// 供应商报告的输出 token 数。
    pub output_tokens: Option<u64>,
    /// Total token count reported by the provider.
    /// 供应商报告的总 token 数。
    pub total_tokens: Option<u64>,
}

/// Stable host-side error codes for simplified model calls.
/// 简化模型调用使用的稳定宿主侧错误码。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelErrorCode {
    /// The requested model capability is not configured or disabled.
    /// 请求的模型能力未配置或已禁用。
    ModelUnavailable,
    /// The caller supplied invalid arguments.
    /// 调用方提供了非法参数。
    InvalidArgument,
    /// The model provider returned an error response or malformed success payload.
    /// 模型供应商返回错误响应或格式异常的成功载荷。
    ProviderError,
    /// The model provider call timed out.
    /// 模型供应商调用超时。
    Timeout,
    /// A host-side budget or quota rejected the call.
    /// 宿主侧预算或配额拒绝了调用。
    #[allow(dead_code)]
    BudgetExceeded,
    /// An unexpected host-side failure occurred.
    /// 发生未预期的宿主侧失败。
    InternalError,
}

/// Structured failure result returned by simplified model calls.
/// 简化模型调用返回的结构化失败结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelError {
    /// Stable host-side error code.
    /// 稳定宿主侧错误码。
    pub code: ModelErrorCode,
    /// Host-generated safe summary message.
    /// 宿主生成的安全摘要信息。
    pub message: String,
    /// Sanitized provider message or raw provider body when available.
    /// 可用时提供脱敏后的供应商信息或原始供应商响应体。
    pub provider_message: Option<String>,
    /// Provider-specific error code when available.
    /// 可用时提供供应商专用错误码。
    pub provider_code: Option<String>,
    /// Provider HTTP status code when available.
    /// 可用时提供供应商 HTTP 状态码。
    pub provider_status: Option<u16>,
}

/// Successful embedding result returned to the future LuaSkills model callback.
/// 返回给未来 LuaSkills 模型回调的成功向量结果。
#[derive(Debug, Clone, Serialize)]
pub struct ModelEmbedResponse {
    /// Embedding vector returned by the provider.
    /// 供应商返回的向量。
    pub vector: Vec<f32>,
    /// Vector dimension count.
    /// 向量维度数量。
    pub dimensions: usize,
    /// Optional token usage metadata.
    /// 可选 token 用量元数据。
    pub usage: Option<ModelUsage>,
}

/// Successful one-shot LLM result returned to the future LuaSkills model callback.
/// 返回给未来 LuaSkills 模型回调的成功单轮 LLM 结果。
#[derive(Debug, Clone, Serialize)]
pub struct ModelLlmResponse {
    /// Assistant text returned by the provider.
    /// 供应商返回的 assistant 文本。
    pub assistant: String,
    /// Optional token usage metadata.
    /// 可选 token 用量元数据。
    pub usage: Option<ModelUsage>,
}

impl ModelError {
    /// Build a host-side error without provider metadata.
    /// 构建不包含供应商元数据的宿主侧错误。
    pub(super) fn new(code: ModelErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            provider_message: None,
            provider_code: None,
            provider_status: None,
        }
    }

    /// Attach provider metadata to an existing host-side error.
    /// 为已有宿主侧错误附加供应商元数据。
    pub(super) fn with_provider(
        mut self,
        provider_message: Option<String>,
        provider_code: Option<String>,
        provider_status: Option<u16>,
    ) -> Self {
        self.provider_message = provider_message;
        self.provider_code = provider_code;
        self.provider_status = provider_status;
        self
    }
}
