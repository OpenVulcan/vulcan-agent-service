use serde_json::{Map, Value, json};
use tonic::Status;

use crate::host_core::model::{RuntimeInputSchema, RuntimeToolAnnotations};
use crate::host_core::{
    LuaSkillPackageDescriptor, LuaSkillToolDescriptor as RuntimeLuaSkillToolDescriptor,
    RuntimeToolCallResult,
};
use crate::transport::mcp::protocol::{ClientInfo, RequestContext};

use super::pb::{
    LuaSkillCallToolResponse, LuaSkillClientContext, LuaSkillDescriptor, LuaSkillProjectionContext,
    LuaSkillTextResponse, LuaSkillToolDescriptor, McpCallRequest,
};

/// Normalized LuaSkills projection policy resolved from one gRPC request.
/// 从一条 gRPC 请求解析出的归一化 LuaSkills 投影策略。
#[derive(Debug, Clone, Default)]
pub(super) struct NormalizedLuaSkillProjectionContext {
    /// Whether the caller can hide LUASKILL_SID from AI-facing schemas and let the host inject it later.
    /// 调用方是否可以从 AI 可见 schema 中隐藏 LUASKILL_SID，并在后续由宿主自动注入。
    pub supports_managed_luaskill_sid: bool,
    /// Stable session identity provided by the host for managed LUASKILL_SID injection.
    /// 宿主提供、用于托管 LUASKILL_SID 自动注入的稳定会话身份。
    pub session_id: Option<String>,
}

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

/// Normalize an optional LuaSkills projection payload into one internal policy object.
/// 把可选 LuaSkills 投影载荷归一化为内部策略对象。
pub(super) fn normalize_luaskill_projection(
    projection: Option<&LuaSkillProjectionContext>,
) -> NormalizedLuaSkillProjectionContext {
    let Some(projection) = projection else {
        return NormalizedLuaSkillProjectionContext::default();
    };
    NormalizedLuaSkillProjectionContext {
        supports_managed_luaskill_sid: projection.supports_managed_luaskill_sid,
        session_id: optional_string(projection.session_id.clone()),
    }
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
        input_schema_json: runtime_input_schema_json(&descriptor.tool.input_schema),
        annotations_json: runtime_tool_annotations_json(&descriptor.tool.annotations),
        skill_id: descriptor.skill_id.clone(),
        entry_name: descriptor.entry_name.clone(),
        root_name: descriptor.root_name.clone(),
        skill_dir: descriptor.skill_dir.clone(),
    }
}

/// Render one runtime input schema as the JSON object string promised by the gRPC descriptor.
/// 将一个运行时输入 schema 渲染为 gRPC 描述符承诺的 JSON 对象字符串。
fn runtime_input_schema_json(input_schema: &RuntimeInputSchema) -> String {
    // Build from the verified schema fields so descriptor encoding cannot hide shape bugs behind "{}".
    // 基于已确认的 schema 字段构造，避免描述符编码把结构问题隐藏成 "{}"。
    let mut schema = Map::new();
    schema.insert(
        "type".to_string(),
        Value::String(input_schema.schema_type.clone()),
    );
    if let Some(properties) = &input_schema.properties {
        schema.insert("properties".to_string(), properties.clone());
    }
    if let Some(required) = &input_schema.required {
        schema.insert(
            "required".to_string(),
            Value::Array(
                required
                    .iter()
                    .map(|name| Value::String(name.clone()))
                    .collect(),
            ),
        );
    }
    Value::Object(schema).to_string()
}

