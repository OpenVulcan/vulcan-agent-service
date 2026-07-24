use super::*;

/// Build all stable VMM binding/admin tool descriptors used by hosts without a native TUI.
/// 构建面向无原生 TUI 宿主的稳定 VMM 绑定与管理工具描述。
pub fn vmm_binding_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![vulcan_bind_descriptor()]
}

/// Build the compact host-facing binding tool descriptor used by hosts that want one parameterized binding surface.
/// 构建供宿主使用的单一参数化绑定工具描述。
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
