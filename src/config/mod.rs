use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Current strict format version required by every host-owned runtime configuration file.
/// 每份宿主持有的运行时配置文件必须使用的当前严格格式版本。
pub const HOST_CONFIG_FORMAT_VERSION: u32 = 1;

/// Runtime application configuration loading and CLI configuration discovery.
/// 运行时应用配置加载与 CLI 配置发现。
pub mod app_config;
/// Client-specific output budget and estimation configuration.
/// 客户端维度输出预算与估算配置。
pub mod client_budget;
/// Host model provider configuration and validation.
/// 宿主模型供应商配置与校验。
pub mod model_config;
/// Shared helpers for runtime-root override discovery.
/// 运行根覆盖发现的共享辅助逻辑。
mod runtime_root;
/// Tool-specific runtime configuration loading and lookup.
/// 工具维度运行时配置加载与查询。
pub mod tool_config;

/// Process-wide transaction lock that keeps multi-config readers on one committed generation.
/// 进程级事务锁，确保多配置读取方始终位于同一个已提交版本。
static RUNTIME_CONFIG_TRANSACTION_LOCK: OnceLock<RwLock<()>> = OnceLock::new();
/// Process-wide test mutex that serializes fixtures mutating runtime-config roots, caches, or model callbacks.
/// 进程级测试互斥锁，用于串行化会修改运行时配置根、缓存或模型回调的夹具。
#[cfg(test)]
static RUNTIME_CONFIG_TEST_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

/// Reports produced by one staged and atomically committed runtime-config refresh.
/// 一次分阶段加载并原子提交的运行时配置刷新报告。
#[derive(Debug, Clone)]
pub(crate) struct RuntimeConfigReloadReport {
    /// Reload report for client budget configuration.
    /// 客户端预算配置刷新报告。
    pub(crate) client_budget: client_budget::ClientBudgetLoadReport,
    /// Reload report for tool configuration.
    /// 工具配置刷新报告。
    pub(crate) tool_config: tool_config::ToolConfigLoadReport,
    /// Reload report for model provider configuration.
    /// 模型供应商配置刷新报告。
    pub(crate) model_config: model_config::ModelConfigLoadReport,
}

/// Return the shared runtime-config transaction lock.
/// 返回共享的运行时配置事务锁。
/// Returns the process-wide lock used by readers and staged commits.
/// 返回读取方与分阶段提交共同使用的进程级锁。
fn runtime_config_transaction_lock() -> &'static RwLock<()> {
    RUNTIME_CONFIG_TRANSACTION_LOCK.get_or_init(|| RwLock::new(()))
}

/// Return the shared mutex used by every test that owns process-wide runtime-config state.
/// 返回所有会占用进程级运行时配置状态的测试共用互斥锁。
/// Returns one stable mutex for the complete test process.
/// 返回整个测试进程共用的稳定互斥锁。
#[cfg(test)]
pub(crate) fn runtime_config_test_lock() -> &'static std::sync::Mutex<()> {
    RUNTIME_CONFIG_TEST_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Acquire a shared guard for one read flow that may combine multiple runtime configs.
/// 为可能组合多份运行时配置的读取流程获取共享守卫。
/// Returns the read guard or an explicit poisoned-lock error.
/// 返回读取守卫或显式的 poisoned 锁错误。
pub(super) fn runtime_config_read_guard() -> Result<RwLockReadGuard<'static, ()>, String> {
    runtime_config_transaction_lock()
        .read()
        .map_err(|_| "runtime config transaction lock poisoned".to_string())
}

/// Acquire an exclusive guard for a runtime-config cache commit.
/// 为运行时配置缓存提交获取独占守卫。
/// Returns the write guard or an explicit poisoned-lock error.
/// 返回写入守卫或显式的 poisoned 锁错误。
fn runtime_config_write_guard() -> Result<RwLockWriteGuard<'static, ()>, String> {
    runtime_config_transaction_lock()
        .write()
        .map_err(|_| "runtime config transaction lock poisoned".to_string())
}

/// Stage every hot-reloadable runtime config, then commit all cache states as one generation.
/// 分阶段加载全部可热重载运行时配置，再把所有缓存状态作为同一版本提交。
/// Returns all load reports after a successful commit, or the first staging/lock error without a partial write.
/// 成功提交后返回全部加载报告；若分阶段加载或加锁失败，则返回首个错误且不执行部分写入。
pub(crate) fn reload_runtime_configs() -> Result<RuntimeConfigReloadReport, String> {
    // Stage disk reads and semantic validation before excluding request-time readers.
    // 在阻塞请求期读取方之前完成磁盘读取与语义校验。
    let (client_budget_runtime, client_budget_report) =
        client_budget::stage_client_budget_runtime()?;
    let (tool_config_runtime, tool_config_report) = tool_config::stage_tool_config_runtime()?;
    let (model_config_runtime, model_config_report) = model_config::stage_model_config_runtime()?;

    // Prevent readers from observing mixed generations while all cache locks are replaced.
    // 在替换全部缓存锁期间阻止读取方观察到混合版本。
    let _transaction_guard = runtime_config_write_guard()?;
    replace_runtime_states(
        client_budget::client_budget_runtime(),
        Ok(client_budget_runtime),
        tool_config::tool_config_runtime(),
        Ok(tool_config_runtime),
        model_config::model_config_runtime(),
        Ok(model_config_runtime),
    )?;

    Ok(RuntimeConfigReloadReport {
        client_budget: client_budget_report,
        tool_config: tool_config_report,
        model_config: model_config_report,
    })
}

