use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

use crate::host_core::state::LuaSkillToolDescriptor;
use crate::transport::mcp::protocol::{Prompt, Resource, ResourceTemplate, Tool};
use luaskills::{RuntimeEntryDescriptor, RuntimeHelpDetail, RuntimeSkillHelpDescriptor};

/// Build the tools/list response value from the host and LuaSkills tool registries.
/// 基于宿主工具注册表与 LuaSkills 工具注册表构建 tools/list 响应值。
pub(super) fn build_mcp_tools_value(
    host_tools: &HashMap<String, Tool>,
    skill_tools: &HashMap<String, Tool>,
) -> Value {
    let mut merged = BTreeMap::new();
    for (name, tool) in skill_tools {
        merged.insert(name.clone(), tool.clone());
    }
    for (name, tool) in host_tools {
        merged.insert(name.clone(), tool.clone());
    }
    let tools: Vec<Tool> = merged.into_values().collect();
    json!({ "tools": tools })
}

/// Build the resources/list response value from registered static resources.
/// 基于已注册静态资源构建 resources/list 响应值。
pub(super) fn build_mcp_resources_value(resources: &[Resource]) -> Value {
    json!({ "resources": resources })
}

/// Build the resources/read response value or the stable not-found error from raw MCP params.
/// 基于原始 MCP 参数构建 resources/read 响应值或稳定的未找到错误。
pub(super) fn build_mcp_resource_read_value(params: Option<Value>) -> Result<Value, (i64, String)> {
    let uri = params
        .and_then(|params| params.get("uri").cloned())
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| (-32602, "Missing required parameter: uri".to_string()))?;

    Err((-32602, format!("Resource not found: {}", uri)))
}

/// Build the resources/templates/list response value from registered resource templates.
/// 基于已注册资源模板构建 resources/templates/list 响应值。
pub(super) fn build_mcp_resource_templates_value(resource_templates: &[ResourceTemplate]) -> Value {
    json!({ "resourceTemplates": resource_templates })
}

/// Build the prompts/list response value from registered static prompts.
/// 基于已注册静态提示构建 prompts/list 响应值。
pub(super) fn build_mcp_prompts_value(prompts: &[Prompt]) -> Value {
    json!({ "prompts": prompts })
}

/// Build the prompts/get response value or stable not-found error from raw MCP params.
/// 基于原始 MCP 参数构建 prompts/get 响应值或稳定的未找到错误。
pub(super) fn build_mcp_prompt_get_value(params: Option<Value>) -> Result<Value, (i64, String)> {
    let params = params.unwrap_or_default();
    let name = params
        .get("name")
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| (-32602, "Missing required parameter: name".to_string()))?;

    Err((-32602, format!("Prompt not found: {}", name)))
}

/// Build the completion/complete response value and delegate prompt-specific values to the caller.
/// 构建 completion/complete 响应值，并把 prompt 专属候选值委托给调用方提供。
pub(super) fn build_mcp_completion_value<F>(
    params: Option<Value>,
    prompt_completions: F,
) -> Result<Value, (i64, String)>
where
    F: FnOnce(&str, &str) -> Result<Option<Vec<String>>, (i64, String)>,
{
    let params = params.unwrap_or_default();

    let ref_type = params
        .get("ref")
        .and_then(|reference| reference.get("type"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| (-32602, "Missing ref.type".to_string()))?;

    let argument_name = params
        .get("argument")
        .and_then(|argument| argument.get("name"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let argument_value = params
        .get("argument")
        .and_then(|argument| argument.get("value"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let ref_name = params
        .get("ref")
        .and_then(|reference| reference.get("name"))
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let prompt_completion_values = if ref_type == "ref/prompt" {
        prompt_completions(ref_name, argument_name)?
    } else {
        None
    };

    let values: Vec<String> = match (ref_type, argument_name) {
        ("ref/prompt", _) if prompt_completion_values.is_some() => prompt_completion_values
            .unwrap_or_default()
            .into_iter()
            .filter(|value| {
                if argument_value.is_empty() {
                    true
                } else {
                    value
                        .to_ascii_lowercase()
                        .contains(&argument_value.to_ascii_lowercase())
                }
            })
            .collect(),
        _ => vec![],
    };

    Ok(json!({
        "completion": {
            "values": values,
            "total": values.len() as u32,
            "hasMore": false
        }
    }))
}

/// Render one structured help list payload into user-facing Markdown.
/// 把一份结构化帮助列表载荷渲染成面向用户的 Markdown 文本。
pub(super) fn render_help_list_markdown(help_tree: &[RuntimeSkillHelpDescriptor]) -> String {
    if help_tree.is_empty() {
        return "# Vulcan Help List\n\nNo help trees are currently registered.".to_string();
    }

    let mut lines = vec!["# Vulcan Help List".to_string(), String::new()];
    for skill_help in help_tree {
        lines.push(format!("## `{}`", skill_help.skill_id));
        let main_description = skill_help.main.description.trim();
        if main_description.is_empty() {
            lines.push("- `main`: skill package description".to_string());
        } else {
            lines.push(format!(
                "- `main`: skill package description. {}",
                main_description
            ));
        }
        for flow in &skill_help.flows {
            if flow.description.trim().is_empty() {
                lines.push(format!("- `{}`", flow.flow_name));
            } else {
                lines.push(format!(
                    "- `{}`: {}",
                    flow.flow_name,
                    flow.description.trim()
                ));
            }
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Render one structured help detail payload into user-facing Markdown.
/// 把一份结构化帮助详情载荷渲染成面向用户的 Markdown 文本。
pub(super) fn render_help_detail_markdown(detail: &RuntimeHelpDetail) -> String {
    detail.content.clone()
}

/// Build one gRPC-facing LuaSkill tool descriptor from MCP tool schema and runtime entry metadata.
/// 基于 MCP 工具 schema 与运行时入口元数据构造一个面向 gRPC 的 LuaSkill 工具描述。
pub(super) fn build_luaskill_tool_descriptor(
    tool: &Tool,
    entry: &RuntimeEntryDescriptor,
) -> LuaSkillToolDescriptor {
    LuaSkillToolDescriptor {
        tool: tool.clone(),
        skill_id: entry.skill_id.clone(),
        entry_name: entry.local_name.clone(),
        root_name: entry.root_name.clone(),
        skill_dir: entry.skill_dir.clone(),
    }
}

/// Require one non-empty gRPC field and return its trimmed value.
/// 要求一个 gRPC 字段非空，并返回去除首尾空白后的值。
pub(super) fn require_non_empty_grpc_field(
    value: &str,
    field_name: &str,
) -> Result<String, (i64, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err((
            -32602,
            format!("gRPC LuaSkills request requires parameter: {}", field_name),
        ))
    } else {
        Ok(trimmed.to_string())
    }
}
