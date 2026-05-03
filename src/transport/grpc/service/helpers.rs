use serde_json::{Value, json};
use tonic::Status;

use crate::host_core::{
    LuaSkillPackageDescriptor, LuaSkillToolDescriptor as RuntimeLuaSkillToolDescriptor,
    RuntimeToolCallResult,
};
use crate::transport::mcp::protocol::{ClientInfo, RequestContext};

use super::pb::{
    LuaSkillCallToolResponse, LuaSkillClientContext, LuaSkillDescriptor, LuaSkillTextResponse,
    LuaSkillToolDescriptor, McpCallRequest,
};

/// Build the request context used by the legacy generic gRPC `Call` method.
/// 构造旧版通用 gRPC `Call` 方法使用的请求上下文。
pub(super) fn build_mcp_call_request_context(req: &McpCallRequest) -> RequestContext {
    let client_name = optional_string(req.client_name.clone());
    let client_version = optional_string(req.client_version.clone()).unwrap_or_default();
    RequestContext {
        transport: Some("grpc_unary".to_string()),
        session_id: optional_string(req.session_id.clone()),
        client_info: client_name.as_ref().map(|name| ClientInfo {
            name: name.clone(),
            version: client_version,
        }),
        client_match_name_override: None,
        exact_client_name: client_name,
        disable_client_match_overrides: true,
        ..RequestContext::default()
    }
}

/// Require a LuaSkills gRPC client context with a non-empty exact client name.
/// 要求 LuaSkills gRPC 客户端上下文存在且包含非空精确客户端名称。
pub(super) fn require_luaskill_context(
    context: Option<&LuaSkillClientContext>,
) -> Result<LuaSkillClientContext, Status> {
    let context = context
        .cloned()
        .ok_or_else(|| Status::invalid_argument("LuaSkills gRPC request requires context"))?;
    let client_name = optional_string(context.client_name.clone()).ok_or_else(|| {
        Status::invalid_argument("LuaSkills gRPC request requires context.client_name")
    })?;
    Ok(LuaSkillClientContext {
        client_name,
        client_version: optional_string(context.client_version).unwrap_or_default(),
        request_id: optional_string(context.request_id).unwrap_or_default(),
    })
}

/// Parse one JSON argument string for a dynamic LuaSkills tool call.
/// 解析动态 LuaSkills 工具调用的一段 JSON 参数字符串。
pub(super) fn parse_json_arguments(arguments_json: &str) -> Result<Value, Status> {
    if arguments_json.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(arguments_json).map_err(|error| {
        Status::invalid_argument(format!("Invalid arguments_json payload: {}", error))
    })
}

/// Convert an optional protobuf string field represented as a plain string into an owned option.
/// 将以普通字符串表示的可选 protobuf 字段转换为自有 Option。
pub(super) fn optional_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Convert a string option into a borrowed option for internal server calls.
/// 将字符串选项转换为内部服务调用使用的借用选项。
pub(super) fn optional_str(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() { None } else { Some(value) }
}

/// Convert one loaded LuaSkill package descriptor into the protobuf response type.
/// 将一个已加载 LuaSkill 包描述转换为 protobuf 响应类型。
pub(super) fn lua_skill_package_to_pb(
    descriptor: &LuaSkillPackageDescriptor,
) -> LuaSkillDescriptor {
    LuaSkillDescriptor {
        skill_id: descriptor.skill_id.clone(),
        root_name: descriptor.root_name.clone(),
        skill_dir: descriptor.skill_dir.clone(),
        tool_names: descriptor.tool_names.clone(),
    }
}

/// Convert one dynamic LuaSkill tool descriptor into the protobuf response type.
/// 将一个动态 LuaSkill 工具描述转换为 protobuf 响应类型。
pub(super) fn lua_skill_tool_to_pb(
    descriptor: &RuntimeLuaSkillToolDescriptor,
) -> LuaSkillToolDescriptor {
    LuaSkillToolDescriptor {
        name: descriptor.tool.name.clone(),
        description: descriptor.tool.description.clone().unwrap_or_default(),
        input_schema_json: serde_json::to_string(&descriptor.tool.input_schema)
            .unwrap_or_else(|_| "{}".to_string()),
        annotations_json: serde_json::to_string(&descriptor.tool.annotations)
            .unwrap_or_else(|_| "null".to_string()),
        skill_id: descriptor.skill_id.clone(),
        entry_name: descriptor.entry_name.clone(),
        root_name: descriptor.root_name.clone(),
        skill_dir: descriptor.skill_dir.clone(),
    }
}

/// Convert one tool-call result into the dynamic gRPC CallTool response.
/// 将一个工具调用结果转换为动态 gRPC CallTool 响应。
pub(super) fn tool_call_result_to_call_response(
    result: &RuntimeToolCallResult,
) -> LuaSkillCallToolResponse {
    let text = tool_call_result_text(result);
    let is_error = result.is_error.unwrap_or(false);
    LuaSkillCallToolResponse {
        result_json: serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string()),
        text: text.clone(),
        is_error,
        message: if is_error { text } else { String::new() },
    }
}

/// Convert one tool-call result into a stable text response.
/// 将一个工具调用结果转换为稳定文本响应。
pub(super) fn tool_call_result_to_text_response(
    result: &RuntimeToolCallResult,
) -> LuaSkillTextResponse {
    let text = tool_call_result_text(result);
    let is_error = result.is_error.unwrap_or(false);
    LuaSkillTextResponse {
        text: text.clone(),
        is_error,
        message: if is_error { text } else { String::new() },
    }
}

/// Convert a successful string payload into a stable text response.
/// 将成功字符串载荷转换为稳定文本响应。
pub(super) fn text_response(text: String) -> LuaSkillTextResponse {
    LuaSkillTextResponse {
        text,
        is_error: false,
        message: String::new(),
    }
}

/// Join the text blocks inside one MCP-compatible tool result.
/// 拼接一个 MCP 兼容工具结果内的文本块。
fn tool_call_result_text(result: &RuntimeToolCallResult) -> String {
    result
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Map an internal JSON-RPC style error tuple to an idiomatic gRPC status.
/// 将内部 JSON-RPC 风格错误元组映射为惯用 gRPC 状态。
pub(super) fn mcp_error_to_status(error: (i64, String)) -> Status {
    match error.0 {
        -32601 => Status::not_found(error.1),
        -32602 => Status::invalid_argument(error.1),
        -32603 => Status::internal(error.1),
        _ => Status::unknown(error.1),
    }
}
