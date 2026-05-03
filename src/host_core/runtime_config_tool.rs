use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use luaskills::{SkillConfigEntry, runtime_config_store::SkillConfigStore};

/// Supported actions for the host-owned unified luaskill-config tool.
/// 宿主自有统一 luaskill-config 工具支持的动作集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RuntimeConfigAction {
    /// List config entries, optionally scoped to one skill namespace.
    /// 列出配置项，可选地限制到单个技能命名空间。
    List,
    /// Read one concrete config value by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 读取单个配置值。
    Get,
    /// Insert or replace one concrete config value by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 写入或替换单个配置值。
    Set,
    /// Delete one concrete config key by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 删除单个配置键。
    Delete,
}

impl RuntimeConfigAction {
    /// Render one stable action name used by logs and user-facing tool output.
    /// 渲染供日志与面向用户的工具输出使用的稳定动作名称。
    fn as_str(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Get => "get",
            Self::Set => "set",
            Self::Delete => "delete",
        }
    }
}

/// Parsed arguments for one host-owned unified luaskill-config tool call.
/// 一次宿主统一 luaskill-config 工具调用解析后的参数载荷。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct RuntimeConfigToolArguments {
    /// Action selector that chooses one of `list/get/set/delete`.
    /// 动作选择器，用于决定 `list/get/set/delete` 中的哪一种。
    pub(super) action: RuntimeConfigAction,
    /// Optional target skill namespace used by `list`, and required by `get/set/delete`.
    /// `list` 可选使用、`get/set/delete` 必填的目标技能命名空间。
    pub(super) skill_id: Option<String>,
    /// Optional config key used by `get/set/delete`.
    /// `get/set/delete` 使用的可选配置键。
    pub(super) key: Option<String>,
    /// Optional string config value used by `set`.
    /// `set` 使用的可选字符串配置值。
    pub(super) value: Option<String>,
}

/// Parse one luaskill-config argument payload into the strongly typed host-tool request model.
/// 把一份 luaskill-config 参数载荷解析为强类型宿主工具请求模型。
pub(super) fn parse_runtime_config_tool_arguments(
    args: &Value,
) -> Result<RuntimeConfigToolArguments, (i64, String)> {
    serde_json::from_value(args.clone()).map_err(|error| {
        (
            -32602,
            format!("Invalid luaskill-config arguments: {}", error),
        )
    })
}

/// Normalize one optional luaskill-config string field by trimming whitespace and dropping blanks.
/// 规范化一项可选 luaskill-config 字符串字段：去除首尾空白并丢弃空串。
fn normalize_optional_runtime_config_field(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Require one non-empty luaskill-config identifier field such as `skill_id` or `key`.
/// 要求一项非空的 luaskill-config 标识字段，例如 `skill_id` 或 `key`。
fn require_runtime_config_identifier_field(
    value: Option<&str>,
    field_name: &str,
    action: &RuntimeConfigAction,
) -> Result<String, (i64, String)> {
    normalize_optional_runtime_config_field(value).ok_or_else(|| {
        (
            -32602,
            format!(
                "luaskill-config action '{}' requires a non-empty parameter: {}",
                action.as_str(),
                field_name
            ),
        )
    })
}

/// Require one raw luaskill-config value field and preserve empty-string payloads for explicit writes.
/// 要求提供一项原始 luaskill-config 值字段，并保留空字符串这种显式写入载荷。
fn require_runtime_config_value_field(
    value: Option<&str>,
    action: &RuntimeConfigAction,
) -> Result<String, (i64, String)> {
    value.map(str::to_string).ok_or_else(|| {
        (
            -32602,
            format!(
                "luaskill-config action '{}' requires parameter: value",
                action.as_str()
            ),
        )
    })
}

/// Group flattened skill-config entries into one stable nested skill-to-key-value mapping.
/// 把扁平化 Skill 配置记录分组为稳定的“技能 -> 键值”嵌套映射。
fn group_runtime_config_entries(
    entries: &[SkillConfigEntry],
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut grouped = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(entry.skill_id.clone())
            .or_insert_with(BTreeMap::new)
            .insert(entry.key.clone(), entry.value.clone());
    }
    grouped
}

