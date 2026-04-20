use chrono::{DateTime, Datelike, Local};
use std::fs;
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

/// Resolve the runtime temp root. The rule is `<exe_parent_parent>/temp`.
/// 解析运行时 temp 根目录，规则为“可执行文件目录的上级目录/temp”。
pub fn resolve_runtime_temp_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or("runtime temp dir: executable directory not found")?;
    let runtime_root = exe_dir.parent().unwrap_or(exe_dir);
    Ok(runtime_root.join("temp"))
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
    if !directory_path.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(directory_path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            cleanup_directory_recursive(&entry_path, now, retention)?;
            if fs::read_dir(&entry_path)?.next().is_none() {
                let _ = fs::remove_dir(&entry_path);
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let modified_time = metadata.modified().unwrap_or(now);
        let file_age = now.duration_since(modified_time).unwrap_or_default();
        if file_age > retention {
            let _ = fs::remove_file(&entry_path);
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
