use chrono::{DateTime, Datelike, Local};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// 中文：临时目录中文件允许保留的最长时间，超过该时间的文件会在清理时被删除。
/// English: Maximum retention period for files inside the runtime temp directory. Files older than this are deleted during cleanup.
const TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// 中文：跨日检查循环的轮询周期。运行中的服务会按该周期检查是否进入新的一天。
/// English: Poll interval for the cross-day cleanup loop. Running services check at this cadence to see whether a new day has started.
const DAILY_CLEANUP_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// 中文：临时目录清理的运行时状态，只记录“上一次按天清理”的日期键。
/// English: Runtime state for temp-directory cleanup. It only records the day key of the last day-based cleanup.
#[derive(Debug, Default)]
struct TempMaintenanceState {
    last_cleanup_day_key: Option<u64>,
}

/// 中文：全局临时目录清理状态，确保同一天内不会重复做“跨日清理”。
/// English: Global temp-maintenance state that prevents repeated day-based cleanup within the same day.
static TEMP_MAINTENANCE_STATE: OnceLock<Mutex<TempMaintenanceState>> = OnceLock::new();

/// 中文：清理触发类型。`Startup` 表示启动时强制清理，`DayBoundary` 表示跨日后的后台清理。
/// English: Cleanup trigger type. `Startup` forces cleanup on process start, while `DayBoundary` is used by the background cross-day pass.
#[derive(Debug, Clone, Copy)]
pub enum CleanupTrigger {
    Startup,
    DayBoundary,
}

/// 中文：返回全局清理状态对象。
/// English: Return the global temp-maintenance state container.
fn maintenance_state() -> &'static Mutex<TempMaintenanceState> {
    TEMP_MAINTENANCE_STATE.get_or_init(|| Mutex::new(TempMaintenanceState::default()))
}

/// 中文：把当前时间转换成本地日期键值，用于判断是否跨日。
/// English: Convert the current time into a local-date day key used to detect day boundaries.
fn current_day_key(now: SystemTime) -> u64 {
    let local_datetime: DateTime<Local> = now.into();
    let local_date = local_datetime.date_naive();
    ((local_date.year() as i64) << 9 | local_date.ordinal0() as i64) as u64
}

/// 中文：解析运行时 temp 根目录，规则为“可执行文件目录的上级目录/temp”。
/// English: Resolve the runtime temp root. The rule is `<exe_parent_parent>/temp`.
pub fn resolve_runtime_temp_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or("runtime temp dir: executable directory not found")?;
    let runtime_root = exe_dir.parent().unwrap_or(exe_dir);
    Ok(runtime_root.join("temp"))
}

/// 中文：确保运行时 temp 目录存在，并返回该目录路径。
/// English: Ensure the runtime temp directory exists and return its path.
pub fn ensure_runtime_temp_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let temp_dir = resolve_runtime_temp_dir()?;
    fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// 中文：清理单个目录树，删除修改时间超过保留期的文件，并顺带移除空目录。
/// English: Clean a directory tree by deleting files older than the retention window and removing empty directories along the way.
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

/// 中文：执行一次临时目录清理。启动清理会强制执行；跨日清理则同一天只执行一次。
/// English: Perform one temp-directory cleanup pass. Startup cleanup is forced, while day-boundary cleanup runs at most once per day.
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

/// 中文：启动后台跨日清理任务。该任务按小时轮询，并在发现进入新的一天后执行一次清理。
/// English: Start the background cross-day cleanup task. It polls hourly and runs one cleanup pass when a new day is detected.
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
