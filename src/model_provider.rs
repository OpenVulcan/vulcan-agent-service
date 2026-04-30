use crate::model_config::{
    EffectiveModelConfig, OpenAiCompatibleModelConfig, current_effective_model_config,
    effective_embedding_api_key, effective_embedding_base_url, effective_llm_api_key,
    effective_llm_base_url,
};
use luaskills::{
    RuntimeModelCaller, RuntimeModelEmbedCallback, RuntimeModelEmbedRequest,
    RuntimeModelEmbedResponse, RuntimeModelError, RuntimeModelErrorCode, RuntimeModelLlmCallback,
    RuntimeModelLlmRequest, RuntimeModelLlmResponse, RuntimeModelUsage, set_model_embed_callback,
    set_model_llm_callback,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Default timeout in milliseconds for model provider calls when a capability does not override it.
/// 当单项能力未覆盖时，模型供应商调用使用的默认超时时间，单位为毫秒。
const DEFAULT_MODEL_TIMEOUT_MS: u64 = 60_000;
/// Stable label for the only provider family currently implemented by the host.
/// 当前宿主已实现的唯一供应商族稳定标签。
const OPENAI_COMPATIBLE_PROVIDER_LABEL: &str = "openai_compatible";

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

/// Runtime-visible model capability names.
/// 运行时可见的模型能力名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCapability {
    /// Single-text embedding capability.
    /// 单文本向量能力。
    Embed,
    /// One-shot non-streaming LLM capability.
    /// 单轮非流式 LLM 能力。
    Llm,
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
struct OpenAiCapabilityProviderConfig {
    /// Parsed OpenAI-compatible configuration shared by request-body builders.
    /// 请求体构建器使用的已解析 OpenAI-compatible 配置。
    config: OpenAiCompatibleModelConfig,
    /// Capability-specific API base URL from the selected capability configuration.
    /// 来自选中能力配置的单项能力 API 基础地址。
    base_url: String,
    /// Capability-specific API key from the selected capability configuration.
    /// 来自选中能力配置的单项能力 API key。
    api_key: String,
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
    fn new(code: ModelErrorCode, message: impl Into<String>) -> Self {
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
    fn with_provider(
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

/// Return the current model capability status without exposing secrets.
/// 返回当前模型能力状态，且不暴露密钥。
pub fn model_status() -> ModelStatus {
    let effective = current_effective_model_config();
    let provider = &effective.config.openai_compatible;
    let embed_ready = provider.enabled
        && provider.embedding.enabled
        && normalized_optional_text(provider.embedding.model.as_deref()).is_some()
        && effective_embedding_base_url(&effective).is_some()
        && effective_embedding_api_key(&effective).is_some();
    let llm_ready = provider.enabled
        && provider.llm.enabled
        && normalized_optional_text(provider.llm.model.as_deref()).is_some()
        && effective_llm_base_url(&effective).is_some()
        && effective_llm_api_key(&effective).is_some();
    ModelStatus {
        provider: OPENAI_COMPATIBLE_PROVIDER_LABEL.to_string(),
        provider_ready: provider.enabled && (embed_ready || llm_ready),
        embed: embed_ready,
        llm: llm_ready,
        source_path: effective
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
    }
}

/// Return whether one simplified model capability is currently available.
/// 返回某个简化模型能力当前是否可用。
pub fn has_model_capability(capability: ModelCapability) -> bool {
    let status = model_status();
    match capability {
        ModelCapability::Embed => status.embed,
        ModelCapability::Llm => status.llm,
    }
}

/// Register or clear LuaSkills model callbacks based on the currently loaded host model configuration.
/// 根据当前已加载的宿主模型配置注册或清理 LuaSkills 模型回调。
pub fn install_luaskills_model_callbacks() {
    if has_model_capability(ModelCapability::Embed) {
        let embed_callback: RuntimeModelEmbedCallback =
            Arc::new(|request: &RuntimeModelEmbedRequest| {
                let context = model_invocation_context_from_runtime_caller(&request.caller);
                model_embed(&request.text, Some(&context))
                    .map(runtime_embed_response_from_model_response)
                    .map_err(runtime_model_error_from_model_error)
            });
        set_model_embed_callback(Some(embed_callback));
    } else {
        set_model_embed_callback(None);
    }

    if has_model_capability(ModelCapability::Llm) {
        let llm_callback: RuntimeModelLlmCallback = Arc::new(|request: &RuntimeModelLlmRequest| {
            let context = model_invocation_context_from_runtime_caller(&request.caller);
            model_llm(&request.system, &request.user, Some(&context))
                .map(runtime_llm_response_from_model_response)
                .map_err(runtime_model_error_from_model_error)
        });
        set_model_llm_callback(Some(llm_callback));
    } else {
        set_model_llm_callback(None);
    }
}

/// Execute a single-text embedding request through the host-owned provider configuration.
/// 通过宿主管理的供应商配置执行一次单文本向量请求。
pub fn model_embed(
    text: &str,
    context: Option<&ModelInvocationContext>,
) -> Result<ModelEmbedResponse, ModelError> {
    let _context = context;
    let input_text = require_non_empty_argument(text, "text")?;
    let effective = current_effective_model_config();
    let provider = require_openai_provider_for_embedding(&effective)?;
    let timeout_ms = provider
        .config
        .embedding
        .timeout_ms
        .unwrap_or(DEFAULT_MODEL_TIMEOUT_MS);
    let client = build_blocking_client(timeout_ms)?;
    let request_body = build_embedding_request_body(&provider.config, &input_text)?;
    let url = openai_endpoint_url(&provider.base_url, "embeddings")?;
    let response_body =
        post_json_value(&client, &url, &provider.api_key, &request_body, timeout_ms)?;
    parse_embedding_response(&response_body)
}

/// Execute a one-shot non-streaming LLM request through the host-owned provider configuration.
/// 通过宿主管理的供应商配置执行一次单轮非流式 LLM 请求。
pub fn model_llm(
    system: &str,
    user: &str,
    context: Option<&ModelInvocationContext>,
) -> Result<ModelLlmResponse, ModelError> {
    let _context = context;
    let system_prompt = require_non_empty_argument(system, "system")?;
    let user_prompt = require_non_empty_argument(user, "user")?;
    let effective = current_effective_model_config();
    let provider = require_openai_provider_for_llm(&effective)?;
    let timeout_ms = provider
        .config
        .llm
        .timeout_ms
        .unwrap_or(DEFAULT_MODEL_TIMEOUT_MS);
    let client = build_blocking_client(timeout_ms)?;
    let request_body = build_llm_request_body(&provider.config, &system_prompt, &user_prompt)?;
    let url = openai_endpoint_url(&provider.base_url, "chat/completions")?;
    let response_body =
        post_json_value(&client, &url, &provider.api_key, &request_body, timeout_ms)?;
    parse_llm_response(&response_body)
}

/// Convert a LuaSkills runtime caller object into the host-side model invocation context.
/// 将 LuaSkills 运行时调用方对象转换为宿主侧模型调用上下文。
fn model_invocation_context_from_runtime_caller(
    caller: &RuntimeModelCaller,
) -> ModelInvocationContext {
    ModelInvocationContext {
        skill_id: caller.skill_id.clone(),
        entry_name: caller.entry_name.clone(),
        canonical_tool_name: caller.canonical_tool_name.clone(),
        root_name: caller.root_name.clone(),
        skill_dir: caller.skill_dir.clone(),
        client_name: caller.client_name.clone(),
        request_id: caller.request_id.clone(),
    }
}

/// Convert one host embedding response into the LuaSkills runtime response type.
/// 将单个宿主向量响应转换为 LuaSkills 运行时响应类型。
fn runtime_embed_response_from_model_response(
    response: ModelEmbedResponse,
) -> RuntimeModelEmbedResponse {
    RuntimeModelEmbedResponse {
        vector: response.vector,
        dimensions: response.dimensions,
        usage: response.usage.map(runtime_usage_from_model_usage),
    }
}

/// Convert one host LLM response into the LuaSkills runtime response type.
/// 将单个宿主 LLM 响应转换为 LuaSkills 运行时响应类型。
fn runtime_llm_response_from_model_response(response: ModelLlmResponse) -> RuntimeModelLlmResponse {
    RuntimeModelLlmResponse {
        assistant: response.assistant,
        usage: response.usage.map(runtime_usage_from_model_usage),
    }
}

/// Convert normalized host usage metadata into the LuaSkills runtime usage type.
/// 将宿主规范化用量元数据转换为 LuaSkills 运行时用量类型。
fn runtime_usage_from_model_usage(usage: ModelUsage) -> RuntimeModelUsage {
    RuntimeModelUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
    }
}

/// Convert one host model error into the LuaSkills runtime error type.
/// 将单个宿主模型错误转换为 LuaSkills 运行时错误类型。
fn runtime_model_error_from_model_error(error: ModelError) -> RuntimeModelError {
    RuntimeModelError {
        code: runtime_model_error_code_from_model_error_code(error.code),
        message: error.message,
        provider_message: error.provider_message,
        provider_code: error.provider_code,
        provider_status: error.provider_status,
    }
}

/// Convert a host-side model error code into the LuaSkills runtime error code.
/// 将宿主侧模型错误码转换为 LuaSkills 运行时错误码。
fn runtime_model_error_code_from_model_error_code(code: ModelErrorCode) -> RuntimeModelErrorCode {
    match code {
        ModelErrorCode::ModelUnavailable => RuntimeModelErrorCode::ModelUnavailable,
        ModelErrorCode::InvalidArgument => RuntimeModelErrorCode::InvalidArgument,
        ModelErrorCode::ProviderError => RuntimeModelErrorCode::ProviderError,
        ModelErrorCode::Timeout => RuntimeModelErrorCode::Timeout,
        ModelErrorCode::BudgetExceeded => RuntimeModelErrorCode::BudgetExceeded,
        ModelErrorCode::InternalError => RuntimeModelErrorCode::InternalError,
    }
}

/// Require that the OpenAI-compatible provider and embedding capability are both enabled and configured.
/// 要求 OpenAI-compatible 供应商与向量能力均已启用且配置完整。
fn require_openai_provider_for_embedding(
    effective: &EffectiveModelConfig,
) -> Result<OpenAiCapabilityProviderConfig, ModelError> {
    let provider = require_provider_enabled(effective)?;
    if !provider.embedding.enabled {
        return Err(ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "embedding capability is disabled",
        ));
    }
    if normalized_optional_text(provider.embedding.model.as_deref()).is_none() {
        return Err(ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "embedding model is not configured",
        ));
    }
    let base_url = effective_embedding_base_url(effective).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "embedding model provider base_url is not configured",
        )
    })?;
    let api_key = effective_embedding_api_key(effective).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "embedding model provider api_key is not configured",
        )
    })?;
    Ok(OpenAiCapabilityProviderConfig {
        config: provider,
        base_url,
        api_key,
    })
}