/// Render one config string value into a readable single-line literal for AI-oriented text output.
/// 把一个配置字符串渲染成适合面向 AI 文本输出的单行字面量。
fn render_runtime_config_value(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Render one grouped skill-config map as stable plain text without exposing the backing file path.
/// 把分组后的 Skill 配置映射渲染为稳定纯文本，且不暴露底层文件路径。
fn render_grouped_runtime_config_entries(
    grouped_entries: &BTreeMap<String, BTreeMap<String, String>>,
) -> String {
    let mut lines = Vec::new();
    for (index, (skill_id, values)) in grouped_entries.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(format!("skill_id: {}", skill_id));
        for (key, value) in values {
            lines.push(format!(
                "- {} = {}",
                key,
                render_runtime_config_value(value)
            ));
        }
    }
    lines.join("\n")
}

/// Execute one host-owned luaskill-config action against the standalone unified skill-config store.
/// 对独立统一 Skill 配置存储执行一次宿主自有 luaskill-config 动作。
pub(super) fn execute_runtime_config_tool(
    store: &SkillConfigStore,
    request: &RuntimeConfigToolArguments,
) -> Result<String, (i64, String)> {
    match &request.action {
        RuntimeConfigAction::List => {
            let requested_skill_id =
                normalize_optional_runtime_config_field(request.skill_id.as_deref());
            let entries = store
                .list_entries(requested_skill_id.as_deref())
                .map_err(|error| (-32603, format!("luaskill-config list failed: {}", error)))?;
            let grouped_entries = group_runtime_config_entries(&entries);
            if grouped_entries.is_empty() {
                return Ok(requested_skill_id
                    .map(|skill_id| format!("No configuration is set for skill `{}`.", skill_id))
                    .unwrap_or_else(|| "No luaskill configuration is currently set.".to_string()));
            }

            let header = requested_skill_id
                .as_deref()
                .map(|skill_id| format!("Configuration for skill `{}`:", skill_id))
                .unwrap_or_else(|| {
                    format!(
                        "Found {} luaskill configuration namespaces:",
                        grouped_entries.len()
                    )
                });
            Ok(format!(
                "{}\n\n{}",
                header,
                render_grouped_runtime_config_entries(&grouped_entries)
            ))
        }
        RuntimeConfigAction::Get => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let value = store
                .get_value(&skill_id, &key)
                .map_err(|error| (-32603, format!("luaskill-config get failed: {}", error)))?;
            Ok(match value {
                Some(value) => format!(
                    "Configuration found.\n\nskill_id: {}\n- {} = {}",
                    skill_id,
                    key,
                    render_runtime_config_value(&value)
                ),
                None => format!(
                    "Configuration key `{}` does not exist under skill `{}`.",
                    key, skill_id
                ),
            })
        }
        RuntimeConfigAction::Set => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let value =
                require_runtime_config_value_field(request.value.as_deref(), &request.action)?;
            store
                .set_value(&skill_id, &key, &value)
                .map_err(|error| (-32603, format!("luaskill-config set failed: {}", error)))?;
            Ok(format!(
                "Configuration updated.\n\nskill_id: {}\n- {} = {}",
                skill_id,
                key,
                render_runtime_config_value(&value)
            ))
        }
        RuntimeConfigAction::Delete => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let deleted = store
                .delete_value(&skill_id, &key)
                .map_err(|error| (-32603, format!("luaskill-config delete failed: {}", error)))?;
            Ok(if deleted {
                format!(
                    "Configuration key `{}` was deleted from skill `{}`.",
                    key, skill_id
                )
            } else {
                format!(
                    "Configuration key `{}` does not exist under skill `{}`, so nothing was deleted.",
                    key, skill_id
                )
            })
        }
    }
}
