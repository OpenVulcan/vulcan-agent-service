use crate::config::model_config::{
    current_effective_model_config, effective_embedding_api_key, effective_embedding_base_url,
    effective_llm_api_key, effective_llm_base_url,
};
use luaskills::{
    RuntimeModelEmbedCallback, RuntimeModelEmbedRequest, RuntimeModelLlmCallback,
    RuntimeModelLlmRequest, set_model_embed_callback, set_model_llm_callback,
};
use std::sync::Arc;

mod openai;
mod runtime_bridge;
mod types;

#[cfg(test)]
use openai::provider_error_from_http_status;
use openai::{
    build_blocking_client, build_embedding_request_body, build_llm_request_body,
    normalized_optional_text, openai_endpoint_url, parse_embedding_response, parse_llm_response,
    post_json_value, require_non_empty_argument, require_openai_provider_for_embedding,
    require_openai_provider_for_llm,
};
use runtime_bridge::{
    model_invocation_context_from_runtime_caller, runtime_embed_response_from_model_response,
    runtime_llm_response_from_model_response, runtime_model_error_from_model_error,
};

pub use types::{
    ModelCapability, ModelEmbedResponse, ModelError, ModelErrorCode, ModelInvocationContext,
    ModelLlmResponse, ModelStatus, ModelUsage,
};

/// Default timeout in milliseconds for model provider calls when a capability does not override it.
/// 当单项能力未覆盖时，模型供应商调用使用的默认超时时间，单位为毫秒。
const DEFAULT_MODEL_TIMEOUT_MS: u64 = 60_000;
/// Stable label for the only provider family currently implemented by the host.
/// 当前宿主已实现的唯一供应商族稳定标签。
const OPENAI_COMPATIBLE_PROVIDER_LABEL: &str = "openai_compatible";

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

#[cfg(test)]
mod tests;