/// Require that the OpenAI-compatible provider and LLM capability are both enabled and configured.
/// 要求 OpenAI-compatible 供应商与 LLM 能力均已启用且配置完整。
fn require_openai_provider_for_llm(
    effective: &EffectiveModelConfig,
) -> Result<OpenAiCapabilityProviderConfig, ModelError> {
    let provider = require_provider_enabled(effective)?;
    if !provider.llm.enabled {
        return Err(ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "llm capability is disabled",
        ));
    }
    if normalized_optional_text(provider.llm.model.as_deref()).is_none() {
        return Err(ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "llm model is not configured",
        ));
    }
    let base_url = effective_llm_base_url(effective).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "llm model provider base_url is not configured",
        )
    })?;
    let api_key = effective_llm_api_key(effective).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "llm model provider api_key is not configured",
        )
    })?;
    Ok(OpenAiCapabilityProviderConfig {
        config: provider,
        base_url,
        api_key,
    })
}

/// Require that the OpenAI-compatible provider is globally enabled.
/// 要求 OpenAI-compatible 供应商已全局启用。
fn require_provider_enabled(
    effective: &EffectiveModelConfig,
) -> Result<OpenAiCompatibleModelConfig, ModelError> {
    let provider = effective.config.openai_compatible.clone();
    if !provider.enabled {
        return Err(ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "model provider is disabled",
        ));
    }
    Ok(provider)
}

