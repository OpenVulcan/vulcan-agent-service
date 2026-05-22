use super::tool_refresh::tool_ids;
use super::*;
use serde_json::json;

/// Verify that common host aliases normalize to stable host kinds.
/// 验证常见宿主别名会归一化为稳定宿主类型。
#[test]
fn normalizes_common_host_aliases() {
    assert_eq!(normalize_host_kind(Some("open code")), HostKind::Opencode);
    assert_eq!(normalize_host_kind(Some("claude")), HostKind::ClaudeCode);
    assert_eq!(normalize_host_kind(Some("qwen_code")), HostKind::QwenCode);
    assert_eq!(normalize_host_kind(Some("mcp")), HostKind::GenericMcp);
    assert_eq!(normalize_host_kind(Some("missing")), HostKind::Unknown);
}

/// Verify that OpenCode adapter stays restart-bound for tool changes.
/// 验证 OpenCode 适配器对 tool 变化保持重启绑定。
#[test]
fn opencode_adapter_is_restart_bound() {
    let descriptor = get_host_adapter_descriptor(Some("opencode"));
    assert_eq!(descriptor.mode, HostAdapterMode::NativePlugin);
    assert_eq!(
        descriptor.identity_mode,
        HostAdapterIdentityMode::NativeSession
    );
    assert_eq!(descriptor.refresh_mode, ToolRefreshMode::RestartRequired);
    assert!(descriptor.supports_session_bound_memory_write);
    assert!(descriptor.supports_workmem_fallback);
}

/// Verify that generic MCP keeps only degraded WorkMem-oriented identity.
/// 验证 generic MCP 仅保留 WorkMem 导向的降级身份。
#[test]
fn generic_mcp_adapter_is_workmem_only() {
    let descriptor = get_host_adapter_descriptor(Some("generic-mcp"));
    assert_eq!(descriptor.mode, HostAdapterMode::McpCompatible);
    assert_eq!(
        descriptor.identity_mode,
        HostAdapterIdentityMode::WorkmemOnly
    );
    assert!(!descriptor.supports_session_bound_memory_write);
    assert!(descriptor.supports_workmem_fallback);
}

/// Verify that runtime context derives WorkMem identity from native session id.
/// 验证运行时上下文会从原生 session id 派生 WorkMem 身份。
#[test]
fn context_uses_session_as_workmem_identity() {
    let context = build_host_runtime_context(HostRuntimeContextInput {
        host_kind: Some("opencode".to_string()),
        session_id: Some(" session-a ".to_string()),
        ..Default::default()
    });
    assert_eq!(context.session_id.as_deref(), Some("session-a"));
    assert_eq!(context.workmem_id.as_deref(), Some("session-a"));
    assert_eq!(context.workmem_source, WorkmemIdSource::SessionId);
    assert!(context.can_use_session_bound_tools);
    assert!(context.degraded_reasons.is_empty());
}

/// Verify that generic MCP can use deterministic workspace fallback identity.
/// 验证 generic MCP 可以使用确定性的 workspace fallback 身份。
#[test]
fn generic_mcp_context_uses_workspace_fallback() {
    let runtime = build_host_adapter_runtime(HostAdapterRuntimeInput {
        host_kind: Some("generic-mcp".to_string()),
        workspace: Some("D:/projects/demo".to_string()),
        ..Default::default()
    });
    assert_eq!(
        runtime.context.workmem_source,
        WorkmemIdSource::GeneratedFromWorkspace
    );
    assert!(!runtime.context.can_use_session_bound_tools);
    assert!(runtime.context.can_use_workmem_bound_tools);
    assert!(runtime.identity_ready);
}

/// Verify that descriptor fingerprints ignore JSON object key ordering.
/// 验证描述符指纹会忽略 JSON 对象键顺序。
#[test]
fn tool_fingerprint_ignores_schema_key_order() {
    let left = ToolDescriptorSnapshot {
        id: "tool-a".to_string(),
        input_schema: Some(json!({
            "type": "object",
            "properties": {
                "b": { "type": "number" },
                "a": { "type": "string" }
            }
        })),
        name: None,
        description: None,
        version: None,
        source: None,
        workflow_count: None,
    };
    let right = ToolDescriptorSnapshot {
        id: "tool-a".to_string(),
        input_schema: Some(json!({
            "properties": {
                "a": { "type": "string" },
                "b": { "type": "number" }
            },
            "type": "object"
        })),
        name: None,
        description: None,
        version: None,
        source: None,
        workflow_count: None,
    };
    assert_eq!(
        build_tool_descriptor_fingerprint(&left).unwrap(),
        build_tool_descriptor_fingerprint(&right).unwrap()
    );
}

/// Verify that tool diff groups added, removed, and updated descriptors.
/// 验证 tool diff 会分组新增、删除和更新的描述符。
#[test]
fn tool_diff_groups_added_removed_and_updated() {
    let diff = diff_tool_registry_snapshots(
        &ToolRegistrySnapshot {
            tools: vec![
                tool("removed-tool", Some("old")),
                tool("updated-tool", Some("old")),
                tool("same-tool", Some("same")),
            ],
        },
        &ToolRegistrySnapshot {
            tools: vec![
                tool("added-tool", Some("new")),
                tool("updated-tool", Some("new")),
                tool("same-tool", Some("same")),
            ],
        },
        &ToolRegistryDiffOptions {
            refresh_mode: Some(ToolRefreshMode::RestartRequired),
            dynamic_tool_refresh_supported: None,
            host_restart_required: false,
        },
    )
    .unwrap();
    assert_eq!(tool_ids(&diff.added), vec!["added-tool"]);
    assert_eq!(tool_ids(&diff.removed), vec!["removed-tool"]);
    assert_eq!(tool_ids(&diff.updated), vec!["updated-tool"]);
    assert_eq!(
        diff.changed_tool_ids,
        vec!["added-tool", "removed-tool", "updated-tool"]
    );
    assert!(diff.restart_required);
}

/// Verify that refresh notice reports OpenCode registry changes as restart-required.
/// 验证刷新提示会把 OpenCode 注册表变化报告为需要重启。
#[test]
fn refresh_notice_marks_opencode_changes_restart_required() {
    let adapter = get_host_adapter_descriptor(Some("opencode"));
    let notice = build_tool_refresh_notice(
        ToolRegistrySnapshot {
            tools: vec![tool("tool-a", None)],
        },
        ToolRegistrySnapshot {
            tools: vec![tool("tool-a", None), tool("tool-b", None)],
        },
        &adapter,
    )
    .unwrap();
    assert!(notice.changed);
    assert_eq!(notice.severity, ToolRefreshNoticeSeverity::Warning);
    assert!(notice.restart_required);
    assert_eq!(notice.added_tool_ids, vec!["tool-b"]);
    assert!(notice.model_message.contains("Restart or reconnect"));
}

/// Build a compact test descriptor.
/// 构建一个紧凑测试描述符。
fn tool(id: &str, description: Option<&str>) -> ToolDescriptorSnapshot {
    ToolDescriptorSnapshot {
        id: id.to_string(),
        name: None,
        description: description.map(str::to_string),
        input_schema: None,
        version: None,
        source: None,
        workflow_count: None,
    }
}
