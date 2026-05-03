use super::{
    ModelEmbedResponse, ModelError, ModelErrorCode, ModelInvocationContext, ModelLlmResponse,
    ModelUsage,
};
use luaskills::{
    RuntimeModelCaller, RuntimeModelEmbedResponse, RuntimeModelError, RuntimeModelErrorCode,
    RuntimeModelLlmResponse, RuntimeModelUsage,
};
/// Convert a LuaSkills runtime caller object into the host-side model invocation context.
/// 将 LuaSkills 运行时调用方对象转换为宿主侧模型调用上下文。
pub(super) fn model_invocation_context_from_runtime_caller(
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
pub(super) fn runtime_embed_response_from_model_response(
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
pub(super) fn runtime_llm_response_from_model_response(
    response: ModelLlmResponse,
) -> RuntimeModelLlmResponse {
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
pub(super) fn runtime_model_error_from_model_error(error: ModelError) -> RuntimeModelError {
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