/// Require a non-empty string argument from the future Lua-facing callback.
/// 要求未来 Lua 回调传入非空字符串参数。
fn require_non_empty_argument(value: &str, field_name: &str) -> Result<String, ModelError> {
    normalized_optional_text(Some(value)).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::InvalidArgument,
            format!("model argument `{}` must be a non-empty string", field_name),
        )
    })
}

/// Build a blocking HTTP client with a capability-specific timeout.
/// 构建带单项能力超时时间的阻塞 HTTP 客户端。
fn build_blocking_client(timeout_ms: u64) -> Result<Client, ModelError> {
    Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(1)))
        .build()
        .map_err(|error| {
            ModelError::new(
                ModelErrorCode::InternalError,
                format!("failed to build model HTTP client: {}", error),
            )
        })
}

/// Build an OpenAI-compatible embedding request body from host configuration.
/// 基于宿主配置构建 OpenAI-compatible 向量请求体。
fn build_embedding_request_body(
    provider: &OpenAiCompatibleModelConfig,
    text: &str,
) -> Result<Value, ModelError> {
    let model = normalized_optional_text(provider.embedding.model.as_deref()).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "embedding model is not configured",
        )
    })?;
    let mut body = json!({
        "model": model,
        "input": text,
    });
    merge_request_overrides(
        &mut body,
        &provider.embedding.request_overrides,
        &["model", "input"],
    );
    Ok(body)
}

/// Build an OpenAI-compatible chat-completion request body from host configuration.
/// 基于宿主配置构建 OpenAI-compatible chat-completion 请求体。
fn build_llm_request_body(
    provider: &OpenAiCompatibleModelConfig,
    system: &str,
    user: &str,
) -> Result<Value, ModelError> {
    let model = normalized_optional_text(provider.llm.model.as_deref()).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "llm model is not configured",
        )
    })?;
    let mut body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "stream": false,
    });
    if let Some(temperature) = provider.llm.temperature.filter(|value| value.is_finite()) {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = provider.llm.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    merge_request_overrides(
        &mut body,
        &provider.llm.request_overrides,
        &["model", "messages", "stream"],
    );
    body["stream"] = json!(false);
    Ok(body)
}

