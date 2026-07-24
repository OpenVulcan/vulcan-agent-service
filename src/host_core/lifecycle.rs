use std::sync::{LockResult, Mutex, MutexGuard, OnceLock};

use crate::host_core::state::ServerInner;
use crate::luaskills_adapter::map_runtime_entry_to_mcp_tool;
use luaskills::{RuntimeEntryDescriptor, RuntimeEntryRegistryDelta};

/// Acquire the process-wide lock that serializes LuaSkills lifecycle callback installation.
/// 获取用于串行化 LuaSkills 生命周期回调安装的进程级互斥锁。
pub(super) fn lock_luaskills_lifecycle_callback() -> MutexGuard<'static, ()> {
    let lock = luaskills_lifecycle_callback_lock();
    recover_luaskills_lifecycle_callback_guard(lock, lock.lock())
}

/// Return the process-wide lock that protects LuaSkills lifecycle callback installation.
/// 返回保护 LuaSkills 生命周期回调安装的进程级互斥锁。
fn luaskills_lifecycle_callback_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Recover a lifecycle callback lock guard when only the unit-valued guard state was poisoned.
/// 当只有单位值锁状态被 poisoning 时恢复生命周期回调锁守卫。
fn recover_luaskills_lifecycle_callback_guard(
    lock: &'static Mutex<()>,
    lock_result: LockResult<MutexGuard<'static, ()>>,
) -> MutexGuard<'static, ()> {
    match lock_result {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!(
                "[LuaSkills:lifecycle] callback installation lock was poisoned; recovering because the guarded state is unit-valued"
            );
            lock.clear_poison();
            poisoned.into_inner()
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Recover a poisoned unit-valued lifecycle callback guard without panicking.
    /// 验证单位值生命周期回调锁守卫被标记为 poisoned 时可恢复且不会 panic。
    #[test]
    fn recover_luaskills_lifecycle_callback_guard_accepts_poisoned_unit_guard() {
        // Acquire the real guard so the recovery helper receives the exact guard type used in production.
        // 获取真实守卫，确保恢复 helper 接收生产路径使用的同一守卫类型。
        let guard = luaskills_lifecycle_callback_lock()
            .lock()
            .expect("test lifecycle callback lock should be available");

        // Wrap the guard as poisoned without permanently poisoning the process-wide mutex.
        // 将守卫包装为 poisoned，且不永久污染进程级互斥锁。
        let recovered = recover_luaskills_lifecycle_callback_guard(
            luaskills_lifecycle_callback_lock(),
            Err(std::sync::PoisonError::new(guard)),
        );

        drop(recovered);
    }
}
