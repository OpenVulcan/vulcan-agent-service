use super::*;

/// Build one runtime adapter binding for a host context.
/// 为一份宿主上下文构建一个运行时适配器绑定。
pub fn build_host_adapter_runtime(input: HostAdapterRuntimeInput) -> HostAdapterRuntime {
    let host_kind_text = input
        .adapter_host_kind
        .as_deref()
        .or(input.host_kind.as_deref());
    let host_kind = normalize_host_kind(host_kind_text);
    let descriptor = get_host_adapter_descriptor(Some(host_kind.as_str()));
    let context = build_host_runtime_context(HostRuntimeContextInput {
        host_kind: Some(host_kind.as_str().to_string()),
        session_id: input.session_id,
        workmem_id: input.workmem_id,
        turn_id: input.turn_id,
        workspace: input.workspace,
        user_message: input.user_message,
        conversation_id: input.conversation_id,
        root_session_id: input.root_session_id,
    });
    let adapter_reasons = resolve_adapter_identity_degradation_reasons(&descriptor, &context);
    let mut degraded_reasons = context.degraded_reasons.clone();
    degraded_reasons.extend(adapter_reasons);
    degraded_reasons = dedupe_strings(degraded_reasons);
    HostAdapterRuntime {
        descriptor,
        context,
        identity_ready: degraded_reasons
            .iter()
            .all(|reason| !reason.starts_with("adapter-requires-")),
        degraded_reasons,
    }
}

/// Normalize raw host context into the shared adapter relay contract.
/// 把原始宿主上下文归一化为共享的适配器中转契约。
pub fn build_host_runtime_context(input: HostRuntimeContextInput) -> HostRuntimeContext {
    let host_kind = normalize_host_kind(input.host_kind.as_deref());
    let capabilities = get_host_capability_profile(Some(host_kind.as_str()));
    let session_id = first_normalized_text([
        input.session_id.as_deref(),
        input.root_session_id.as_deref(),
        input.conversation_id.as_deref(),
    ]);
    let explicit_workmem_id = normalize_context_text(input.workmem_id.as_deref());
    let workspace = normalize_context_text(input.workspace.as_deref());
    let turn_id = normalize_context_text(input.turn_id.as_deref());
    let user_message = normalize_context_text(input.user_message.as_deref());
    let (workmem_id, workmem_source) = resolve_workmem_identity(
        host_kind,
        session_id.as_deref(),
        explicit_workmem_id,
        workspace.as_deref(),
    );
    let mut degraded_reasons = Vec::new();

    if session_id.is_none() {
        degraded_reasons.push(
            "missing-session-id: session-bound tools must use WorkMem fallback or stay disabled"
                .to_string(),
        );
    }
    if workmem_id.is_none() {
        degraded_reasons.push(
            "missing-workmem-id: WorkMem-compatible tools need an explicit id or workspace fallback"
                .to_string(),
        );
    }
    if capabilities
        .capabilities
        .get(&CapabilityName::SessionIdAccess)
        .is_some_and(|support| support.level == CapabilityLevel::None)
    {
        degraded_reasons.push(
            "host-has-no-session-id-access: native memory attribution is unavailable".to_string(),
        );
    }

    HostRuntimeContext {
        host_kind,
        capabilities,
        session_id: session_id.map(str::to_string),
        workmem_id,
        workmem_source,
        can_use_session_bound_tools: session_id.is_some(),
        can_use_workmem_bound_tools: workmem_source != WorkmemIdSource::Missing,
        turn_id,
        workspace,
        user_message,
        degraded_reasons,
    }
}

/// Resolve adapter-level identity degradation reasons.
/// 解析适配器层面的身份降级原因。
fn resolve_adapter_identity_degradation_reasons(
    descriptor: &HostAdapterDescriptor,
    context: &HostRuntimeContext,
) -> Vec<String> {
    match descriptor.identity_mode {
        HostAdapterIdentityMode::NativeSession if !context.can_use_session_bound_tools => vec![
            "adapter-requires-native-session: this host path needs a real session id for full memory attribution"
                .to_string(),
        ],
        HostAdapterIdentityMode::SessionOrWorkmem
            if !context.can_use_session_bound_tools && !context.can_use_workmem_bound_tools =>
        {
            vec![
                "adapter-requires-session-or-workmem: provide a session id, workmem id, or workspace fallback"
                    .to_string(),
            ]
        }
        HostAdapterIdentityMode::WorkmemOnly if !context.can_use_workmem_bound_tools => vec![
            "adapter-requires-workmem: this degraded host path needs an explicit or generated workmem id"
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

/// Normalize optional runtime context text into a trimmed optional string.
/// 把可选运行时上下文文本归一化为裁剪后的可选字符串。
fn normalize_context_text(value: Option<&str>) -> Option<String> {
    let normalized = value.unwrap_or_default().trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

/// Pick the first non-empty text value from a list of candidates.
/// 从候选值列表中选择第一条非空文本。
fn first_normalized_text<const N: usize>(values: [Option<&str>; N]) -> Option<&str> {
    values
        .into_iter()
        .find(|value| value.is_some_and(|text| !text.trim().is_empty()))
        .flatten()
        .map(str::trim)
}

/// Resolve the effective WorkMem identity and its source.
/// 解析有效 WorkMem 身份及其来源。
fn resolve_workmem_identity(
    host_kind: HostKind,
    session_id: Option<&str>,
    explicit_workmem_id: Option<String>,
    workspace: Option<&str>,
) -> (Option<String>, WorkmemIdSource) {
    if let Some(workmem_id) = explicit_workmem_id {
        return (Some(workmem_id), WorkmemIdSource::ProvidedWorkmemId);
    }
    if let Some(session_id) = session_id {
        return (Some(session_id.to_string()), WorkmemIdSource::SessionId);
    }
    if let Some(workspace) = workspace {
        return (
            Some(format!(
                "vwm_fallback_{}_{}",
                host_kind.as_str().replace('-', "_"),
                stable_text_hash(workspace)
            )),
            WorkmemIdSource::GeneratedFromWorkspace,
        );
    }
    (None, WorkmemIdSource::Missing)
}

/// Build a short deterministic hash for non-secret fallback identifiers.
/// 为非敏感 fallback 标识构建一条短确定性哈希。
fn stable_text_hash(text: &str) -> String {
    let mut hash = 0x811c9dc5_u32;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{hash:08x}")
}

/// Remove duplicate strings while preserving deterministic order.
/// 去除重复字符串并保持确定性顺序。
fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