/// Merge host-owned provider-specific request overrides while preserving reserved protocol fields.
/// 合并宿主管理的供应商专用请求覆盖，同时保留协议保留字段。
fn merge_request_overrides(body: &mut Value, overrides: &Value, reserved_keys: &[&str]) {
    let (Value::Object(body_map), Value::Object(override_map)) = (body, overrides) else {
        return;
    };
    for (key, value) in override_map {
        if reserved_keys
            .iter()
            .any(|reserved| *reserved == key.as_str())
        {
            continue;
        }
        body_map.insert(key.clone(), value.clone());
    }
}

/// Join an OpenAI-compatible API base URL with one endpoint path.
/// 将 OpenAI-compatible API 基础地址与单个端点路径拼接。
fn openai_endpoint_url(base_url: &str, endpoint: &str) -> Result<String, ModelError> {
    let base = normalized_optional_text(Some(base_url)).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::ModelUnavailable,
            "model provider base_url is not configured",
        )
    })?;
    Ok(format!(
        "{}/{}",
        base.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    ))
}

/// Post a JSON body and return a parsed JSON value, preserving provider errors on failure.
/// 发送 JSON 请求并返回解析后的 JSON 值，失败时保留供应商错误信息。
fn post_json_value(
    client: &Client,
    url: &str,
    api_key: &str,
    request_body: &Value,
    timeout_ms: u64,
) -> Result<Value, ModelError> {
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(request_body)
        .send()
        .map_err(|error| map_reqwest_error(error, api_key))?;
    let status = response.status();
    let response_text = response.text().map_err(|error| {
        ModelError::new(
            ModelErrorCode::ProviderError,
            format!("failed to read model provider response: {}", error),
        )
    })?;
    if !status.is_success() {
        return Err(provider_error_from_http_status(
            status,
            &response_text,
            api_key,
        ));
    }
    serde_json::from_str::<Value>(&response_text).map_err(|error| {
        ModelError::new(
            ModelErrorCode::ProviderError,
            format!(
                "model provider returned invalid JSON after {} ms timeout window: {}",
                timeout_ms, error
            ),
        )
        .with_provider(
            normalized_optional_text(Some(&sanitize_provider_text(&response_text, api_key))),
            None,
            Some(status.as_u16()),
        )
    })
}

/// Map a reqwest transport error into the simplified model error envelope.
/// 将 reqwest 传输错误映射到简化模型错误结构。
fn map_reqwest_error(error: reqwest::Error, api_key: &str) -> ModelError {
    let sanitized = sanitize_provider_text(&error.to_string(), api_key);
    if error.is_timeout() {
        ModelError::new(ModelErrorCode::Timeout, "model provider request timed out").with_provider(
            normalized_optional_text(Some(&sanitized)),
            None,
            None,
        )
    } else {
        ModelError::new(
            ModelErrorCode::ProviderError,
            "model provider request failed",
        )
        .with_provider(normalized_optional_text(Some(&sanitized)), None, None)
    }
}

/// Build a provider-error envelope from an HTTP non-success response.
/// 根据 HTTP 非成功响应构建供应商错误结构。
fn provider_error_from_http_status(status: StatusCode, body: &str, api_key: &str) -> ModelError {
    let sanitized_body = sanitize_provider_text(body, api_key);
    let parsed_body = serde_json::from_str::<Value>(&sanitized_body).ok();
    let provider_message = parsed_body
        .as_ref()
        .and_then(extract_provider_message)
        .or_else(|| normalized_optional_text(Some(&sanitized_body)));
    let provider_code = parsed_body.as_ref().and_then(extract_provider_code);
    ModelError::new(
        ModelErrorCode::ProviderError,
        format!("model provider returned HTTP {}", status.as_u16()),
    )
    .with_provider(provider_message, provider_code, Some(status.as_u16()))
}

/// Extract a provider message from common OpenAI-compatible error shapes.
/// 从常见 OpenAI-compatible 错误结构中提取供应商消息。
fn extract_provider_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(Value::as_str))
        .and_then(|message| normalized_optional_text(Some(message)))
}

