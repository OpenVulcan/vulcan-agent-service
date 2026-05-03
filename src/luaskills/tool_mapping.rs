use crate::host_core::model::{RuntimeToolAnnotations, RuntimeToolDescriptor};
use luaskills::RuntimeEntryDescriptor;
use serde_json::{Value, json};

/// Map one generic runtime entry descriptor into the runtime tool descriptor exposed by host core.
/// 把一份通用运行时入口描述映射为 host core 暴露的运行时工具描述。
pub fn map_runtime_entry_to_mcp_tool(entry: &RuntimeEntryDescriptor) -> RuntimeToolDescriptor {
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

    RuntimeToolDescriptor::with_annotations(
        &entry.canonical_name,
        &entry.description,
        Value::Object(props),
        required,
        RuntimeToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}
