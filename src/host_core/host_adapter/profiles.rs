use super::*;

/// Normalize arbitrary host text into a stable host kind.
/// 把任意宿主文本归一化为稳定宿主类型。
pub fn normalize_host_kind(value: Option<&str>) -> HostKind {
    let normalized = value.unwrap_or_default().trim().to_lowercase();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join("-");
    match normalized.as_str() {
        "claude" | "claude_code" | "claude-code" | "claudecode" => HostKind::ClaudeCode,
        "generic" | "generic-mcp" | "mcp" => HostKind::GenericMcp,
        "hermes" | "hermes-agent" | "hermes_agent" | "hermesagent" => HostKind::HermesAgent,
        "open-claw" | "open_claw" | "openclaw" => HostKind::Openclaw,
        "open-code" | "open_code" | "opencode" => HostKind::Opencode,
        "qwen" | "qwen-code" | "qwen_code" | "qwencode" => HostKind::QwenCode,
        "unknown" => HostKind::Unknown,
        _ => HostKind::Unknown,
    }
}

/// Return the capability profile for one host kind or alias.
/// 返回某个宿主类型或别名对应的能力画像。
pub fn get_host_capability_profile(host_kind: Option<&str>) -> HostCapabilityProfile {
    let kind = normalize_host_kind(host_kind);
    match kind {
        HostKind::Opencode => opencode_profile(),
        HostKind::Openclaw => openclaw_profile(),
        HostKind::ClaudeCode => claude_code_profile(),
        HostKind::QwenCode => qwen_code_profile(),
        HostKind::HermesAgent => hermes_agent_profile(),
        HostKind::GenericMcp => generic_mcp_profile(),
        HostKind::Unknown => unknown_profile(),
    }
}

/// Build one adapter descriptor from a host kind or alias.
/// 从宿主类型或别名构建一个适配器描述符。
pub fn get_host_adapter_descriptor(host_kind: Option<&str>) -> HostAdapterDescriptor {
    let profile = get_host_capability_profile(host_kind);
    let identity_mode = resolve_host_adapter_identity_mode(&profile);
    HostAdapterDescriptor {
        adapter_id: profile.host_kind,
        host_kind: profile.host_kind,
        display_name: format!("{} Adapter", profile.display_name),
        mode: host_adapter_mode(profile.host_kind),
        identity_mode,
        refresh_mode: resolve_tool_refresh_mode(&profile),
        supports_session_bound_memory_write: profile
            .capabilities
            .get(&CapabilityName::SessionIdAccess)
            .is_some_and(|support| support.level != CapabilityLevel::None),
        supports_workmem_fallback: profile
            .capabilities
            .get(&CapabilityName::WorkmemIdFallback)
            .is_some_and(|support| support.level != CapabilityLevel::None),
        notes: profile.notes.clone(),
        profile,
    }
}

/// Build one capability support object.
/// 构建一个能力支持对象。
fn capability(
    level: CapabilityLevel,
    reason: &str,
    evidence: CapabilityEvidence,
    requires_restart: Option<bool>,
) -> CapabilitySupport {
    CapabilitySupport {
        level,
        reason: reason.to_string(),
        evidence,
        requires_restart,
    }
}