/// Extract a provider code from common OpenAI-compatible error shapes.
/// 从常见 OpenAI-compatible 错误结构中提取供应商错误码。
fn extract_provider_code(value: &Value) -> Option<String> {
    let code_value = value
        .get("error")
        .and_then(|error| error.get("code"))
        .or_else(|| value.get("code"))?;
    match code_value {
        Value::String(text) => normalized_optional_text(Some(text)),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Parse one embedding success response into the simplified result envelope.
/// 将单个向量成功响应解析为简化结果结构。
fn parse_embedding_response(value: &Value) -> Result<ModelEmbedResponse, ModelError> {
    let embedding_value = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("embedding"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ModelError::new(
                ModelErrorCode::ProviderError,
                "model provider embedding response did not contain data[0].embedding",
            )
        })?;
    let mut vector = Vec::with_capacity(embedding_value.len());
    for item in embedding_value {
        let number = item.as_f64().ok_or_else(|| {
            ModelError::new(
                ModelErrorCode::ProviderError,
                "model provider embedding vector contained a non-number value",
            )
        })?;
        vector.push(number as f32);
    }
    Ok(ModelEmbedResponse {
        dimensions: vector.len(),
        vector,
        usage: parse_usage(value),
    })
}

/// Parse one chat-completion success response into the simplified LLM result envelope.
/// 将单个 chat-completion 成功响应解析为简化 LLM 结果结构。
fn parse_llm_response(value: &Value) -> Result<ModelLlmResponse, ModelError> {
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(assistant_content_to_string)
        .ok_or_else(|| {
            ModelError::new(
                ModelErrorCode::ProviderError,
                "model provider LLM response did not contain choices[0].message.content",
            )
        })?;
    Ok(ModelLlmResponse {
        assistant: content,
        usage: parse_usage(value),
    })
}

/// Convert common assistant content shapes into one final assistant text.
/// 将常见 assistant content 形态转换为最终 assistant 文本。
fn assistant_content_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut rendered_parts = Vec::new();
            for item in items {
                if let Some(text) = item
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
                {
                    rendered_parts.push(text.to_string());
                }
            }
            Some(rendered_parts.join(""))
        }
        _ => None,
    }
}

