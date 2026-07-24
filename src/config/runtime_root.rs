use crate::support::hosted_application_root_from_executable;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{LockResult, RwLockReadGuard};

/// Clone one runtime-root override from a read-lock result without hiding poisoned locks.
/// 从读锁结果中克隆运行根覆盖值，且不隐藏 poisoned 锁。
/// Parameters: `lock_result` is the read-lock result produced by a runtime-root override store.
/// 参数：`lock_result` 是运行根覆盖存储产生的读锁结果。
/// Parameters: `lock_error_message` is the caller-specific error returned when the lock is poisoned.
/// 参数：`lock_error_message` 是锁被 poisoned 时返回的调用方专用错误。
/// Returns the cloned runtime-root override or a lock error.
/// 返回克隆后的运行根覆盖值或锁错误。
pub(super) fn clone_runtime_root_override(
    lock_result: LockResult<RwLockReadGuard<'_, Option<PathBuf>>>,
    lock_error_message: &str,
) -> Result<Option<PathBuf>, String> {
    // Convert poisoned locks into explicit discovery errors instead of pretending no override exists.
    // 将 poisoned 锁转换为显式发现错误，而不是假装没有覆盖值。
    let guard = lock_result.map_err(|_| lock_error_message.to_string())?;
    Ok(guard.clone())
}

/// Find one optional config file from an explicit runtime root, executable-side runtime root, or repository runtime template.
/// 从显式运行根、可执行文件侧运行根或仓库 runtime 模板中查找一个可选配置文件。
/// Parameters: `runtime_root` is the optional explicit runtime root already selected by startup.
/// 参数：`runtime_root` 是启动流程已经选定的可选显式运行根。
/// Parameters: `file_name` is the config file name under the `configs` directory.
/// 参数：`file_name` 是 `configs` 目录下的配置文件名。
/// Parameters: `config_label` names the config type in diagnostics.
/// 参数：`config_label` 用于在诊断信息中标识配置类型。
/// Returns the config file path, `None` when absent, or an inspection/discovery error.
/// 返回配置文件路径、缺失时的 `None`，或路径检查/发现错误。
pub(super) fn find_optional_runtime_config_file(
    runtime_root: Option<PathBuf>,
    file_name: &str,
    config_label: &str,
) -> Result<Option<PathBuf>, String> {
    if let Some(runtime_root) = runtime_root {
        // Explicit runtime roots are authoritative; a missing optional config there means the config is absent.
        // 显式运行根具备权威性；该位置缺少可选配置表示配置不存在。
        return optional_config_file_at(runtime_root.join("configs").join(file_name), config_label);
    }

    let exe_path = std::env::current_exe().map_err(|error| {
        format!("failed to resolve current executable while locating {config_label}: {error}")
    })?;
    if let Some(parent_dir) = hosted_application_root_from_executable(&exe_path) {
        let runtime_path = parent_dir.join("configs").join(file_name);
        if optional_config_file_at(&runtime_path, config_label)?.is_some() {
            return Ok(Some(runtime_path));
        }
    }

    let repository_path = Path::new("runtime").join("configs").join(file_name);
    optional_config_file_at(repository_path, config_label)
}

/// Inspect one optional config path and reject non-file shapes or metadata errors.
/// 检查一个可选配置路径，并拒绝非文件形态或元数据错误。
/// Parameters: `path` is the candidate config file path.
/// 参数：`path` 是候选配置文件路径。
/// Parameters: `config_label` names the config type in diagnostics.
/// 参数：`config_label` 用于在诊断信息中标识配置类型。
/// Returns the file path when it exists, `None` when absent, or an inspection error.
/// 返回存在的文件路径、缺失时的 `None`，或路径检查错误。
fn optional_config_file_at(
    path: impl AsRef<Path>,
    config_label: &str,
) -> Result<Option<PathBuf>, String> {
    let path = path.as_ref();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to inspect {config_label} config path {}: {}",
                path.display(),
                error
            ));
        }
    };
    if !metadata.is_file() {
        return Err(format!(
            "{config_label} config path is not a file: {}",
            path.display()
        ));
    }
    Ok(Some(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{PoisonError, RwLock};

    /// Runtime-root helper should report poisoned locks instead of returning no override.
    /// 运行根 helper 应报告 poisoned 锁，而不是返回无覆盖值。
    #[test]
    fn clone_runtime_root_override_reports_poisoned_lock() {
        // Build a local read guard so the test never poisons a process-global lock.
        // 构造局部读锁 guard，避免测试污染进程级全局锁。
        let lock = RwLock::new(Some(PathBuf::from("runtime")));
        let guard = lock.read().expect("local runtime-root lock should read");

        // Wrap the local guard in a synthetic poison error.
        // 将局部 guard 包装为合成的 poison 错误。
        let poisoned = Err(PoisonError::new(guard));

        // Clone through the shared helper used by config discovery paths.
        // 通过配置发现路径复用的共享 helper 执行克隆。
        let error = clone_runtime_root_override(poisoned, "runtime-root lock poisoned")
            .expect_err("poisoned runtime-root override lock should become an explicit error");

        assert_eq!(error, "runtime-root lock poisoned");
    }

    /// Optional config helper should return the explicit runtime config file when it exists.
    /// 可选配置 helper 应在显式运行根配置文件存在时返回该文件。
    #[test]
    fn find_optional_runtime_config_file_returns_explicit_runtime_file() {
        // Create an isolated explicit runtime root so no executable fallback participates in the assertion.
        // 创建隔离的显式运行根，确保断言不受可执行文件回退影响。
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-runtime-config-helper-{}",
            uuid::Uuid::new_v4()
        ));
        // Write one optional config file under the standard configs directory.
        // 在标准 configs 目录下写入一个可选配置文件。
        let config_path = root.join("configs").join("client_budgets.yaml");
        std::fs::create_dir_all(config_path.parent().expect("config parent should exist"))
            .expect("config parent should be created");
        std::fs::write(&config_path, "format_version: 1\ndefaults: {}\n")
            .expect("config file should be written");

        let found =
            find_optional_runtime_config_file(Some(root.clone()), "client_budgets.yaml", "test")
                .expect("optional config lookup should succeed");

        assert_eq!(found, Some(config_path));
        std::fs::remove_dir_all(&root).expect("test runtime root should be removed");
    }

    /// Optional config helper should reject directory-shaped config paths.
    /// 可选配置 helper 应拒绝目录形态的配置路径。
    #[test]
    fn find_optional_runtime_config_file_rejects_directory_shaped_config_path() {
        // Create an isolated explicit runtime root with a directory where the config file should be.
        // 创建隔离的显式运行根，并在配置文件位置放置目录。
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-runtime-config-directory-{}",
            uuid::Uuid::new_v4()
        ));
        let config_path = root.join("configs").join("tool_configs.yaml");
        std::fs::create_dir_all(&config_path).expect("directory-shaped config should be created");

        let error =
            find_optional_runtime_config_file(Some(root.clone()), "tool_configs.yaml", "test")
                .expect_err("directory-shaped optional config should fail");

        assert!(
            error.contains("config path is not a file"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&root).expect("test runtime root should be removed");
    }
}
