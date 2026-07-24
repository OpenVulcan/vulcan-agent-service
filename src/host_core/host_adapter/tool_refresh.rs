use super::*;

/// Resolve the safest tool refresh mode for one host profile.
/// 为某个宿主画像解析最安全的 tool 刷新模式。
pub fn resolve_tool_refresh_mode(profile: &HostCapabilityProfile) -> ToolRefreshMode {
    if profile
        .capabilities
        .get(&CapabilityName::DynamicToolRefresh)
        .is_some_and(|support| support.level == CapabilityLevel::Full)
    {
        return ToolRefreshMode::Dynamic;
    }
    if profile
        .capabilities
        .get(&CapabilityName::RestartRequiredToolRefresh)
        .is_some_and(|support| support.level != CapabilityLevel::None)
    {
        return ToolRefreshMode::RestartRequired;
    }
    ToolRefreshMode::Unsupported
}

/// Normalize one tool descriptor snapshot before diffing or fingerprinting.
/// 在差异比较或指纹计算前归一化单个 tool 描述符快照。
pub fn normalize_tool_descriptor_snapshot(
    tool: &ToolDescriptorSnapshot,
) -> Result<ToolDescriptorSnapshot, String> {
    let id = normalize_context_text(Some(&tool.id))
        .ok_or_else(|| "Tool descriptor id is required".to_string())?;
    Ok(ToolDescriptorSnapshot {
        id,
        name: normalize_context_text(tool.name.as_deref()),
        description: normalize_context_text(tool.description.as_deref()),
        input_schema: tool.input_schema.as_ref().map(stable_json_value),
        version: normalize_context_text(tool.version.as_deref()),
        source: tool.source,
        workflow_count: tool.workflow_count,
    })
}

/// Build a deterministic descriptor fingerprint for update detection.
/// 为更新检测构建一条确定性的描述符指纹。
pub fn build_tool_descriptor_fingerprint(tool: &ToolDescriptorSnapshot) -> Result<String, String> {
    let normalized = normalize_tool_descriptor_snapshot(tool)?;
    serde_json::to_string(&normalized).map_err(|error| error.to_string())
}

/// Diff two tool registry snapshots and derive restart guidance.
/// 对比两份 tool 注册表快照并推导重启提示。
pub fn diff_tool_registry_snapshots(
    previous: &ToolRegistrySnapshot,
    next: &ToolRegistrySnapshot,
    options: &ToolRegistryDiffOptions,
) -> Result<ToolRegistryDiff, String> {
    let previous_index = index_tool_snapshots(&previous.tools)?;
    let next_index = index_tool_snapshots(&next.tools)?;
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut updated = Vec::new();
    let mut unchanged = Vec::new();

    for (id, next_tool) in &next_index {
        if let Some(previous_tool) = previous_index.get(id) {
            if build_tool_descriptor_fingerprint(previous_tool)?
                == build_tool_descriptor_fingerprint(next_tool)?
            {
                unchanged.push(next_tool.clone());
            } else {
                updated.push(next_tool.clone());
            }
        } else {
            added.push(next_tool.clone());
        }
    }

    for (id, previous_tool) in &previous_index {
        if !next_index.contains_key(id) {
            removed.push(previous_tool.clone());
        }
    }

    sort_tools_by_id(&mut added);
    sort_tools_by_id(&mut removed);
    sort_tools_by_id(&mut updated);
    sort_tools_by_id(&mut unchanged);

    let mut changed_tool_ids = added
        .iter()
        .chain(removed.iter())
        .chain(updated.iter())
        .map(|tool| tool.id.clone())
        .collect::<Vec<_>>();
    changed_tool_ids.sort();
    let restart_required = !changed_tool_ids.is_empty() && should_require_restart(options);
    Ok(ToolRegistryDiff {
        summary: build_tool_registry_diff_summary(
            added.len(),
            removed.len(),
            updated.len(),
            restart_required,
        ),
        added,
        removed,
        updated,
        unchanged,
        changed_tool_ids,
        restart_required,
    })
}

/// Build one refresh notice directly from previous and next snapshots.
/// 直接从旧快照和新快照构建一条刷新提示。
pub fn build_tool_refresh_notice(
    previous: ToolRegistrySnapshot,
    next: ToolRegistrySnapshot,
    adapter: &HostAdapterDescriptor,
) -> Result<ToolRefreshNotice, String> {
    let diff = diff_tool_registry_snapshots(
        &previous,
        &next,
        &ToolRegistryDiffOptions {
            refresh_mode: Some(adapter.refresh_mode),
            dynamic_tool_refresh_supported: Some(adapter.refresh_mode == ToolRefreshMode::Dynamic),
            host_restart_required: adapter.refresh_mode == ToolRefreshMode::RestartRequired,
        },
    )?;
    Ok(build_tool_refresh_notice_from_diff(
        diff,
        &adapter.profile.display_name,
        adapter.refresh_mode,
    ))
}

/// Build one refresh notice from an already computed tool registry diff.
/// 从已经计算好的 tool 注册表差异构建一条刷新提示。
pub fn build_tool_refresh_notice_from_diff(
    diff: ToolRegistryDiff,
    host_display_name: &str,
    refresh_mode: ToolRefreshMode,
) -> ToolRefreshNotice {
    let changed = !diff.changed_tool_ids.is_empty();
    let severity = resolve_notice_severity(changed, refresh_mode);
    let restart_required = changed && refresh_mode == ToolRefreshMode::RestartRequired;
    ToolRefreshNotice {
        changed,
        host_display_name: host_display_name.to_string(),
        refresh_mode,
        severity,
        restart_required,
        added_tool_ids: tool_ids(&diff.added),
        removed_tool_ids: tool_ids(&diff.removed),
        updated_tool_ids: tool_ids(&diff.updated),
        changed_tool_ids: diff.changed_tool_ids.clone(),
        diff_summary: diff.summary,
        model_message: build_model_refresh_message(
            host_display_name,
            changed,
            refresh_mode,
            &diff.changed_tool_ids,
        ),
        user_message: build_user_refresh_message(
            host_display_name,
            changed,
            refresh_mode,
            &diff.changed_tool_ids,
        ),
    }
}