/// Parse OpenAI-compatible usage fields into normalized usage metadata.
/// 将 OpenAI-compatible usage 字段解析为统一用量元数据。
fn parse_usage(value: &Value) -> Option<ModelUsage> {
    let usage = value.get("usage")?;
    let input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.get("input_tokens").and_then(Value::as_u64));
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.get("output_tokens").and_then(Value::as_u64));
    let total_tokens = usage.get("total_tokens").and_then(Value::as_u64);
    Some(ModelUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

/// Replace known secret material from provider text before it enters errors or logs.
/// 在供应商文本进入错误或日志前替换已知密钥内容。
fn sanitize_provider_text(text: &str, api_key: &str) -> String {
    if api_key.trim().is_empty() {
        return text.to_string();
    }
    text.replace(&format!("Bearer {}", api_key), "Bearer ***")
        .replace(api_key, "***")
}

/// Normalize optional text by trimming whitespace and dropping blank values.
/// 规范化可选文本：去除首尾空白并丢弃空值。
fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_config::{EmbeddingModelConfig, LlmModelConfig};
    use crate::model_config::{initialize_model_config_runtime_root, preload_model_config};
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::thread::JoinHandle;

    /// Captured one-shot HTTP request received by the local mock model provider.
    /// 本地模型供应商假服务收到的单次 HTTP 请求记录。
    struct MockProviderRequest {
        /// Request path including the OpenAI-compatible API endpoint path.
        /// 请求路径，包含 OpenAI-compatible API 端点路径。
        path: String,
        /// Authorization header captured from the request.
        /// 从请求中捕获的 Authorization 请求头。
        authorization: Option<String>,
        /// JSON request body captured from the request.
        /// 从请求中捕获的 JSON 请求体。
        body: Value,
    }

    /// Return one shared mutex used to serialize process-wide LuaSkills model callback tests.
    /// 返回一个共享互斥锁，用于串行化进程级 LuaSkills 模型回调测试。
    fn callback_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Build one unique temporary directory path for a model-provider test case.
    /// 为模型供应商测试用例构建唯一临时目录路径。
    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-mcp-model-provider-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ))
    }

    /// Build one minimal LuaSkills engine for checking the Lua-facing model capability surface.
    /// 构建一个最小 LuaSkills 引擎，用于检查 Lua 面向模型能力面。
    fn make_lua_engine() -> LuaEngine {
        LuaEngine::new(LuaEngineOptions {
            host_options: LuaRuntimeHostOptions::default(),
            pool_config: LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
        })
        .expect("Lua engine should be created")
    }

    /// Restore model config discovery to the repository template and re-apply callback registration.
    /// 将模型配置发现恢复到仓库模板，并重新应用回调注册。
    fn restore_default_model_callbacks() {
        let _ = initialize_model_config_runtime_root(None);
        let _ = preload_model_config();
        install_luaskills_model_callbacks();
    }

    /// Start a one-shot local HTTP server that returns the provided OpenAI-compatible response body.
    /// 启动一个单次使用的本地 HTTP 服务，并返回指定 OpenAI-compatible 响应体。
    fn start_mock_provider_server(
        response_body: Value,
    ) -> (String, JoinHandle<MockProviderRequest>) {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("mock provider should bind a local port");
        let base_url = format!(
            "http://{}/v1",
            listener
                .local_addr()
                .expect("mock provider local address should resolve")
        );
        let response_text = response_body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("mock provider should receive one request");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .expect("mock provider read timeout should set");
            let request_bytes = read_http_request_bytes(&mut stream);
            let captured_request = parse_mock_provider_request(&request_bytes);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_text.len(),
                response_text
            );
            stream
                .write_all(response.as_bytes())
                .expect("mock provider response should write");
            captured_request
        });
        (base_url, handle)
    }

    /// Read exactly one HTTP request from the mock provider stream.
    /// 从假服务连接中读取完整的单个 HTTP 请求。
    fn read_http_request_bytes(stream: &mut TcpStream) -> Vec<u8> {
        let mut request_bytes = Vec::new();
        loop {
            if let Some(expected_len) = expected_http_request_len(&request_bytes)
                && request_bytes.len() >= expected_len
            {
                request_bytes.truncate(expected_len);
                return request_bytes;
            }
            let mut chunk = [0_u8; 512];
            let read_len = stream
                .read(&mut chunk)
                .expect("mock provider request should read");
            if read_len == 0 {
                return request_bytes;
            }
            request_bytes.extend_from_slice(&chunk[..read_len]);
        }
    }

    /// Return the total expected request byte length once headers and Content-Length are available.
    /// 在请求头与 Content-Length 可用后返回预期完整请求字节长度。
    fn expected_http_request_len(request_bytes: &[u8]) -> Option<usize> {
        let header_end = http_header_end_index(request_bytes)?;
        let header_text = std::str::from_utf8(&request_bytes[..header_end]).ok()?;
        let content_length = header_text
            .lines()
            .find_map(|line| parse_content_length_header(line))
            .unwrap_or(0);
        Some(header_end + content_length)
    }

    /// Return the byte index immediately after the HTTP header terminator.
    /// 返回 HTTP 请求头结束符之后的字节索引。
    fn http_header_end_index(request_bytes: &[u8]) -> Option<usize> {
        request_bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
    }

    /// Parse one Content-Length header line.
    /// 解析单行 Content-Length 请求头。
    fn parse_content_length_header(line: &str) -> Option<usize> {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    }

    /// Parse one captured HTTP request into the compact mock-provider request struct.
    /// 将捕获到的 HTTP 请求解析为紧凑的假服务请求结构。
    fn parse_mock_provider_request(request_bytes: &[u8]) -> MockProviderRequest {
        let request_text =
            std::str::from_utf8(request_bytes).expect("mock provider request should be UTF-8");
        let (header_text, body_text) = request_text
            .split_once("\r\n\r\n")
            .expect("mock provider request should contain headers");
        let path = header_text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .expect("mock provider request path should exist")
            .to_string();
        let authorization = header_text.lines().find_map(parse_authorization_header);
        let body = serde_json::from_str::<Value>(body_text)
            .expect("mock provider request body should be JSON");
        MockProviderRequest {
            path,
            authorization,
            body,
        }
    }

    /// Parse one Authorization header line.
    /// 解析单行 Authorization 请求头。
    fn parse_authorization_header(line: &str) -> Option<String> {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("authorization") {
            Some(value.trim().to_string())
        } else {
            None
        }
    }

    /// Write an isolated model configuration file for one mock-provider integration test.
    /// 为单个假服务集成测试写入隔离模型配置文件。
    fn write_mock_model_config(root: &std::path::Path, base_url: &str, body: &str) {
        let config_path = root.join("configs").join("model_config.yaml");
        std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
            .expect("model config directory should be created");
        std::fs::write(&config_path, body.replace("${MOCK_BASE_URL}", base_url))
            .expect("model config should be written");
    }

    /// Build one provider fixture with both embedding and LLM enabled.
    /// 构建同时启用向量与 LLM 的供应商测试夹具。
    fn provider_fixture() -> OpenAiCompatibleModelConfig {
        OpenAiCompatibleModelConfig {
            enabled: true,
            embedding: EmbeddingModelConfig {
                enabled: true,
                base_url: Some("https://embedding.example.test/v1/".to_string()),
                api_key: Some("sk-embed".to_string()),
                model: Some("embed-small".to_string()),
                timeout_ms: Some(1000),
                request_overrides: json!({
                    "model": "must-not-win",
                    "input": "must-not-win",
                    "encoding_format": "float"
                }),
            },
            llm: LlmModelConfig {
                enabled: true,
                base_url: Some("https://llm.example.test/v1/".to_string()),
                api_key: Some("sk-llm".to_string()),
                model: Some("llm-small".to_string()),
                temperature: Some(0.1),
                max_tokens: Some(300),
                timeout_ms: Some(1000),
                request_overrides: json!({
                    "model": "must-not-win",
                    "messages": [],
                    "stream": true,
                    "enable_thinking": false
                }),
            },
        }
    }

    /// Embedding request bodies should preserve host-owned model/input while accepting safe overrides.
    /// 向量请求体应保留宿主管理的 model/input，同时接受安全覆盖字段。
    #[test]
    fn embedding_request_body_preserves_reserved_fields() {
        let body = build_embedding_request_body(&provider_fixture(), "hello")
            .expect("embedding body should build");

        assert_eq!(body["model"], "embed-small");
        assert_eq!(body["input"], "hello");
        assert_eq!(body["encoding_format"], "float");
    }

    /// LLM request bodies should stay non-streaming and reject reserved override fields.
    /// LLM 请求体应保持非流式，并拒绝保留字段覆盖。
    #[test]
    fn llm_request_body_forces_non_streaming_and_reserved_fields() {
        let body = build_llm_request_body(&provider_fixture(), "sys", "user")
            .expect("llm body should build");

        assert_eq!(body["model"], "llm-small");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "user");
        assert_eq!(body["enable_thinking"], false);
    }

    /// OpenAI-compatible embedding responses should parse vector dimensions and usage.
    /// OpenAI-compatible 向量响应应解析向量维度与用量信息。
    #[test]
    fn embedding_response_parses_vector_and_usage() {
        let response = json!({
            "data": [
                { "embedding": [0.25, -1.0, 2.5] }
            ],
            "usage": {
                "prompt_tokens": 7,
                "total_tokens": 7
            }
        });

        let parsed = parse_embedding_response(&response).expect("embedding response should parse");

        assert_eq!(parsed.dimensions, 3);
        assert_eq!(parsed.vector, vec![0.25, -1.0, 2.5]);
        assert_eq!(
            parsed.usage,
            Some(ModelUsage {
                input_tokens: Some(7),
                output_tokens: None,
                total_tokens: Some(7),
            })
        );
    }

    /// OpenAI-compatible LLM responses should parse assistant text and usage.
    /// OpenAI-compatible LLM 响应应解析 assistant 文本与用量信息。
    #[test]
    fn llm_response_parses_assistant_text_and_usage() {
        let response = json!({
            "choices": [
                { "message": { "content": "done" } }
            ],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 2,
                "total_tokens": 7
            }
        });

        let parsed = parse_llm_response(&response).expect("llm response should parse");

        assert_eq!(parsed.assistant, "done");
        assert_eq!(
            parsed.usage,
            Some(ModelUsage {
                input_tokens: Some(5),
                output_tokens: Some(2),
                total_tokens: Some(7),
            })
        );
    }

    /// Provider HTTP errors should preserve sanitized provider message, code, and status.
    /// 供应商 HTTP 错误应保留脱敏后的供应商消息、错误码与状态码。
    #[test]
    fn provider_error_preserves_sanitized_provider_fields() {
        let error = provider_error_from_http_status(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"bad sk-secret","code":"bad_request"}}"#,
            "sk-secret",
        );

        assert_eq!(error.code, ModelErrorCode::ProviderError);
        assert_eq!(error.provider_message.as_deref(), Some("bad ***"));
        assert_eq!(error.provider_code.as_deref(), Some("bad_request"));
        assert_eq!(error.provider_status, Some(400));
    }

    /// Model errors should serialize into the stable Lua-facing error object shape.
    /// 模型错误应序列化为稳定的 Lua 面向错误对象形态。
    #[test]
    fn model_error_serializes_stable_lua_error_shape() {
        let value = json!({
            "ok": false,
            "error": ModelError::new(ModelErrorCode::ModelUnavailable, "missing")
        });

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "model_unavailable");
        assert_eq!(value["error"]["message"], "missing");
    }

    /// Endpoint joining should trim duplicate slashes while keeping the configured API base path.
    /// 端点拼接应去除重复斜杠，同时保留配置中的 API 基础路径。
    #[test]
    fn endpoint_url_preserves_base_path() {
        let url = openai_endpoint_url("https://example.test/compatible-mode/v1/", "/embeddings")
            .expect("url should build");

        assert_eq!(url, "https://example.test/compatible-mode/v1/embeddings");
    }

    /// Embedding calls should send a real OpenAI-compatible HTTP request and parse the mock provider response.
    /// 向量调用应发送真实 OpenAI-compatible HTTP 请求，并解析假供应商响应。
    #[test]
    fn model_embed_posts_to_openai_compatible_endpoint() {
        let _guard = callback_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = unique_test_dir("mock-embed");
        let (base_url, handle) = start_mock_provider_server(json!({
            "data": [
                { "embedding": [0.5, 0.25, -0.75] }
            ],
            "usage": {
                "prompt_tokens": 9,
                "total_tokens": 9
            }
        }));
        write_mock_model_config(
            &root,
            &base_url,
            r#"
openai_compatible:
  enabled: true
  embedding:
    enabled: true
    base_url: "${MOCK_BASE_URL}"
    api_key: "sk-embed-local"
    model: "embed-small"
    timeout_ms: 5000
    request_overrides:
      encoding_format: "float"
"#,
        );
        initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
        preload_model_config().expect("model config should preload");

        let response = model_embed("hello", None).expect("embedding call should succeed");
        let request = handle.join().expect("mock provider should finish");

        assert_eq!(response.vector, vec![0.5, 0.25, -0.75]);
        assert_eq!(response.dimensions, 3);
        assert_eq!(
            response.usage,
            Some(ModelUsage {
                input_tokens: Some(9),
                output_tokens: None,
                total_tokens: Some(9),
            })
        );
        assert_eq!(request.path, "/v1/embeddings");
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer sk-embed-local")
        );
        assert_eq!(request.body["model"], "embed-small");
        assert_eq!(request.body["input"], "hello");
        assert_eq!(request.body["encoding_format"], "float");

        restore_default_model_callbacks();
        let _ = std::fs::remove_dir_all(root);
    }

    /// LLM calls should send a non-streaming OpenAI-compatible chat request and parse the mock provider response.
    /// LLM 调用应发送非流式 OpenAI-compatible chat 请求，并解析假供应商响应。
    #[test]
    fn model_llm_posts_non_streaming_chat_completion() {
        let _guard = callback_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = unique_test_dir("mock-llm");
        let (base_url, handle) = start_mock_provider_server(json!({
            "choices": [
                { "message": { "content": "mock assistant" } }
            ],
            "usage": {
                "prompt_tokens": 11,
                "completion_tokens": 4,
                "total_tokens": 15
            }
        }));
        write_mock_model_config(
            &root,
            &base_url,
            r#"
openai_compatible:
  enabled: true
  llm:
    enabled: true
    base_url: "${MOCK_BASE_URL}"
    api_key: "sk-llm-local"
    model: "llm-small"
    temperature: 0.1
    max_tokens: 64
    timeout_ms: 5000
    request_overrides:
      stream: true
      enable_thinking: false
"#,
        );
        initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
        preload_model_config().expect("model config should preload");

        let response =
            model_llm("system prompt", "user prompt", None).expect("LLM call should succeed");
        let request = handle.join().expect("mock provider should finish");

        assert_eq!(response.assistant, "mock assistant");
        assert_eq!(
            response.usage,
            Some(ModelUsage {
                input_tokens: Some(11),
                output_tokens: Some(4),
                total_tokens: Some(15),
            })
        );
        assert_eq!(request.path, "/v1/chat/completions");
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer sk-llm-local")
        );
        assert_eq!(request.body["model"], "llm-small");
        assert_eq!(request.body["stream"], false);
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][0]["content"], "system prompt");
        assert_eq!(request.body["messages"][1]["role"], "user");
        assert_eq!(request.body["messages"][1]["content"], "user prompt");
        assert_eq!(request.body["enable_thinking"], false);

        restore_default_model_callbacks();
        let _ = std::fs::remove_dir_all(root);
    }

    /// LuaSkills model callbacks should be registered only when the host model config enables the capability.
    /// 只有宿主模型配置启用对应能力时，才应注册 LuaSkills 模型回调。
    #[test]
    fn install_callbacks_registers_enabled_capabilities_for_lua_status() {
        let _guard = callback_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = unique_test_dir("enabled-callbacks");
        let config_path = root.join("configs").join("model_config.yaml");
        std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
            .expect("config dir should be created");
        std::fs::write(
            &config_path,
            r#"
openai_compatible:
  enabled: true
  embedding:
    enabled: true
    base_url: "http://127.0.0.1:9/v1"
    api_key: "sk-test"
    model: "embed-small"
  llm:
    enabled: false
    model: ""
"#,
        )
        .expect("model config should be written");

        initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
        preload_model_config().expect("model config should preload");
        install_luaskills_model_callbacks();

        let engine = make_lua_engine();
        let result = engine
            .run_lua(
                r#"
local status = vulcan.models.status()
return {
  status_ok = status.ok,
  embed = status.capabilities.embed,
  llm = status.capabilities.llm,
  has_embed = vulcan.models.has("embed"),
  has_llm = vulcan.models.has("llm"),
}
"#,
                &json!({}),
                None,
            )
            .expect("Lua model status should run");

        assert_eq!(result["status_ok"], true);
        assert_eq!(result["embed"], true);
        assert_eq!(result["llm"], false);
        assert_eq!(result["has_embed"], true);
        assert_eq!(result["has_llm"], false);

        restore_default_model_callbacks();
        let _ = std::fs::remove_dir_all(root);
    }
}
