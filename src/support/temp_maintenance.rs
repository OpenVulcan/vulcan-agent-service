use super::runtime_layout::luaskills_runtime_root;
use chrono::{DateTime, Datelike, Local};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Maximum retention period for files inside the runtime temp directory. Files older than this are deleted during cleanup.
/// 临时目录中文件允许保留的最长时间，超过该时间的文件会在清理时被删除。
const TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Poll interval for the cross-day cleanup loop. Running services check at this cadence to see whether a new day has started.
/// 跨日检查循环的轮询周期。运行中的服务会按该周期检查是否进入新的一天。
const DAILY_CLEANUP_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Runtime state for temp-directory cleanup. It only records the day key of the last day-based cleanup.
/// 临时目录清理的运行时状态，只记录“上一次按天清理”的日期键。
#[derive(Debug, Default)]
struct TempMaintenanceState {
    last_cleanup_day_key: Option<u64>,
}

/// Global temp-maintenance state that prevents repeated day-based cleanup within the same day.
/// 全局临时目录清理状态，确保同一天内不会重复做“跨日清理”。
static TEMP_MAINTENANCE_STATE: OnceLock<Mutex<TempMaintenanceState>> = OnceLock::new();

/// Optional explicit runtime root captured from host configuration so temp maintenance follows the unified runtime layout.
/// 从宿主配置捕获的可选显式运行根目录，用于让临时目录维护遵循统一运行时布局。
static CONFIGURED_RUNTIME_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Cleanup trigger type. `Startup` forces cleanup on process start, while `DayBoundary` is used by the background cross-day pass.
/// 清理触发类型。`Startup` 表示启动时强制清理，`DayBoundary` 表示跨日后的后台清理。
#[derive(Debug, Clone, Copy)]
pub enum CleanupTrigger {
    Startup,
    DayBoundary,
}

/// Return the global temp-maintenance state container.
/// 返回全局清理状态对象。
fn maintenance_state() -> &'static Mutex<TempMaintenanceState> {
    TEMP_MAINTENANCE_STATE.get_or_init(|| Mutex::new(TempMaintenanceState::default()))
}

/// Convert the current time into a local-date day key used to detect day boundaries.
/// 把当前时间转换成本地日期键值，用于判断是否跨日。
fn current_day_key(now: SystemTime) -> u64 {
    let local_datetime: DateTime<Local> = now.into();
    let local_date = local_datetime.date_naive();
    ((local_date.year() as i64) << 9 | local_date.ordinal0() as i64) as u64
}

/// Register one configured runtime root so temp maintenance resolves under the same unified runtime layout.
/// 注册一份显式运行根目录，让临时目录维护与统一运行时布局保持一致。
///
/// Parameters: `runtime_root` is the optional resolved runtime root selected by startup configuration.
/// 参数：`runtime_root` 是启动配置选出的可选已解析运行根。
///
/// Returns: `Ok(())` when the root is absent, newly registered, or identical to the existing root.
/// 返回：当运行根缺失、新注册或与既有运行根一致时返回 `Ok(())`。
pub fn initialize_runtime_temp_root(runtime_root: Option<&Path>) -> Result<(), String> {
    initialize_runtime_temp_root_cell(&CONFIGURED_RUNTIME_ROOT, runtime_root)
}

/// Register one runtime root into a concrete OnceLock cell while rejecting conflicting roots.
/// 将一个运行根注册到指定 OnceLock 单元，并拒绝互相冲突的运行根。
///
/// Parameters: `cell` stores the process-wide or test-local runtime root.
/// 参数：`cell` 存储进程级或测试局部的运行根。
///
/// Parameters: `runtime_root` is the optional runtime root to register.
/// 参数：`runtime_root` 是需要注册的可选运行根。
///
/// Returns: `Ok(())` when registration is compatible, or an error describing the conflicting roots.
/// 返回：注册兼容时返回 `Ok(())`，否则返回描述冲突运行根的错误。
fn initialize_runtime_temp_root_cell(
    cell: &OnceLock<PathBuf>,
    runtime_root: Option<&Path>,
) -> Result<(), String> {
    let Some(root) = runtime_root else {
        return Ok(());
    };

    if let Some(existing_root) = cell.get() {
        if existing_root == root {
            return Ok(());
        }
        return Err(format!(
            "runtime temp root already initialized as {}, cannot reinitialize as {}",
            existing_root.display(),
            root.display()
        ));
    }

    match cell.set(root.to_path_buf()) {
        Ok(()) => Ok(()),
        Err(rejected_root) => {
            let Some(existing_root) = cell.get() else {
                return Err(format!(
                    "runtime temp root initialization raced while registering {}",
                    rejected_root.display()
                ));
            };
            if existing_root == &rejected_root {
                return Ok(());
            }
            Err(format!(
                "runtime temp root already initialized as {}, cannot reinitialize as {}",
                existing_root.display(),
                rejected_root.display()
            ))
        }
    }
}

