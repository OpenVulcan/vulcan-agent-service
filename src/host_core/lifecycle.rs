use crate::host_core::state::ServerInner;
use crate::luaskills_adapter::map_runtime_entry_to_mcp_tool;
use luaskills::{RuntimeEntryDescriptor, RuntimeEntryRegistryDelta};

/// Return the process-wide lock that protects LuaSkills lifecycle callback installation.
/// 返回保护 LuaSkills 生命周期回调安装的进程级互斥锁。
pub(super) fn luaskills_lifecycle_callback_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Apply one runtime entry-registry delta to the MCP host tool registry.
/// 把一份运行时入口注册表差异应用到 MCP 宿主工具注册表。
pub(super) fn apply_runtime_entry_registry_delta(
    inner: &mut ServerInner,
    delta: &RuntimeEntryRegistryDelta,
) {
    for removed_name in &delta.removed_entry_names {
        inner.skill_tools.remove(removed_name);
        inner.skill_entries.remove(removed_name);
    }
    for entry in &delta.updated_entries {
        insert_skill_entry(inner, entry.clone());
    }
    for entry in &delta.added_entries {
        insert_skill_entry(inner, entry.clone());
    }
}

/// Insert one LuaSkills runtime entry into the dynamic registry while rejecting host-reserved name collisions.
/// 将单个 LuaSkills 运行时入口插入动态注册表，并拒绝与宿主保留名称发生冲突。
pub(super) fn insert_skill_entry(inner: &mut ServerInner, entry: RuntimeEntryDescriptor) {
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    if inner.host_tools.contains_key(&tool.name) {
        eprintln!(
            "[LuaSkills] Skip dynamic tool '{}' because it collides with a host-owned tool",
            tool.name
        );
        inner.skill_tools.remove(&tool.name);
        inner.skill_entries.remove(&tool.name);
        return;
    }
    inner.skill_entries.insert(tool.name.clone(), entry);
    inner.skill_tools.insert(tool.name.clone(), tool);
}
