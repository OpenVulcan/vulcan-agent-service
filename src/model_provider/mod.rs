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
    ModelEmbedResponse, ModelError, ModelErrorCode, ModelInvocationContext, ModelLlmResponse,
    ModelStatus, ModelUsage,
};

/// Default timeout in milliseconds for model provider calls when a capability does not override it.
/// 当单项能力未覆盖时，模型供应商调用使用的默认超时时间，单位为毫秒。
const DEFAULT_MODEL_TIMEOUT_MS: u64 = 60_000;
/// Stable label for the only provider family currently implemented by the host.
/// 当前宿主已实现的唯一供应商族稳定标签。
const OPENAI_COMPATIBLE_PROVIDER_LABEL: &str = "openai_compatible";

/// Return the current model capability status without exposing secrets.
/// 返回当前模型能力状态，且不暴露密钥。
pub fn model_status() -> Result<ModelStatus, ModelError> {
    // Read the cached effective config explicitly so cache failures are visible to callers.
    // 显式读取缓存中的生效配置，确保缓存失败能被调用方看见。
    let effective = current_effective_model_config().map_err(model_config_runtime_error)?;
    // Inspect the configured OpenAI-compatible provider without cloning secrets into status output.
    // 检查已配置的 OpenAI-compatible 供应商，同时不把密钥克隆到状态输出中。
    let provider = &effective.config.openai_compatible;
    // Compute whether embedding has every required host-side setting.
    // 计算向量能力是否具备所有必需的宿主侧配置。
    let embed_ready = provider.enabled
        && provider.embedding.enabled
        && normalized_optional_text(provider.embedding.model.as_deref()).is_some()
        && effective_embedding_base_url(&effective).is_some()
        && effective_embedding_api_key(&effective).is_some();
    // Compute whether LLM has every required host-side setting.
    // 计算 LLM 能力是否具备所有必需的宿主侧配置。
    let llm_ready = provider.enabled
        && provider.llm.enabled
        && normalized_optional_text(provider.llm.model.as_deref()).is_some()
        && effective_llm_base_url(&effective).is_some()
        && effective_llm_api_key(&effective).is_some();
    Ok(ModelStatus {
        provider: OPENAI_COMPATIBLE_PROVIDER_LABEL.to_string(),
        provider_ready: provider.enabled && (embed_ready || llm_ready),
        embed: embed_ready,
        llm: llm_ready,
        source_path: effective
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
    })
}

/// Register or clear LuaSkills model callbacks based on the currently loaded host model configuration.
/// 根据当前已加载的宿主模型配置注册或清理 LuaSkills 模型回调。
pub fn install_luaskills_model_callbacks() -> Result<(), ModelError> {
    // Read the status once so embed and LLM callback decisions use one consistent snapshot.
    // 仅读取一次状态，确保向量与 LLM 回调决策使用同一份一致快照。
    let status = model_status()?;
    if status.embed {
        // Embed callback bridges LuaSkills requests into the host-owned embedding provider.
        // 向量回调用于把 LuaSkills 请求桥接到宿主拥有的向量供应商。
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

    if status.llm {
        // LLM callback bridges LuaSkills requests into the host-owned chat provider.
        // LLM 回调用于把 LuaSkills 请求桥接到宿主拥有的对话供应商。
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
    Ok(())
}

/// Execute a single-text embedding request through the host-owned provider configuration.
/// 通过宿主管理的供应商配置执行一次单文本向量请求。
pub fn model_embed(
    text: &str,
    context: Option<&ModelInvocationContext>,
) -> Result<ModelEmbedResponse, ModelError> {
    let _context = context;
    let input_text = require_non_empty_argument(text, "text")?;
    // Read the latest effective config and surface cache failures as model internal errors.
    // 读取最新生效配置，并把缓存失败暴露为模型内部错误。
    let effective = current_effective_model_config().map_err(model_config_runtime_error)?;
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
    // Read the latest effective config and surface cache failures as model internal errors.
    // 读取最新生效配置，并把缓存失败暴露为模型内部错误。
    let effective = current_effective_model_config().map_err(model_config_runtime_error)?;
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

/// Convert a model-config cache failure into the model provider error envelope.
/// 将模型配置缓存失败转换为模型供应商错误结构。
/// Parameters: `message` is the cache failure message produced by the config layer.
/// 参数：`message` 是配置层产生的缓存失败消息。
/// Returns a model error marked as an internal host-side failure.
/// 返回标记为宿主侧内部失败的模型错误。
fn model_config_runtime_error(message: String) -> ModelError {
    ModelError::new(ModelErrorCode::InternalError, message)
}

#[cfg(test)]
mod tests;
