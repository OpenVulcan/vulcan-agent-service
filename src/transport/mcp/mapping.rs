use serde_json::{Value, json};

use crate::host_core::model::{
    RuntimeInputSchema, RuntimeTextContent, RuntimeToolAnnotations, RuntimeToolCallResult,
    RuntimeToolDescriptor,
};
use crate::support::{RuntimeClientInfo, RuntimeRequestContext};
use crate::transport::mcp::protocol::{
    ClientInfo, InputSchema, RequestContext, TextContent, Tool, ToolAnnotations, ToolCallResult,
};

/// Convert an MCP request context into the host runtime request context.
/// 将 MCP 请求上下文转换为宿主运行时请求上下文。
pub(crate) fn runtime_context_from_mcp(context: &RequestContext) -> RuntimeRequestContext {
    RuntimeRequestContext {
        transport: context.transport.clone(),
        session_id: context.session_id.clone(),
        protocol_version: context.protocol_version.clone(),
        client_info: context
            .client_info
            .as_ref()
            .map(runtime_client_info_from_mcp),
        client_match_name_override: context.client_match_name_override.clone(),
        exact_client_name: context.exact_client_name.clone(),
        disable_client_match_overrides: context.disable_client_match_overrides,
        client_capabilities: context.client_capabilities.clone(),
    }
}

/// Convert runtime tool descriptors into an MCP tools/list result value.
/// 将运行时工具描述转换为 MCP tools/list 结果值。
pub(super) fn mcp_tools_list_value_from_runtime(
    tools: Vec<RuntimeToolDescriptor>,
) -> Result<Value, (i64, String)> {
    let tools = tools
        .into_iter()
        .map(mcp_tool_from_runtime)
        .collect::<Vec<_>>();
    serde_json::to_value(json!({ "tools": tools }))
        .map_err(|error| (-32603, format!("Tool list serialization error: {}", error)))
}

/// Convert a runtime tool-call result into an MCP JSON result value.
/// 将运行时工具调用结果转换为 MCP JSON 结果值。
pub(super) fn mcp_tool_call_result_value_from_runtime(
    result: RuntimeToolCallResult,
) -> Result<Value, (i64, String)> {
    serde_json::to_value(mcp_tool_call_result_from_runtime(result)).map_err(|error| {
        (
            -32603,
            format!("Tool result serialization error: {}", error),
        )
    })
}

/// Convert one MCP client identity into a runtime-neutral client identity.
/// 将单个 MCP 客户端身份转换为运行时中立客户端身份。
fn runtime_client_info_from_mcp(client: &ClientInfo) -> RuntimeClientInfo {
    RuntimeClientInfo {
        name: client.name.clone(),
        version: client.version.clone(),
    }
}

/// Convert one runtime tool descriptor into an MCP protocol tool descriptor.
/// 将单个运行时工具描述转换为 MCP 协议工具描述。
fn mcp_tool_from_runtime(tool: RuntimeToolDescriptor) -> Tool {
    Tool {
        name: tool.name,
        description: tool.description,
        input_schema: mcp_input_schema_from_runtime(tool.input_schema),
        annotations: tool.annotations.map(mcp_tool_annotations_from_runtime),
    }
}

/// Convert one runtime input schema into an MCP input schema.
/// 将单个运行时输入 schema 转换为 MCP 输入 schema。
fn mcp_input_schema_from_runtime(schema: RuntimeInputSchema) -> InputSchema {
    InputSchema {
        schema_type: schema.schema_type,
        properties: schema.properties,
        required: schema.required,
    }
}

/// Convert runtime tool annotations into MCP tool annotations.
/// 将运行时工具注解转换为 MCP 工具注解。
fn mcp_tool_annotations_from_runtime(annotations: RuntimeToolAnnotations) -> ToolAnnotations {
    ToolAnnotations {
        read_only_hint: annotations.read_only_hint,
        destructive_hint: annotations.destructive_hint,
        user_confirmation_required: annotations.user_confirmation_required,
        idempotent_hint: annotations.idempotent_hint,
    }
}

/// Convert one runtime tool-call result into the MCP protocol result DTO.
/// 将单个运行时工具调用结果转换为 MCP 协议结果 DTO。
fn mcp_tool_call_result_from_runtime(result: RuntimeToolCallResult) -> ToolCallResult {
    ToolCallResult {
        content: result
            .content
            .into_iter()
            .map(mcp_text_content_from_runtime)
            .collect(),
        is_error: result.is_error,
    }
}

/// Convert one runtime text content block into an MCP text content block.
/// 将单个运行时文本内容块转换为 MCP 文本内容块。
fn mcp_text_content_from_runtime(content: RuntimeTextContent) -> TextContent {
    TextContent::text(&content.text)
}