/// Build an id-indexed map and reject duplicate ids early.
/// 构建以 id 为键的映射，并尽早拒绝重复 id。
fn index_tool_snapshots(
    tools: &[ToolDescriptorSnapshot],
) -> Result<BTreeMap<String, ToolDescriptorSnapshot>, String> {
    let mut index = BTreeMap::new();
    for tool in tools {
        let normalized = normalize_tool_descriptor_snapshot(tool)?;
        if index.contains_key(&normalized.id) {
            return Err(format!("Duplicate tool descriptor id: {}", normalized.id));
        }
        index.insert(normalized.id.clone(), normalized);
    }
    Ok(index)
}

/// Sort tool descriptors by stable id in-place.
/// 按稳定 id 对 tool 描述符进行原地排序。
fn sort_tools_by_id(tools: &mut [ToolDescriptorSnapshot]) {
    tools.sort_by(|left, right| left.id.cmp(&right.id));
}

/// Decide whether a changed registry requires host restart.
/// 判断发生变化的注册表是否需要宿主重启。
fn should_require_restart(options: &ToolRegistryDiffOptions) -> bool {
    if options.host_restart_required {
        return true;
    }
    if let Some(refresh_mode) = options.refresh_mode {
        return refresh_mode == ToolRefreshMode::RestartRequired;
    }
    options.dynamic_tool_refresh_supported != Some(true)
}

/// Build a compact diff summary suitable for logs and diagnostics.
/// 构建适合日志与诊断的紧凑差异摘要。
fn build_tool_registry_diff_summary(
    added_count: usize,
    removed_count: usize,
    updated_count: usize,
    restart_required: bool,
) -> String {
    let changed_count = added_count + removed_count + updated_count;
    let restart_text = if restart_required {
        "restart required"
    } else {
        "restart not required"
    };
    format!(
        "{changed_count} changed tool(s): {added_count} added, {removed_count} removed, {updated_count} updated; {restart_text}."
    )
}

/// Resolve notice severity from change state and refresh mode.
/// 根据变化状态和刷新模式解析提示严重级别。
fn resolve_notice_severity(
    changed: bool,
    refresh_mode: ToolRefreshMode,
) -> ToolRefreshNoticeSeverity {
    if !changed {
        return ToolRefreshNoticeSeverity::None;
    }
    match refresh_mode {
        ToolRefreshMode::Dynamic => ToolRefreshNoticeSeverity::Info,
        ToolRefreshMode::RestartRequired => ToolRefreshNoticeSeverity::Warning,
        ToolRefreshMode::Unsupported => ToolRefreshNoticeSeverity::Error,
    }
}

/// Extract stable ids from one tool descriptor bucket.
/// 从一个 tool 描述符分组中提取稳定 id。
pub(super) fn tool_ids(tools: &[ToolDescriptorSnapshot]) -> Vec<String> {
    let mut ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
    ids.sort();
    ids
}

/// Build a model-facing refresh message.
/// 构建模型可见的刷新提示消息。
fn build_model_refresh_message(
    host_display_name: &str,
    changed: bool,
    refresh_mode: ToolRefreshMode,
    changed_tool_ids: &[String],
) -> String {
    if !changed {
        return "Tool registry unchanged. Continue using the existing tool surface.".to_string();
    }
    let ids = changed_tool_ids.join(", ");
    match refresh_mode {
        ToolRefreshMode::Dynamic => format!(
            "Tool registry changed for {host_display_name}, and dynamic refresh is supported. Changed tools: {ids}."
        ),
        ToolRefreshMode::RestartRequired => format!(
            "Tool registry changed for {host_display_name}. Restart or reconnect the host before relying on these changed tools: {ids}."
        ),
        ToolRefreshMode::Unsupported => format!(
            "Tool registry changed for {host_display_name}, but this host has no safe refresh path. Changed tools: {ids}."
        ),
    }
}

/// Build a user-facing refresh message.
/// 构建用户可见的刷新提示消息。
fn build_user_refresh_message(
    host_display_name: &str,
    changed: bool,
    refresh_mode: ToolRefreshMode,
    changed_tool_ids: &[String],
) -> String {
    if !changed {
        return "工具注册表没有变化，无需重启宿主。".to_string();
    }
    let ids = changed_tool_ids.join(", ");
    match refresh_mode {
        ToolRefreshMode::Dynamic => {
            format!(
                "{host_display_name} 已支持动态刷新，本次 tool 变化可以继续使用。变化项：{ids}。"
            )
        }
        ToolRefreshMode::RestartRequired => {
            format!(
                "{host_display_name} 的 tool 表面已变化，请重启或重新连接宿主后再依赖这些 tool：{ids}。"
            )
        }
        ToolRefreshMode::Unsupported => {
            format!(
                "{host_display_name} 的 tool 表面已变化，但当前没有安全刷新路径。变化项：{ids}。"
            )
        }
    }
}

/// Normalize arbitrary JSON values into stable-key-order structures.
/// 把任意 JSON 值归一化为键顺序稳定的结构。
fn stable_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(stable_json_value).collect()),
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            let mut stable = serde_json::Map::new();
            for (key, value) in entries {
                stable.insert(key.clone(), stable_json_value(value));
            }
            Value::Object(stable)
        }
        _ => value.clone(),
    }
}