/// Ensure the temp directory under one already resolved runtime root exists and return its path.
/// 确保已解析运行根下的 temp 目录存在，并返回该目录路径。
pub fn ensure_runtime_temp_dir_for_root(
    runtime_root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // A caller-owned runtime root is authoritative, so temp placement should not consult global fallback state.
    // 调用方持有的运行根具备权威性，因此 temp 位置不应再查询全局回退状态。
    let temp_dir = runtime_root.join("temp");
    fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// Derive the temp directory from an explicit runtime root when provided, otherwise from the executable output layout.
/// 当提供显式运行根目录时从其派生 temp 目录，否则按可执行文件输出布局派生。
fn derive_runtime_temp_dir(
    explicit_runtime_root: Option<&Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(root) = explicit_runtime_root {
        return Ok(root.join("temp"));
    }
    let exe_path = std::env::current_exe()?;
    derive_runtime_temp_dir_from_exe_path(&exe_path)
}

/// Derive the LuaSkills temp directory from an executable path only when its application layout is recognizable.
/// 仅当可执行文件所属应用布局可识别时，才从可执行文件路径派生 LuaSkills temp 目录。
fn derive_runtime_temp_dir_from_exe_path(
    exe_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // The executable must live under a real directory before any runtime-root inference can be trusted.
    // 可执行文件必须位于真实目录下，运行根推导才有可信基础。
    let exe_dir = exe_path
        .parent()
        .ok_or("runtime temp dir: executable directory not found")?;
    // The supported hosted layout places binaries below `<application_root>/bin`, so its parent is the application root.
    // 受支持的宿主布局会把二进制放在 `<application_root>/bin` 下，因此其上级目录是应用根。
    let application_root = exe_dir
        .parent()
        .ok_or("runtime temp dir: executable parent application root not found")?;
    if !looks_like_hosted_application_root(application_root)? {
        return Err(format!(
            "runtime temp dir: executable parent is not an application root: {}",
            application_root.display()
        )
        .into());
    }
    // LuaSkills owns all runtime state below the fixed child package; host-level temp is intentionally unsupported.
    // LuaSkills 在固定子包下拥有全部运行状态；这里有意不再支持宿主顶层 temp。
    Ok(luaskills_runtime_root(application_root).join("temp"))
}

/// Return whether a directory has the fixed hosted application markers used by config and LuaSkills discovery.
/// 判断某个目录是否具备配置与 LuaSkills 发现使用的固定宿主应用标记。
fn looks_like_hosted_application_root(root: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let configs_present =
        optional_temp_directory_present(&root.join("configs"), "runtime temp configs marker")?;
    let lua_runtime_present = optional_temp_directory_present(
        &luaskills_runtime_root(root),
        "runtime temp LuaSkills marker",
    )?;
    Ok(configs_present || lua_runtime_present)
}

/// Return whether one optional temp-maintenance directory exists and reject non-directory shapes.
/// 返回一个可选临时目录维护路径是否存在，并拒绝非目录形态。
/// Parameters: `path` is the temp-maintenance path to inspect.
/// 参数：`path` 是需要检查的临时目录维护路径。
/// Parameters: `path_label` names the path role in diagnostics.
/// 参数：`path_label` 用于在诊断中标识路径角色。
/// Returns `true` when present, `false` when absent, or an inspection/shape error.
/// 目录存在时返回 `true`，缺失时返回 `false`，否则返回检查/形态错误。
fn optional_temp_directory_present(
    path: &Path,
    path_label: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(
                    format!("{} is not a directory: {}", path_label, path.display()).into(),
                );
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to inspect {} {}: {}",
            path_label,
            path.display(),
            error
        )
        .into()),
    }
}

