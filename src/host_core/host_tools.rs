use serde_json::json;

use crate::host_core::model::{RuntimeToolAnnotations, RuntimeToolDescriptor};

/// Return whether one tool name belongs to the host-owned MCP tool surface.
/// 返回某个工具名是否属于宿主自有的 MCP 工具面。
pub fn is_host_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "vulcan-help-list"
            | "vulcan-help-detail"
            | "reload_vulcan_mcp_configs"
            | "runtime-config"
            | "skill-manager"
    )
}

/// Return whether one host-owned MCP tool requires a ready Lua engine to succeed.
/// 返回某个宿主自有 MCP 工具在执行时是否依赖已就绪的 Lua 引擎。
pub fn host_tool_requires_lua_engine(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "vulcan-help-list" | "vulcan-help-detail" | "runtime-config" | "skill-manager"
    )
}

/// Build the hot-reload host tool descriptor for runtime config files.
/// 构建运行时配置文件热重载宿主工具描述。
pub(super) fn reload_runtime_configs_tool() -> RuntimeToolDescriptor {
    RuntimeToolDescriptor::with_annotations(
        "reload_vulcan_mcp_configs",
        "Reload hot-reloadable Vulcan MCP runtime config files. This refreshes client_budgets.yaml, tool_configs.yaml, and model_config.yaml, but does not reload config.yaml or restart-bound transport settings. Use this only when the user explicitly asks to reload runtime configs; do not call it proactively during normal tool execution.",
        json!({}),
        vec![],
        RuntimeToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}

/// Build the host-owned LuaSkills package management tool descriptor.
/// 构建宿主自有的 LuaSkills 包管理工具描述。
pub(super) fn skill_manager_tool() -> RuntimeToolDescriptor {
    RuntimeToolDescriptor::with_annotations(
        "skill-manager",
        "Manage locally installed USER-layer LuaSkills packages through the host runtime. Supports `list`, `install`, `update`, and `uninstall`. Only call `install`, `update`, or `uninstall` when the user explicitly authorizes that exact operation; never perform skill lifecycle changes proactively. The target layer is fixed to USER and cannot be changed. Install derives the skill id from `source`; update and uninstall require `skill_id`. Uninstall retains SQLite and LanceDB data.",
        json!({
            "action": {
                "type": "string",
                "description": "Operation to perform. Supported values: `list`, `install`, `update`, `uninstall`.",
                "enum": ["list", "install", "update", "uninstall"]
            },
            "source": {
                "type": "string",
                "description": "Install source locator. Required for `install`; examples: `LuaSkills/vulcan-codekit` or `https://github.com/LuaSkills/vulcan-codekit`."
            },
            "source_type": {
                "type": "string",
                "description": "Optional install source type override. When omitted, GitHub repository locators are treated as `github`, and non-GitHub HTTP(S) URLs are treated as `url`.",
                "enum": ["github", "url"]
            },
            "skill_id": {
                "type": "string",
                "description": "Target skill id. Required for `update` and `uninstall`; not used for `install` because it is derived from the source."
            }
        }),
        vec!["action".to_string()],
        RuntimeToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            user_confirmation_required: Some(true),
            idempotent_hint: Some(false),
        },
    )
}

/// Build the host-managed unified LuaSkill configuration tool descriptor.
/// 构建宿主管理的统一 LuaSkill 配置工具描述。
pub(super) fn runtime_config_tool() -> RuntimeToolDescriptor {
    RuntimeToolDescriptor::with_annotations(
        "runtime-config",
        "Dispatch one already-authorized LuaSkills 0.5.5 package-configuration request. Supports `describe`, `validate`, `list`, `get`, `set`, `delete`, and `refresh`; typed values, batch writes, revisions, CAS, declaration validation, ROOT/system-store routing, and response errors follow the upstream stable JSON contract. This tool requires user confirmation because the single canonical entry can disclose values or mutate persisted configuration.",
        json!({
            "action": {
                "type": "string",
                "description": "Dispatcher action.",
                "enum": ["describe", "validate", "list", "get", "set", "delete", "refresh"]
            },
            "skill_id": {
                "type": "string",
                "description": "Effective package identifier when required by the selected action."
            },
            "key": {
                "type": "string",
                "description": "Single configuration key for `get`, `set`, or `delete`."
            },
            "value": {
                "type": ["string", "number", "boolean"],
                "description": "Typed scalar used by the single-key `set` form."
            },
            "values": {
                "type": "object",
                "description": "Typed key-to-scalar map used by the atomic batch `set` form.",
                "additionalProperties": {
                    "type": ["string", "number", "boolean"]
                }
            },
            "expected_revision": {
                "type": "string",
                "description": "Optional canonical decimal revision used for compare-and-swap writes."
            },
            "include_values": {
                "type": "boolean",
                "description": "Whether read responses may disclose raw persisted values.",
                "default": false
            },
            "mode": {
                "type": "string",
                "description": "Declaration discovery mode used by `describe`.",
                "enum": ["effective", "installed"],
                "default": "effective"
            },
            "root_name": {
                "type": "string",
                "description": "Optional physical root filter accepted only by installed `describe`."
            },
            "store_scope": {
                "type": "string",
                "description": "Optional persisted store scope accepted only by `refresh`.",
                "enum": ["skills", "system-skills"]
            }
        }),
        vec!["action".to_string()],
        RuntimeToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            user_confirmation_required: Some(true),
            idempotent_hint: Some(false),
        },
    )
}

/// Build the host-wrapped LuaSkills help-list tool descriptor.
/// 构建宿主包装的 LuaSkills help-list 工具描述。
pub(super) fn lua_help_list_tool() -> RuntimeToolDescriptor {
    RuntimeToolDescriptor::with_annotations(
        "vulcan-help-list",
        "List all registered strict LuaSkills help trees and their available flow descriptions. This MCP wrapper renders compact AI-facing Markdown from lib/system structured help data.",
        json!({}),
        vec![],
        RuntimeToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}

/// Build the host-wrapped LuaSkills help-detail tool descriptor.
/// 构建宿主包装的 LuaSkills help-detail 工具描述。
pub(super) fn lua_help_detail_tool() -> RuntimeToolDescriptor {
    RuntimeToolDescriptor::with_annotations(
        "vulcan-help-detail",
        "Read one strict LuaSkills help flow from lib/system help data and render it as Markdown for MCP clients. Use flow=`main` to read the skill package description node.",
        json!({
            "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit` or `vulcan-lua`."},
            "flow": {"type": "string", "description": "Help flow name. Use `main` for the skill package description node, or pass one declared workflow/topic name."}
        }),
        vec!["skill".to_string(), "flow".to_string()],
        RuntimeToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}