/// Build a complete capability map while defaulting unspecified entries to unsupported.
/// 构建完整能力映射，并把未声明能力默认标记为不支持。
fn define_capabilities(
    overrides: Vec<(CapabilityName, CapabilitySupport)>,
) -> BTreeMap<CapabilityName, CapabilitySupport> {
    let mut capabilities = ALL_CAPABILITY_NAMES
        .iter()
        .copied()
        .map(|name| {
            (
                name,
                capability(
                    CapabilityLevel::None,
                    "No host-level evidence for this capability yet.",
                    CapabilityEvidence::Assumption,
                    None,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (name, support) in overrides {
        capabilities.insert(name, support);
    }
    capabilities
}

/// Build the OpenCode host capability profile.
/// 构建 OpenCode 宿主能力画像。
fn opencode_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Opencode,
        display_name: "OpenCode".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "The current repository has a working OpenCode native plugin.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode can consume MCP tools outside the native plugin path.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::None,
                    "OpenCode tool ids should be treated as restart-bound after registry changes.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Install, uninstall, and update may change tool ids; restart is the safe path.",
                    CapabilityEvidence::CurrentPlugin,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin contexts expose the active session id.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "WorkMem can use explicit or generated ids when native session identity is unavailable.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode message transform paths can inject precheck material.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths own memory post-processing after model/tool activity.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode message transform paths can scope prompt material to one turn.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Workflow prompts can be plugin-managed but are not yet a native durable registry.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths include before-tool and after-tool hooks.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths receive session lifecycle events.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode exposes compacting hook paths used by the current plugin.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Treat OpenCode as the highest-fidelity native adapter today.".to_string(),
            "Tool id changes should produce a host restart notice.".to_string(),
        ],
    }
}

/// Build the OpenClaw host capability profile.
/// 构建 OpenClaw 宿主能力画像。
fn openclaw_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Openclaw,
        display_name: "OpenClaw".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw has plugin and internal hook installation paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw exposes MCP-oriented integration paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Control-plane refresh points exist, but adapter reconciliation must prove live replacement.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Restart remains the conservative fallback when dynamic refresh cannot be proven.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw source exposes session keys and control-plane session operations.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "WorkMem can degrade to explicit ids when host session wiring is incomplete.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Limited,
                    "Message preprocessing hooks can support precheck-like behavior subject to exact timing.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Message and tool hook paths exist, but multi-stage turn merge requires validation.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Message preprocessing can inject prompt content but one-turn scope must be enforced.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Workflow prompts can map through hooks, but durable skill registry needs adapter work.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Tool events are visible, but blocking and result mutation semantics need checks.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Internal hooks include session-level events.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact-specific parity still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "OpenClaw is the best next native-adapter candidate.".to_string(),
            "Turn boundaries must be validated before marking full fidelity.".to_string(),
        ],
    }
}

/// Build the Claude Code host capability profile.
/// 构建 Claude Code 宿主能力画像。
fn claude_code_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::ClaudeCode,
        display_name: "Claude Code".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Limited,
                    "Claude Code has plugins, commands, hooks, agents, and skills but not OpenCode's adapter surface.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Claude Code plugins can include MCP server configuration.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "The documented reload flow exists, but live model prompt/tool refresh should not be assumed.",
                    CapabilityEvidence::OfficialDocs,
                    Some(true),
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Plugin changes should be treated as reload or restart bound unless live refresh is confirmed.",
                    CapabilityEvidence::OfficialDocs,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Limited,
                    "Documentation does not guarantee the same session id access required by VMM attribution.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Explicit WorkMem ids can support MCP-compatible tools when session binding is missing.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks may support prompt or tool gating, but one-turn precheck injection is not proven.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks may support post-action behavior, but multi-stage turn merge needs proof.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Skills and slash commands exist, but temporary per-turn injection should be degraded.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Full,
                    "Claude Code plugins document skills as plugin content.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks and MCP tools can support lifecycle interception in some form, but guarantees are unclear.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Lifecycle hooks may exist, but adapter-grade session identity needs validation.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact parity and per-turn memory timing are not guaranteed.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Prefer MCP-compatible mode first for Claude Code.".to_string(),
            "Do not assume per-turn scoped prompt injection until hook payloads are verified."
                .to_string(),
        ],
    }
}

/// Build the Qwen Code host capability profile.
/// 构建 Qwen Code 宿主能力画像。
fn qwen_code_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::QwenCode,
        display_name: "Qwen Code".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Limited,
                    "Qwen Code exposes hooks and settings rather than OpenCode-style plugin packaging.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code has MCP server configuration and tool registry paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Tool discovery exists, but live tool id replacement still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Tool id changes should fall back to host or session restart until dynamic replacement is proven.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code config paths expose session id data.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code can degrade to explicit WorkMem ids if session binding is unavailable.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include UserPromptSubmit and PreToolUse events.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Stop and PostToolUse hooks exist, but final turn merge semantics should be verified.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Prompt submit hooks can add context, but exact one-turn scope remains adapter-defined.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Hook-provided context can emulate workflow prompts, but durable skill registry is not proven.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include PreToolUse, PostToolUse, and PostToolUseFailure.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include SessionStart and SessionEnd.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include PreCompact coverage.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Qwen Code looks hook-rich, but package deployment differs from OpenCode.".to_string(),
            "Use hook evidence to shape the gRPC adapter contract before native packaging."
                .to_string(),
        ],
    }
}