/// Resolve the runtime temp root. Configured runtime roots take precedence over executable-derived fallbacks.
/// 解析运行时 temp 根目录。显式配置的运行根优先，其次才是基于可执行文件位置的回退规则。
pub fn resolve_runtime_temp_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    derive_runtime_temp_dir(CONFIGURED_RUNTIME_ROOT.get().map(PathBuf::as_path))
}

/// Ensure the runtime temp directory exists and return its path.
/// 确保运行时 temp 目录存在，并返回该目录路径。
pub fn ensure_runtime_temp_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let temp_dir = resolve_runtime_temp_dir()?;
    fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// Clean a directory tree by deleting files older than the retention window and removing empty directories along the way.
/// 清理单个目录树，删除修改时间超过保留期的文件，并顺带移除空目录。
fn cleanup_directory_recursive(
    directory_path: &Path,
    now: SystemTime,
    retention: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    if !optional_temp_directory_present(directory_path, "runtime temp cleanup directory")? {
        return Ok(());
    }

    for entry in fs::read_dir(directory_path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            cleanup_directory_recursive(&entry_path, now, retention)?;
            if fs::read_dir(&entry_path)?.next().is_none() {
                // Empty temp directories should disappear, and deletion errors need path context for diagnosis.
                // 空临时目录应被移除，删除失败时需要携带路径上下文便于诊断。
                fs::remove_dir(&entry_path).map_err(|error| {
                    format!(
                        "remove empty temp directory failed for {}: {}",
                        entry_path.display(),
                        error
                    )
                })?;
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        // Read the actual modification timestamp so platform metadata failures cannot masquerade as fresh temp files.
        // 读取真实修改时间，避免平台元数据错误被伪装成“刚生成的临时文件”。
        let modified_time = metadata.modified().map_err(|error| {
            format!(
                "read temp file modified time failed for {}: {}",
                entry_path.display(),
                error
            )
        })?;
        // Treat files from the future as not expired; the clock says they have not exceeded the retention window yet.
        // 将未来时间戳的文件视为未过期；按照当前时钟它们尚未超过保留窗口。
        let file_age = match now.duration_since(modified_time) {
            Ok(age) => age,
            Err(_) => continue,
        };
        if file_age > retention {
            fs::remove_file(&entry_path).map_err(|error| {
                format!(
                    "remove expired temp file failed for {}: {}",
                    entry_path.display(),
                    error
                )
            })?;
        }
    }

    Ok(())
}

/// Perform one temp-directory cleanup pass. Startup cleanup is forced, while day-boundary cleanup runs at most once per day.
/// 执行一次临时目录清理。启动清理会强制执行；跨日清理则同一天只执行一次。
pub fn maintain_runtime_temp_dir(
    trigger: CleanupTrigger,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = SystemTime::now();
    let current_day = current_day_key(now);
    let mut state = maintenance_state()
        .lock()
        .map_err(|_| "temp maintenance state poisoned")?;

    let should_run = match trigger {
        CleanupTrigger::Startup => true,
        CleanupTrigger::DayBoundary => state.last_cleanup_day_key != Some(current_day),
    };

    if !should_run {
        return Ok(());
    }

    let temp_dir = ensure_runtime_temp_dir()?;
    cleanup_directory_recursive(&temp_dir, now, TEMP_FILE_RETENTION)?;
    state.last_cleanup_day_key = Some(current_day);
    Ok(())
}

/// Start the background cross-day cleanup task. It polls hourly and runs one cleanup pass when a new day is detected.
/// 启动后台跨日清理任务。该任务按小时轮询，并在发现进入新的一天后执行一次清理。
pub fn spawn_cross_day_cleanup_task() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(DAILY_CLEANUP_POLL_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(error) = maintain_runtime_temp_dir(CleanupTrigger::DayBoundary) {
                eprintln!("[TempCleanup] Cross-day cleanup failed: {}", error);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit runtime roots should own the temp directory so all runtime artifacts stay under one unified root.
    /// 显式运行根目录应接管 temp 目录位置，以保证所有运行时产物都位于统一根目录之下。
    #[test]
    fn derive_runtime_temp_dir_prefers_explicit_runtime_root() {
        let root = std::env::temp_dir().join("vulcan-agent-service-temp-maintenance-test-root");
        let derived = derive_runtime_temp_dir(Some(&root)).expect("temp dir should derive");
        assert_eq!(derived, root.join("temp"));
    }

    /// Known runtime roots should create and return their own temp directory without executable fallback.
    /// 已知运行根应直接创建并返回自身 temp 目录，不经过可执行文件回退。
    #[test]
    fn ensure_runtime_temp_dir_for_root_creates_runtime_temp_dir() {
        // Use a unique runtime root so the directory creation assertion is isolated.
        // 使用唯一运行根目录，确保目录创建断言彼此隔离。
        let runtime_root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-root-known-{}",
            uuid::Uuid::new_v4()
        ));

        let temp_dir = ensure_runtime_temp_dir_for_root(&runtime_root)
            .expect("known runtime root temp dir should be created");

        assert_eq!(temp_dir, runtime_root.join("temp"));
        assert!(temp_dir.is_dir());
        std::fs::remove_dir_all(&runtime_root).expect("test runtime root should be removed");
    }

    /// Runtime temp root registration should accept repeated registration of the same root.
    /// runtime temp 根注册应接受同一运行根的重复注册。
    #[test]
    fn initialize_runtime_temp_root_cell_accepts_same_root() {
        // Build a local OnceLock so the test does not mutate the process-global runtime root.
        // 构造局部 OnceLock，避免测试修改进程级运行根。
        let cell = OnceLock::new();
        // Build one deterministic local root path for equality checks.
        // 构造一个确定的局部运行根路径用于相等性检查。
        let runtime_root = std::env::temp_dir().join("vulcan-agent-service-temp-same-root");

        initialize_runtime_temp_root_cell(&cell, Some(runtime_root.as_path()))
            .expect("first runtime temp root registration should succeed");
        initialize_runtime_temp_root_cell(&cell, Some(runtime_root.as_path()))
            .expect("same runtime temp root registration should remain compatible");

        assert_eq!(cell.get(), Some(&runtime_root));
    }

    /// Runtime temp root registration should reject conflicting roots instead of keeping the first one silently.
    /// runtime temp 根注册应拒绝互相冲突的运行根，而不是静默保留第一次的值。
    #[test]
    fn initialize_runtime_temp_root_cell_rejects_conflicting_root() {
        // Build a local OnceLock so the conflict assertion cannot leak into other tests.
        // 构造局部 OnceLock，确保冲突断言不会泄漏到其它测试。
        let cell = OnceLock::new();
        // Build the initially registered runtime root.
        // 构造首次注册的运行根。
        let first_root = std::env::temp_dir().join("vulcan-agent-service-temp-first-root");
        // Build a different runtime root that should be rejected.
        // 构造一个应被拒绝的不同运行根。
        let second_root = std::env::temp_dir().join("vulcan-agent-service-temp-second-root");

        initialize_runtime_temp_root_cell(&cell, Some(first_root.as_path()))
            .expect("first runtime temp root registration should succeed");
        let error = initialize_runtime_temp_root_cell(&cell, Some(second_root.as_path()))
            .expect_err("conflicting runtime temp root should be rejected");

        assert!(error.contains("runtime temp root already initialized as"));
        assert!(error.contains(&first_root.to_string_lossy().to_string()));
        assert!(error.contains(&second_root.to_string_lossy().to_string()));
        assert_eq!(cell.get(), Some(&first_root));
    }

    /// Runtime temp root registration should ignore an absent root without writing global fallback state.
    /// runtime temp 根注册应忽略缺失运行根，且不写入全局回退状态。
    #[test]
    fn initialize_runtime_temp_root_cell_ignores_absent_root() {
        // Build a local OnceLock so the absence case can assert the cell remains empty.
        // 构造局部 OnceLock，使缺失运行根场景可以断言单元保持为空。
        let cell = OnceLock::new();

        initialize_runtime_temp_root_cell(&cell, None)
            .expect("absent runtime temp root should be accepted");

        assert!(cell.get().is_none());
    }

    /// Executable-side fallback should accept the supported `<application_root>/bin/<exe>` hosted layout.
    /// 可执行文件侧回退应接受受支持的 `<application_root>/bin/<exe>` 宿主布局。
    #[test]
    fn derive_runtime_temp_dir_accepts_hosted_bin_layout() {
        // Use a unique runtime root so the hosted-layout marker is fully controlled by this test.
        // 使用唯一运行根目录，确保宿主布局标记完全由本测试控制。
        let runtime_root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-layout-hosted-{}",
            uuid::Uuid::new_v4()
        ));
        // Create the configs marker because config discovery also treats it as a hosted runtime root.
        // 创建 configs 标记，因为配置发现同样用它识别宿主运行根。
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("runtime root configs marker should be created");
        // Place the fake executable under the hosted bin directory.
        // 将假可执行文件放在宿主 bin 目录下。
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        std::fs::create_dir_all(
            fake_exe
                .parent()
                .expect("fake executable parent should exist"),
        )
        .expect("fake executable parent should be created");

        let derived = derive_runtime_temp_dir_from_exe_path(&fake_exe)
            .expect("hosted executable layout should derive temp dir");

        assert_eq!(derived, runtime_root.join("lua_runtime").join("temp"));
        std::fs::remove_dir_all(&runtime_root).expect("test runtime root should be removed");
    }

    /// Executable-side fallback should reject arbitrary parents that do not look like runtime roots.
    /// 可执行文件侧回退应拒绝不具备运行根形态的任意父目录。
    #[test]
    fn derive_runtime_temp_dir_rejects_unmarked_executable_parent() {
        // Use a unique parent without configs or lua_runtime markers to prove layout inference is explicit.
        // 使用不含 configs 或 lua_runtime 标记的唯一父目录，证明布局推导是显式的。
        let runtime_root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-layout-unmarked-{}",
            uuid::Uuid::new_v4()
        ));
        // Place the fake executable under bin without creating any runtime-root markers.
        // 将假可执行文件放在 bin 下，但不创建任何运行根标记。
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        std::fs::create_dir_all(
            fake_exe
                .parent()
                .expect("fake executable parent should exist"),
        )
        .expect("fake executable parent should be created");

        let error = derive_runtime_temp_dir_from_exe_path(&fake_exe)
            .expect_err("unmarked executable parent should be rejected");

        assert!(
            error
                .to_string()
                .contains("executable parent is not an application root")
        );
        std::fs::remove_dir_all(&runtime_root).expect("test runtime root should be removed");
    }

    /// Executable-side fallback should reject the removed top-level skills marker without a current application marker.
    /// 可执行文件侧回退应拒绝仅含已移除顶层 skills 标记且不含当前应用标记的布局。
    #[test]
    fn derive_runtime_temp_dir_rejects_removed_top_level_skills_layout() {
        // RuntimeRoot contains only the historical marker, proving it no longer authorizes layout discovery.
        // RuntimeRoot 仅包含历史标记，用于证明该标记已不能再授权布局发现。
        let runtime_root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-layout-legacy-skills-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(runtime_root.join("skills"))
            .expect("historical top-level skills marker should be created");
        // FakeExe follows the hosted binary placement while deliberately omitting current markers.
        // FakeExe 遵循宿主二进制位置，同时有意省略当前标记。
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        std::fs::create_dir_all(
            fake_exe
                .parent()
                .expect("fake executable parent should exist"),
        )
        .expect("fake executable parent should be created");

        let error = derive_runtime_temp_dir_from_exe_path(&fake_exe)
            .expect_err("removed top-level skills layout should be rejected");

        assert!(
            error
                .to_string()
                .contains("executable parent is not an application root")
        );
        std::fs::remove_dir_all(&runtime_root).expect("test runtime root should be removed");
    }

    /// Executable-side fallback should reject file-shaped hosted runtime markers instead of treating them as absent.
    /// 可执行文件侧回退应拒绝文件形态的宿主运行根标记，而不是把它们视为缺失。
    #[test]
    fn derive_runtime_temp_dir_rejects_file_shaped_runtime_marker() {
        // Use a unique runtime root with a corrupted configs marker to exercise marker inspection.
        // 使用带有损坏 configs 标记的唯一运行根，以覆盖标记检查逻辑。
        let runtime_root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-layout-file-marker-{}",
            uuid::Uuid::new_v4()
        ));
        // Place the fake executable under the hosted bin directory.
        // 将假可执行文件放在宿主 bin 目录下。
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        std::fs::create_dir_all(
            fake_exe
                .parent()
                .expect("fake executable parent should exist"),
        )
        .expect("fake executable parent should be created");
        // Create a file where the configs marker directory should live.
        // 在 configs 标记目录位置创建一个文件。
        std::fs::write(runtime_root.join("configs"), b"not-a-directory")
            .expect("file-shaped configs marker should be written");

        let error = derive_runtime_temp_dir_from_exe_path(&fake_exe)
            .expect_err("file-shaped runtime marker should be rejected");

        assert!(
            error
                .to_string()
                .contains("runtime temp configs marker is not a directory"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&runtime_root).expect("test runtime root should be removed");
    }

    /// Cleanup should reject a file where the cleanup root must be a directory.
    /// 清理逻辑应拒绝清理根位置出现文件。
    #[test]
    fn cleanup_directory_recursive_rejects_file_shaped_cleanup_root() {
        // Use a unique file path as the cleanup root to prove root inspection is explicit.
        // 使用唯一文件路径作为清理根，以证明根路径检查是显式的。
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-cleanup-file-root-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&root, b"not-a-directory").expect("file-shaped cleanup root should exist");

        let error = cleanup_directory_recursive(&root, SystemTime::now(), TEMP_FILE_RETENTION)
            .expect_err("file-shaped cleanup root should fail");

        assert!(
            error
                .to_string()
                .contains("runtime temp cleanup directory is not a directory"),
            "unexpected error: {error}"
        );
        std::fs::remove_file(&root).expect("test cleanup root file should be removed");
    }

    /// Expired regular files should be removed during a cleanup pass.
    /// 超过保留期的普通文件应在清理过程中被删除。
    #[test]
    fn cleanup_directory_recursive_removes_expired_regular_files() {
        // Isolate this test under a unique temp root so parallel test runs cannot share cleanup state.
        // 使用唯一临时根目录隔离测试，避免并行测试共享清理状态。
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-cleanup-expired-{}",
            uuid::Uuid::new_v4()
        ));
        // Create one regular file whose real filesystem timestamp will be compared against a synthetic cleanup time.
        // 创建一个普通文件，并用合成清理时间与其真实文件系统时间戳比较。
        let file_path = root.join("expired.txt");
        fs::create_dir_all(&root).expect("test temp root should be created");
        fs::write(&file_path, b"expired").expect("test temp file should be written");

        // Advance the cleanup clock beyond the retention window instead of mutating platform file times.
        // 推进清理时钟到保留窗口之后，避免依赖平台文件时间修改能力。
        let modified_time = fs::metadata(&file_path)
            .expect("test temp file metadata should be readable")
            .modified()
            .expect("test temp file modified time should be readable");
        let cleanup_time = modified_time + TEMP_FILE_RETENTION + Duration::from_secs(1);

        cleanup_directory_recursive(&root, cleanup_time, TEMP_FILE_RETENTION)
            .expect("expired temp file cleanup should succeed");

        assert!(!file_path.exists());
        fs::remove_dir_all(&root).expect("test temp root should be removed");
    }

    /// Files dated after the cleanup clock should be kept because they are not older than the retention window.
    /// 修改时间晚于清理时钟的文件应保留，因为它们没有超过保留窗口。
    #[test]
    fn cleanup_directory_recursive_keeps_future_dated_files() {
        // Isolate this test under a unique temp root so cleanup decisions are based only on this file.
        // 使用唯一临时根目录隔离测试，确保清理判断只受当前文件影响。
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-temp-cleanup-future-{}",
            uuid::Uuid::new_v4()
        ));
        // Create one regular file whose modified time is naturally after UNIX_EPOCH on supported platforms.
        // 创建一个普通文件，其修改时间在支持的平台上自然晚于 UNIX_EPOCH。
        let file_path = root.join("future.txt");
        fs::create_dir_all(&root).expect("test temp root should be created");
        fs::write(&file_path, b"future").expect("test temp file should be written");

        cleanup_directory_recursive(&root, SystemTime::UNIX_EPOCH, TEMP_FILE_RETENTION)
            .expect("future-dated temp file cleanup should succeed");

        assert!(file_path.exists());
        fs::remove_dir_all(&root).expect("test temp root should be removed");
    }
}