/// Acquire every cache write lock before replacing any state, preventing lock failures from partially committing.
/// 在替换任何状态前获取全部缓存写锁，避免加锁失败造成部分提交。
/// Parameters: each lock/value pair identifies one cache and its staged replacement state.
/// 参数：每组锁与值标识一份缓存及其分阶段替换状态。
/// Returns success after all three replacements, or a cache-specific poisoned-lock error before mutation.
/// 三份状态全部替换后返回成功；若锁 poisoned，则在修改前返回对应缓存错误。
fn replace_runtime_states<ClientState, ToolState, ModelState>(
    client_budget_lock: &RwLock<ClientState>,
    client_budget_state: ClientState,
    tool_config_lock: &RwLock<ToolState>,
    tool_config_state: ToolState,
    model_config_lock: &RwLock<ModelState>,
    model_config_state: ModelState,
) -> Result<(), String> {
    let mut client_budget_guard = client_budget_lock
        .write()
        .map_err(|_| "client budget runtime lock poisoned".to_string())?;
    let mut tool_config_guard = tool_config_lock
        .write()
        .map_err(|_| "tool config runtime lock poisoned".to_string())?;
    let mut model_config_guard = model_config_lock
        .write()
        .map_err(|_| "model config runtime lock poisoned".to_string())?;

    *client_budget_guard = client_budget_state;
    *tool_config_guard = tool_config_state;
    *model_config_guard = model_config_state;
    Ok(())
}

/// Re-export application config types for the stable `crate::config::Config` entrypoint.
/// 为稳定的 `crate::config::Config` 入口重导出应用配置类型。
pub use app_config::*;

#[cfg(test)]
mod tests {
    use super::replace_runtime_states;
    use std::sync::{Arc, PoisonError, RwLock};

    /// Verify that a poisoned cache lock prevents every staged state from being committed.
    /// 验证任一缓存锁 poisoned 时，所有分阶段状态都不会被提交。
    /// Returns nothing; assertions prove that both healthy caches retain their original values.
    /// 无返回值；断言证明两份健康缓存仍保留原值。
    #[test]
    fn replace_runtime_states_does_not_partially_commit_when_a_lock_is_poisoned() {
        // Use independent local locks so the poison scenario cannot affect process-wide test state.
        // 使用独立的局部锁，避免 poison 场景影响进程级测试状态。
        let client_budget_lock = RwLock::new(10_u8);
        // Share only the middle lock with the poison-producing thread.
        // 仅与制造 poison 的线程共享中间锁。
        let tool_config_lock = Arc::new(RwLock::new(20_u8));
        // Preserve a healthy trailing lock to detect an accidental partial commit.
        // 保留健康的末尾锁，用于检测意外的部分提交。
        let model_config_lock = RwLock::new(30_u8);
        // Clone the shared lock into the thread that intentionally panics while holding its writer guard.
        // 把共享锁克隆到线程中，该线程会在持有写守卫时故意 panic。
        let poisoned_tool_config_lock = Arc::clone(&tool_config_lock);
        // Join consumes the intentional panic and leaves the lock in a deterministic poisoned state.
        // join 消化预期的 panic，并让锁稳定地进入 poisoned 状态。
        let poison_result = std::thread::spawn(move || {
            // Hold the writer guard until unwinding marks the lock as poisoned.
            // 保持写守卫直到展开过程将锁标记为 poisoned。
            let _guard = poisoned_tool_config_lock
                .write()
                .expect("fresh test lock should be writable");
            panic!("intentional lock poison");
        })
        .join();
        assert!(poison_result.is_err());

        // Attempt one three-cache commit with distinct replacement values.
        // 使用不同的替换值尝试一次三缓存提交。
        let error = replace_runtime_states(
            &client_budget_lock,
            11_u8,
            tool_config_lock.as_ref(),
            21_u8,
            &model_config_lock,
            31_u8,
        )
        .expect_err("poisoned tool-config lock should abort the complete commit");

        assert_eq!(error, "tool config runtime lock poisoned");
        assert_eq!(
            *client_budget_lock
                .read()
                .expect("healthy client-budget lock should remain readable"),
            10_u8
        );
        assert_eq!(
            *model_config_lock
                .read()
                .expect("healthy model-config lock should remain readable"),
            30_u8
        );
        assert_eq!(
            *tool_config_lock
                .read()
                .unwrap_or_else(PoisonError::into_inner),
            20_u8
        );
    }
}