/// Build the Hermes Agent host capability profile.
/// 构建 Hermes Agent 宿主能力画像。
fn hermes_agent_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::HermesAgent,
        display_name: "Hermes Agent".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "Hermes exposes shell hook and skill preprocessing surfaces suitable for a native adapter.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Hermes exposes MCP configuration and serving commands.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "ACP/MCP paths can refresh parts of the tool surface, while CLI configuration may need new sessions.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "New sessions remain the conservative fallback for MCP configuration changes.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "Hermes skill preprocessing includes session id substitution paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Hermes can degrade to explicit WorkMem ids when host session id is absent.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "Hermes shell hooks include pre-tool blocking and Claude-style decision conversion.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Post-tool hooks exist, but final answer memory merge must be proven.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Skill preprocessing can inject prompt material, but exact one-turn scope needs adapter ownership.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Full,
                    "Hermes skill command and preprocessing code can carry workflow-style prompt material.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hermes shell hooks include pre_tool_call and post_tool_call surfaces.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hermes ACP adapter includes explicit session flows.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact lifecycle parity still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Hermes should use the same vulcan-agent-service relay contract as TypeScript hosts."
                .to_string(),
            "Python runtime differences argue for gRPC adapter contracts.".to_string(),
        ],
    }
}

/// Build the generic MCP host capability profile.
/// 构建通用 MCP 宿主能力画像。
fn generic_mcp_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::GenericMcp,
        display_name: "Generic MCP Host".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "The host can call MCP tools but has no known native plugin contract.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Most MCP clients discover tools at connection or session start.",
                    CapabilityEvidence::Fallback,
                    Some(true),
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Without host session identity, tools can require explicit or generated WorkMem ids.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "This profile is the minimum viable compatibility layer.".to_string(),
            "It should not claim precheck, postaction, or host session memory attribution."
                .to_string(),
        ],
    }
}

/// Build the unknown host capability profile.
/// 构建未知宿主能力画像。
fn unknown_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Unknown,
        display_name: "Unknown Host".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Limited,
                    "Unknown hosts may support MCP, but this must be configured explicitly.",
                    CapabilityEvidence::Assumption,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Without host-specific refresh evidence, restart or reconnect is the safe guidance.",
                    CapabilityEvidence::Fallback,
                    Some(true),
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Explicit WorkMem ids are the safest cross-host fallback.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Use this profile until the adapter can positively identify the host.".to_string(),
            "Never assume precheck, postaction, or session id access for unknown hosts."
                .to_string(),
        ],
    }
}

/// Resolve the adapter mode for one host kind.
/// 为单个宿主类型解析适配器模式。
fn host_adapter_mode(host_kind: HostKind) -> HostAdapterMode {
    match host_kind {
        HostKind::Opencode => HostAdapterMode::NativePlugin,
        HostKind::GenericMcp => HostAdapterMode::McpCompatible,
        HostKind::Unknown => HostAdapterMode::Unknown,
        HostKind::Openclaw | HostKind::ClaudeCode | HostKind::QwenCode | HostKind::HermesAgent => {
            HostAdapterMode::Hybrid
        }
    }
}

/// Resolve the identity strategy for one host capability profile.
/// 为一个宿主能力画像解析身份策略。
fn resolve_host_adapter_identity_mode(profile: &HostCapabilityProfile) -> HostAdapterIdentityMode {
    match profile
        .capabilities
        .get(&CapabilityName::SessionIdAccess)
        .map(|support| support.level)
        .unwrap_or(CapabilityLevel::None)
    {
        CapabilityLevel::Full => HostAdapterIdentityMode::NativeSession,
        CapabilityLevel::Limited => HostAdapterIdentityMode::SessionOrWorkmem,
        CapabilityLevel::None => HostAdapterIdentityMode::WorkmemOnly,
    }
}
