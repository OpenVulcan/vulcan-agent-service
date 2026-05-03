use super::types::OpenAiCapabilityProviderConfig;
use super::{ModelEmbedResponse, ModelError, ModelErrorCode, ModelLlmResponse, ModelUsage};
use crate::config::model_config::{
    EffectiveModelConfig, OpenAiCompatibleModelConfig, effective_embedding_api_key,
    effective_embedding_base_url, effective_llm_api_key, effective_llm_base_url,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;
/// Require that the OpenAI-compatible provider and embedding capability are both enabled and configured.
/// 要求 OpenAI-compatible 供应商与向量能力均已启用且配置完整。
pub(super) fn require_openai_provider_for_embedding(
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
pub(super) fn require_openai_provider_for_llm(
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
pub(super) fn require_non_empty_argument(
    value: &str,
    field_name: &str,
) -> Result<String, ModelError> {
    normalized_optional_text(Some(value)).ok_or_else(|| {
        ModelError::new(
            ModelErrorCode::InvalidArgument,
            format!("model argument `{}` must be a non-empty string", field_name),
        )
    })
}

/// Build a blocking HTTP client with a capability-specific timeout.
/// 构建带单项能力超时时间的阻塞 HTTP 客户端。
pub(super) fn build_blocking_client(timeout_ms: u64) -> Result<Client, ModelError> {
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
pub(super) fn build_embedding_request_body(
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
pub(super) fn build_llm_request_body(
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
pub(super) fn openai_endpoint_url(base_url: &str, endpoint: &str) -> Result<String, ModelError> {
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
pub(super) fn post_json_value(
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
pub(super) fn provider_error_from_http_status(
    status: StatusCode,
    body: &str,
    api_key: &str,
) -> ModelError {
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
pub(super) fn parse_embedding_response(value: &Value) -> Result<ModelEmbedResponse, ModelError> {
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
pub(super) fn parse_llm_response(value: &Value) -> Result<ModelLlmResponse, ModelError> {
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
pub(super) fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