/// Render optional runtime tool annotations as the JSON string promised by the gRPC descriptor.
/// 将可选运行时工具注解渲染为 gRPC 描述符承诺的 JSON 字符串。
fn runtime_tool_annotations_json(annotations: &Option<RuntimeToolAnnotations>) -> String {
    let Some(annotations) = annotations else {
        return Value::Null.to_string();
    };

    // Mirror RuntimeToolAnnotations serde field names without routing through a fallible fallback.
    // 直接镜像 RuntimeToolAnnotations 的 serde 字段名，而不经过带兜底的可失败路径。
    let mut rendered = Map::new();
    if let Some(read_only_hint) = annotations.read_only_hint {
        rendered.insert("readOnlyHint".to_string(), Value::Bool(read_only_hint));
    }
    if let Some(destructive_hint) = annotations.destructive_hint {
        rendered.insert("destructiveHint".to_string(), Value::Bool(destructive_hint));
    }
    if let Some(user_confirmation_required) = annotations.user_confirmation_required {
        rendered.insert(
            "userConfirmationRequired".to_string(),
            Value::Bool(user_confirmation_required),
        );
    }
    if let Some(idempotent_hint) = annotations.idempotent_hint {
        rendered.insert("idempotentHint".to_string(), Value::Bool(idempotent_hint));
    }
    Value::Object(rendered).to_string()
}

/// Convert one tool-call result into the dynamic gRPC CallTool response.
/// 将一个工具调用结果转换为动态 gRPC CallTool 响应。
pub(super) fn tool_call_result_to_call_response(
    result: &RuntimeToolCallResult,
) -> LuaSkillCallToolResponse {
    let text = tool_call_result_text(result);
    let is_error = result.is_error.unwrap_or(false);
    LuaSkillCallToolResponse {
        result_json: tool_call_result_json(result),
        text: text.clone(),
        is_error,
        message: if is_error { text } else { String::new() },
    }
}

