use std::sync::atomic::{AtomicBool, Ordering};

/// Controls whether non-error logs are allowed in the current process.
/// 控制当前进程是否允许输出非错误级日志。
/// `true` allows regular info/warn logs, while `false` keeps only error logs.
/// `true` 表示允许常规信息/告警日志，`false` 表示仅保留错误日志。
static NON_ERROR_LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);

/// Set whether the current process allows non-error logs.
/// 设置当前进程是否允许输出非错误级日志。
pub fn set_non_error_logging_enabled(enabled: bool) {
    NON_ERROR_LOGGING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check whether the current process allows non-error logs.
/// 判断当前进程是否允许输出非错误级日志。
pub fn non_error_logging_enabled() -> bool {
    NON_ERROR_LOGGING_ENABLED.load(Ordering::Relaxed)
}

/// Emit an informational log, or stay silent when non-error logs are disabled.
/// 输出普通信息日志；若当前模式禁用非错误日志，则直接静默。
pub fn info(message: impl AsRef<str>) {
    if non_error_logging_enabled() {
        eprintln!("{}", message.as_ref());
    }
}

/// Emit a warning log, or stay silent when the runtime keeps only errors.
/// 输出告警日志；在仅保留错误日志的模式下同样静默。
pub fn warn(message: impl AsRef<str>) {
    if non_error_logging_enabled() {
        eprintln!("{}", message.as_ref());
    }
}

/// Emit an error log. Error logs are always preserved.
/// 输出错误日志；错误日志始终保留。
pub fn error(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}
