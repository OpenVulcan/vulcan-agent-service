use super::*;

/// Execute one external command and surface non-zero exits as rich errors.
/// 执行一条外部命令，并把非零退出码上抛为带上下文的错误。
pub(super) fn run_command_checked(
    program: &str,
    args: &[OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    let outcome = capture_command_outcome(program, args)?;
    if outcome.success {
        return Ok(());
    }
    Err(format!(
        "{} failed with code {:?}: {}",
        program,
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
}

/// Capture stdout from one external command and surface failures as rich errors.
/// 捕获一条外部命令的标准输出，并把失败上抛为带上下文的错误。
pub(super) fn capture_command_stdout(
    program: &str,
    args: &[OsString],
) -> Result<String, Box<dyn std::error::Error>> {
    let outcome = capture_command_outcome(program, args)?;
    if outcome.success {
        return Ok(outcome.stdout.trim().to_string());
    }
    Err(format!(
        "{} failed with code {:?}: {}",
        program,
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
}

/// Lightweight command execution snapshot used by service status normalization and error reporting.
/// 用于服务状态归一化和错误上报的轻量命令执行快照。
pub(super) struct CommandOutcome {
    /// Whether the command exited successfully.
    /// 命令是否成功退出。
    pub(super) success: bool,
    /// Optional numeric exit code returned by the process.
    /// 进程返回的可选数字退出码。
    pub(super) exit_code: Option<i32>,
    /// Captured standard output text.
    /// 捕获到的标准输出文本。
    pub(super) stdout: String,
    /// Captured standard error text.
    /// 捕获到的标准错误文本。
    pub(super) stderr: String,
}

impl CommandOutcome {
    /// Return the most useful diagnostic text from stderr first and stdout second.
    /// 优先返回标准错误，其次返回标准输出中的最有用诊断文本。
    pub(super) fn primary_diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr;
        }
        self.stdout.trim()
    }
}

/// Execute one external command and keep the full outcome for callers that need to normalize non-zero states.
/// 执行一条外部命令，并为需要归一化非零状态的调用方保留完整结果。
pub(super) fn capture_command_outcome(
    program: &str,
    args: &[OsString],
) -> Result<CommandOutcome, Box<dyn std::error::Error>> {
    let output = Command::new(program).args(args).output()?;
    Ok(CommandOutcome {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Normalize one `systemctl` query result into a stable textual status value.
/// 将单条 `systemctl` 查询结果规范化为稳定的文本状态值。
pub(super) fn normalize_systemd_status_value(
    outcome: &CommandOutcome,
) -> Result<String, Box<dyn std::error::Error>> {
    let stdout = outcome.stdout.trim();
    if !stdout.is_empty() {
        return Ok(stdout.to_string());
    }
    let stderr = outcome.stderr.trim();
    if stderr.is_empty() {
        return Ok("unknown".to_string());
    }
    let stderr_lower = stderr.to_ascii_lowercase();
    if stderr_lower.contains("could not be found")
        || stderr_lower.contains("not found")
        || stderr_lower.contains("no such file")
    {
        return Ok("not-found".to_string());
    }
    Err(format!(
        "systemctl status query failed with code {:?}: {}",
        outcome.exit_code, stderr
    )
    .into())
}

/// Return whether one launchd diagnostic text represents a normal not-loaded or not-found state.
/// 判断某条 launchd 诊断文本是否表示正常的未加载或未找到状态。
pub(super) fn is_launchd_not_loaded_message(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("could not find service")
        || lowered.contains("service could not be found")
        || lowered.contains("service not found")
        || lowered.contains("no such process")
        || lowered.contains("not loaded")
}
