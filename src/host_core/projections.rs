use std::collections::{BTreeMap, HashMap};

use crate::host_core::model::RuntimeToolDescriptor;
use crate::host_core::state::LuaSkillToolDescriptor;
use luaskills::{RuntimeEntryDescriptor, RuntimeHelpDetail, RuntimeSkillHelpDescriptor};

/// Merge host and LuaSkills tool registries into stable runtime tool descriptors.
/// 将宿主工具与 LuaSkills 工具注册表合并为稳定的运行时工具描述。
pub(super) fn build_runtime_tools(
    host_tools: &HashMap<String, RuntimeToolDescriptor>,
    skill_tools: &HashMap<String, RuntimeToolDescriptor>,
) -> Vec<RuntimeToolDescriptor> {
    let mut merged = BTreeMap::new();
    for (name, tool) in skill_tools {
        merged.insert(name.clone(), tool.clone());
    }
    for (name, tool) in host_tools {
        merged.insert(name.clone(), tool.clone());
    }
    merged.into_values().collect()
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
    tool: &RuntimeToolDescriptor,
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