/// Render one runtime tool-call result as the JSON object shape expected by gRPC callers.
/// 将一个运行时工具调用结果渲染为 gRPC 调用方期望的 JSON 对象形态。
fn tool_call_result_json(result: &RuntimeToolCallResult) -> String {
    let content = result
        .content
        .iter()
        .map(|item| json!({ "text": item.text.as_str() }))
        .collect::<Vec<_>>();

    if let Some(is_error) = result.is_error {
        json!({
            "content": content,
            "is_error": is_error,
        })
        .to_string()
    } else {
        json!({
            "content": content,
        })
        .to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_core::model::{RuntimeTextContent, RuntimeToolDescriptor};

    /// Render LuaSkill tool schema JSON from the exact runtime descriptor fields.
    /// 验证 LuaSkill 工具 schema JSON 来自准确的运行时描述字段。
    #[test]
    fn lua_skill_tool_to_pb_renders_schema_json_from_descriptor() {
        // Define a runtime schema with both optional schema branches present.
        // 定义一个同时包含两个可选 schema 分支的运行时 schema。
        let input_schema = RuntimeInputSchema {
            schema_type: "object".to_string(),
            properties: Some(serde_json::json!({
                "topic": {
                    "type": "string",
                    "description": "Topic to inspect"
                }
            })),
            required: Some(vec!["topic".to_string()]),
        };

        // Build the dynamic descriptor shape consumed by lua_skill_tool_to_pb.
        // 构造 lua_skill_tool_to_pb 消费的动态描述符形态。
        let descriptor = RuntimeLuaSkillToolDescriptor {
            tool: RuntimeToolDescriptor {
                name: "skill.inspect".to_string(),
                description: Some("Inspect a topic".to_string()),
                input_schema,
                annotations: None,
            },
            skill_id: "skill".to_string(),
            entry_name: "inspect".to_string(),
            root_name: "root".to_string(),
            skill_dir: "/tmp/skill".to_string(),
        };

        // Convert through the public descriptor mapper so proto metadata stays coupled.
        // 通过公开描述符映射函数转换，确保 proto 元信息保持耦合。
        let protobuf_descriptor = lua_skill_tool_to_pb(&descriptor);
        let schema: Value = serde_json::from_str(&protobuf_descriptor.input_schema_json)
            .expect("input_schema_json should be valid JSON");
        let annotations: Value = serde_json::from_str(&protobuf_descriptor.annotations_json)
            .expect("annotations_json should be valid JSON");

        assert_eq!(schema["type"].as_str(), Some("object"));
        assert_eq!(
            schema["properties"]["topic"]["description"].as_str(),
            Some("Topic to inspect")
        );
        assert_eq!(schema["required"][0].as_str(), Some("topic"));
        assert!(annotations.is_null());
    }

    /// Render LuaSkill tool annotations JSON from the exact runtime descriptor fields.
    /// 验证 LuaSkill 工具 annotations JSON 来自准确的运行时描述字段。
    #[test]
    fn lua_skill_tool_to_pb_renders_annotations_json_from_descriptor() {
        // Define annotations with a missing field so skip-if-none behavior is observable.
        // 定义包含缺失字段的注解，便于观察 skip-if-none 行为。
        let annotations = RuntimeToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: None,
            idempotent_hint: Some(true),
        };

        // Build a minimal dynamic descriptor with explicit annotations.
        // 构造带显式注解的最小动态描述符。
        let descriptor = RuntimeLuaSkillToolDescriptor {
            tool: RuntimeToolDescriptor {
                name: "skill.read".to_string(),
                description: None,
                input_schema: RuntimeInputSchema {
                    schema_type: "object".to_string(),
                    properties: None,
                    required: None,
                },
                annotations: Some(annotations),
            },
            skill_id: "skill".to_string(),
            entry_name: "read".to_string(),
            root_name: "root".to_string(),
            skill_dir: "/tmp/skill".to_string(),
        };

        // Convert through the public descriptor mapper to verify the emitted proto fields.
        // 通过公开描述符映射函数转换，验证输出的 proto 字段。
        let protobuf_descriptor = lua_skill_tool_to_pb(&descriptor);
        let schema: Value = serde_json::from_str(&protobuf_descriptor.input_schema_json)
            .expect("input_schema_json should be valid JSON");
        let annotations: Value = serde_json::from_str(&protobuf_descriptor.annotations_json)
            .expect("annotations_json should be valid JSON");

        assert_eq!(schema["type"].as_str(), Some("object"));
        assert_eq!(annotations["readOnlyHint"].as_bool(), Some(true));
        assert_eq!(annotations["destructiveHint"].as_bool(), Some(false));
        assert_eq!(annotations["idempotentHint"].as_bool(), Some(true));
        assert!(annotations.get("userConfirmationRequired").is_none());
    }

    /// Render result_json without an is_error field when the runtime result omitted it.
    /// 验证运行时结果省略 is_error 时 result_json 也不输出该字段。
    #[test]
    fn tool_call_result_json_omits_absent_error_flag() {
        // Build a normal tool result with the same transport-neutral model used by dispatch.
        // 使用分发路径相同的传输无关模型构造普通工具结果。
        let result = RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text("ok")],
            is_error: None,
        };

        // Convert through the public gRPC response helper so text and JSON stay coupled.
        // 通过公开 gRPC 响应辅助函数转换，确保文本与 JSON 保持一致。
        let response = tool_call_result_to_call_response(&result);
        let rendered: Value =
            serde_json::from_str(&response.result_json).expect("result_json should be valid JSON");

        assert_eq!(response.text, "ok");
        assert_eq!(rendered["content"][0]["text"].as_str(), Some("ok"));
        assert!(rendered.get("is_error").is_none());
    }

    /// Render result_json with an is_error field when the runtime result supplied it.
    /// 验证运行时结果提供 is_error 时 result_json 会输出该字段。
    #[test]
    fn tool_call_result_json_preserves_error_flag() {
        // Build an error tool result to cover the explicit is_error serialization path.
        // 构造错误工具结果以覆盖显式 is_error 序列化路径。
        let result = RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text("failed")],
            is_error: Some(true),
        };

        // Convert through the public gRPC response helper to verify message derivation too.
        // 通过公开 gRPC 响应辅助函数转换，同时验证 message 派生逻辑。
        let response = tool_call_result_to_call_response(&result);
        let rendered: Value =
            serde_json::from_str(&response.result_json).expect("result_json should be valid JSON");

        assert!(response.is_error);
        assert_eq!(response.message, "failed");
        assert_eq!(rendered["is_error"].as_bool(), Some(true));
    }
}
