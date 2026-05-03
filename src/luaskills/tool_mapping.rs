use crate::transport::mcp::protocol::{Tool, ToolAnnotations};
use luaskills::RuntimeEntryDescriptor;
use serde_json::{Value, json};

/// Map one generic runtime entry descriptor into the MCP `Tool` object exposed to clients.
/// 把一份通用运行时入口描述映射为对外暴露给 MCP 客户端的 `Tool` 对象。
pub fn map_runtime_entry_to_mcp_tool(entry: &RuntimeEntryDescriptor) -> Tool {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for parameter in &entry.parameters {
        props.insert(
            parameter.name.clone(),
            json!({
                "type": parameter.param_type,
                "description": parameter.description
            }),
        );
        if parameter.required {
            required.push(parameter.name.clone());
        }
    }

    Tool::with_annotations(
        &entry.canonical_name,
        &entry.description,
        Value::Object(props),
        required,
        ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}
