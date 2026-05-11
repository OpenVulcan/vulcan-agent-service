use super::*;

/// Build all stable VMM binding/admin tool descriptors used by hosts without a native TUI.
/// 构建面向无原生 TUI 宿主的稳定 VMM 绑定与管理工具描述。
pub fn vmm_binding_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![
        vulcan_bind_descriptor(),
        vmm_get_bindings_descriptor(),
        vmm_list_users_descriptor(),
        vmm_bind_default_user_descriptor(),
        vmm_list_projects_descriptor(),
        vmm_bind_default_project_descriptor(),
        vmm_bind_agent_project_descriptor(),
        vmm_clear_agent_project_descriptor(),
    ]
}
fn vulcan_bind_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action", "resource"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["inspect", "list", "bind", "clear"],
                "description": "Binding operation. inspect returns the current effective binding state, list returns durable VMM identities, bind persists one host binding target, and clear removes one per-agent project override."
            },
            "resource": {
                "type": "string",
                "enum": ["bindings", "user", "project"],
                "description": "Binding resource. Use bindings with inspect, user or project with list/bind, and project with clear."
            },
            "scope": {
                "type": "string",
                "enum": ["global", "agent"],
                "description": "Binding scope. global updates the shared default host binding, while agent updates or clears one main-agent project override."
            },
            "ref": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric user_id/project_id, durable user name, or canonical Team/Space/Project path depending on the selected resource."
            },
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id. Omit to reuse the current trusted main-agent context when the host provides one."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may ask VMM to create a missing durable user name or canonical Team/Space/Project path while binding."
            }
        }
    });
    let description = "Inspect, list, bind, or clear host-level VMM user/project bindings through one compact management surface. Use this when the host does not have an OpenCode-style TUI and you still need to choose a shared default user_id/project_id, inspect the active binding state, or assign one main agent to a dedicated project.\n\nInput parameters:\n- action: inspect | list | bind | clear.\n- resource: bindings | user | project.\n- scope: global | agent. Use global for shared defaults and agent for one main-agent project override.\n- ref: Existing numeric user_id/project_id, durable user name, or canonical Team/Space/Project path depending on resource.\n- agentId: Optional main-agent id for inspect, bind(scope=agent), or clear.\n- createIfMissing: Optional boolean that only applies to bind and only when the selected ref can be created safely.";
    build_binding_descriptor(
        "vulcan_bind",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_CONSOLIDATED,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the local binding-inspection descriptor used by hosts without an OpenCode-style TUI.
/// 构建面向无 OpenCode 风格 TUI 宿主的本地绑定查看工具描述。
fn vmm_get_bindings_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id to inspect. Omit to inspect the current trusted agent when one is available."
            }
        }
    });
    let description = "Inspect the effective VMM user/project bindings used by the current host adapter. Use this when you need to confirm the shared default user_id/project_id, inspect whether one main agent has a dedicated project override, or debug which binding source currently wins. This is a host-level binding inspection tool, not a memory recall tool.\n\nInput parameters:\n- agentId: Optional host main-agent id to inspect. Omit to inspect the current trusted agent when one is available.";
    build_binding_descriptor(
        "vulcan_vmm_get_bindings",
        description,
        schema,
        "local",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the remote durable-user listing descriptor used by no-TUI hosts.
/// 构建供无 TUI 宿主使用的远程长期用户列表工具描述。
fn vmm_list_users_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    });
    let description = "List durable VMM users so the host can bind one real numeric user_id instead of guessing identity. Use this before choosing or switching the shared default user binding.";
    build_binding_descriptor(
        "vulcan_vmm_list_users",
        description,
        schema,
        "remote",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid default-user binding descriptor used by hosts that must persist bindings locally.
/// 构建供需要本地持久化绑定的宿主使用的混合型默认用户绑定工具描述。
fn vmm_bind_default_user_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["userRef"],
        "properties": {
            "userRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric user id or durable user name. When createIfMissing=true and the name does not exist, VMM will create it."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the user when userRef is a missing durable user name."
            }
        }
    });
    let description = "Resolve or create one durable VMM user, then persist its real numeric user_id as the host's shared default user binding. Use this when the host does not provide an OpenCode-style TUI and you need to manage default user selection through tools instead.\n\nInput parameters:\n- userRef: Existing numeric user id or durable user name.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing durable user name.";
    build_binding_descriptor(
        "vulcan_vmm_bind_default_user",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the remote project-listing descriptor used by no-TUI hosts.
/// 构建供无 TUI 宿主使用的远程项目列表工具描述。
fn vmm_list_projects_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    });
    let description = "List durable VMM Team/Space/Project entries so the host can bind one real numeric project_id or canonical display path before enabling memory workflows.";
    build_binding_descriptor(
        "vulcan_vmm_list_projects",
        description,
        schema,
        "remote",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid default-project binding descriptor used by hosts that persist project bindings locally.
/// 构建供本地持久化项目绑定的宿主使用的混合型默认项目绑定工具描述。
fn vmm_bind_default_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["projectRef"],
        "properties": {
            "projectRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric project id or canonical Team/Space/Project path. Creation requires a canonical Team/Space/Project path."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the project when projectRef is a missing canonical Team/Space/Project path."
            }
        }
    });
    let description = "Resolve or create one durable VMM project, then persist its real numeric project_id as the host's shared default project binding. Use this when a host without an OpenCode-style TUI needs to manage its shared default project through tools.\n\nInput parameters:\n- projectRef: Existing numeric project id or canonical Team/Space/Project path.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing canonical Team/Space/Project path.";
    build_binding_descriptor(
        "vulcan_vmm_bind_default_project",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid per-agent project binding descriptor used by hosts that support one project override per main agent.
/// 构建供支持主 agent 单独项目覆盖的宿主使用的混合型按 agent 项目绑定工具描述。
fn vmm_bind_agent_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["projectRef"],
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id to override. Omit to use the current trusted main agent id when one is available."
            },
            "projectRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric project id or canonical Team/Space/Project path. Creation requires a canonical Team/Space/Project path."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the project when projectRef is a missing canonical Team/Space/Project path."
            }
        }
    });
    let description = "Bind one host main agent to a dedicated VMM project_id while keeping the shared default project untouched. Use this when one agent needs an isolated project binding and all unconfigured agents should still fall back to the shared default project.\n\nInput parameters:\n- agentId: Optional main-agent id to override.\n- projectRef: Existing numeric project id or canonical Team/Space/Project path.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing canonical Team/Space/Project path.";
    build_binding_descriptor(
        "vulcan_vmm_bind_agent_project",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the local per-agent project-clear descriptor used by hosts that persist overrides locally.
/// 构建供本地持久化覆盖关系的宿主使用的本地按 agent 清除项目覆盖工具描述。
fn vmm_clear_agent_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id whose dedicated project override should be cleared. Omit to use the current trusted main agent id when one is available."
            }
        }
    });
    let description = "Clear one host main-agent project override so that agent falls back to the shared default project binding again. Use this when a dedicated per-agent project is no longer needed.";
    build_binding_descriptor(
        "vulcan_vmm_clear_agent_project",
        description,
        schema,
        "local",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}
