#[cfg(not(windows))]
use crate::bootstrap::{ProcessShutdownMode, run_service_host_for_runtime_root};
use crate::config::Config;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

mod command_outcome;
mod definition;
mod manifest;
#[cfg(windows)]
mod platform;

use command_outcome::{
    CommandOutcome, capture_command_outcome, capture_command_stdout, is_launchd_not_loaded_message,
    normalize_systemd_status_value, run_command_checked,
};
use platform::{
    install_service, print_service_definition, print_service_status, restart_service,
    run_service_entrypoint, start_service, stop_service, uninstall_service,
};

mod windows;

use definition::{HostServiceManager, ServiceInstallArtifact, build_install_artifact};
use manifest::{HostServiceManifest, remove_manifest_file, write_manifest_file};

/// Default stable service name used when the caller does not override it.
/// 当调用方未显式覆盖时使用的默认稳定服务名称。
pub(crate) const DEFAULT_SERVICE_NAME: &str = "VulcanAgentService";

/// Default service description exposed to platform service managers.
/// 暴露给平台服务管理器的默认服务描述。
pub(crate) const DEFAULT_SERVICE_DESCRIPTION: &str = "Vulcan unified agent service host";

/// Cross-platform service scope used for installation and lifecycle management.
/// 安装与生命周期管理使用的跨平台服务作用域。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum ServiceScope {
    /// Install into the system-wide service manager scope.
    /// 安装到系统级服务管理作用域。
    System,
    /// Install into the current-user service manager scope when the platform supports it.
    /// 在平台支持时安装到当前用户服务管理作用域。
    User,
}

impl ServiceScope {
    /// Render one stable lowercase scope token.
    /// 渲染稳定的小写作用域标记。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

/// Startup policy requested during service installation.
/// 服务安装时请求的启动策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum ServiceStartup {
    /// Start automatically with the platform service manager.
    /// 跟随平台服务管理器自动启动。
    Auto,
    /// Require explicit manual start.
    /// 需要显式手动启动。
    Manual,
}

impl ServiceStartup {
    /// Render one stable lowercase startup token.
    /// 渲染稳定的小写启动策略标记。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

/// Installation options shared by install and print-definition commands.
/// 安装与定义预览命令共享的安装选项。
#[derive(Clone, Debug)]
pub(crate) struct ServiceInstallOptions {
    /// Optional explicit runtime root override used by service installation flows.
    /// 服务安装流程使用的可选显式运行根覆盖。
    pub(crate) runtime_root: Option<PathBuf>,
    /// Stable service name used by the platform manager.
    /// 平台服务管理器使用的稳定服务名。
    pub(crate) service_name: String,
    /// Optional display name shown by the platform manager.
    /// 平台管理器展示的可选显示名称。
    pub(crate) display_name: Option<String>,
    /// Optional description shown by the platform manager.
    /// 平台管理器展示的可选描述。
    pub(crate) description: Option<String>,
    /// Installation scope requested by the caller.
    /// 调用方请求的安装作用域。
    pub(crate) scope: ServiceScope,
    /// Startup policy requested during installation.
    /// 安装时请求的启动策略。
    pub(crate) startup: ServiceStartup,
    /// Whether the service should start immediately after installation.
    /// 安装完成后是否立即启动服务。
    pub(crate) start_immediately: bool,
    /// Whether an existing service definition may be overwritten or replaced.
    /// 是否允许覆盖或替换已有服务定义。
    pub(crate) force: bool,
}

/// Target options shared by lifecycle commands that operate on an existing service.
/// 作用于已有服务的生命周期命令共享目标选项。
#[derive(Clone, Debug)]
pub(crate) struct ServiceTargetOptions {
    /// Stable service name used by the platform manager.
    /// 平台服务管理器使用的稳定服务名。
    pub(crate) service_name: String,
    /// Target scope used by Linux/macOS service managers.
    /// Linux/macOS 服务管理器使用的目标作用域。
    pub(crate) scope: ServiceScope,
    /// Whether forceful cleanup is allowed when the manager supports it.
    /// 当平台支持时是否允许强制清理。
    pub(crate) force: bool,
}

/// Internal run options used by the platform service host entrypoint.
/// 平台服务宿主入口使用的内部运行选项。
#[derive(Clone, Debug)]
pub(crate) struct ServiceRunOptions {
    /// Optional explicit runtime root override used by the internal service host entrypoint.
    /// 内部服务宿主入口使用的可选显式运行根覆盖。
    pub(crate) runtime_root: Option<PathBuf>,
    /// Stable service name used by the platform manager.
    /// 平台服务管理器使用的稳定服务名。
    pub(crate) service_name: String,
}

/// Top-level service command variants parsed from the CLI.
/// 从 CLI 解析得到的顶层服务命令变体。
#[derive(Clone, Debug)]
pub(crate) enum ServiceCommand {
    /// Install one platform-native service definition.
    /// 安装一份平台原生服务定义。
    Install(ServiceInstallOptions),
    /// Remove one installed service definition.
    /// 卸载一份已安装的服务定义。
    Uninstall(ServiceTargetOptions),
    /// Start one installed service.
    /// 启动一个已安装服务。
    Start(ServiceTargetOptions),
    /// Stop one running service.
    /// 停止一个正在运行的服务。
    Stop(ServiceTargetOptions),
    /// Restart one installed service.
    /// 重启一个已安装服务。
    Restart(ServiceTargetOptions),
    /// Show current service status information.
    /// 显示当前服务状态信息。
    Status(ServiceTargetOptions),
    /// Run the internal service host entrypoint.
    /// 运行内部服务宿主入口。
    Run(ServiceRunOptions),
    /// Print the current platform service definition without installing it.
    /// 打印当前平台服务定义而不实际安装。
    PrintDefinition(ServiceInstallOptions),
}

/// Dispatch one parsed service command to the current platform implementation.
/// 将一条已解析的服务命令分发到当前平台实现。
pub(crate) fn run_service_command(
    command: ServiceCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ServiceCommand::Install(options) => install_service(options),
        ServiceCommand::Uninstall(options) => uninstall_service(options),
        ServiceCommand::Start(options) => start_service(options),
        ServiceCommand::Stop(options) => stop_service(options),
        ServiceCommand::Restart(options) => restart_service(options),
        ServiceCommand::Status(options) => print_service_status(options),
        ServiceCommand::Run(options) => run_service_entrypoint(options),
        ServiceCommand::PrintDefinition(options) => print_service_definition(options),
    }
}

/// Install one platform-native service definition and persist the resulting manifest.

/// Resolve the platform service manager supported by the current target OS.
/// 解析当前目标操作系统支持的平台服务管理器。
fn current_service_manager() -> Result<HostServiceManager, Box<dyn std::error::Error>> {
    if cfg!(windows) {
        return Ok(HostServiceManager::WindowsScm);
    }
    if cfg!(target_os = "linux") {
        return Ok(HostServiceManager::Systemd);
    }
    if cfg!(target_os = "macos") {
        return Ok(HostServiceManager::Launchd);
    }
    Err(format!("unsupported service platform: {}", std::env::consts::OS).into())
}

/// Reject unsupported scope combinations for platform managers with narrower capabilities.
/// 拒绝对能力更窄的平台管理器不受支持的作用域组合。
fn ensure_scope_supported(
    scope: ServiceScope,
    manager: HostServiceManager,
) -> Result<(), Box<dyn std::error::Error>> {
    if manager == HostServiceManager::WindowsScm && scope != ServiceScope::System {
        return Err("Windows service mode currently supports only system scope".into());
    }
    Ok(())
}

/// Resolve one explicit or implicit runtime root for service install and run flows.
/// 为服务安装与运行流程解析一份显式或隐式运行根。
pub(crate) fn resolve_service_runtime_root(
    runtime_root: Option<&Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(runtime_root) = runtime_root {
        return normalize_service_runtime_root_path(runtime_root);
    }
    let current_dir = std::env::current_dir()?;
    let exe_path = std::env::current_exe()?;
    let inferred_root = resolve_service_runtime_root_from_layout(&current_dir, &exe_path).ok_or(
        "failed to resolve service runtime root from the current layout; run the command from <runtime_root> or <runtime_root>/bin, or pass --runtime-root explicitly",
    )?;
    normalize_service_runtime_root_path(&inferred_root)
}

/// Normalize one service runtime root path into a stable absolute directory path.
/// 把一份服务运行根路径规范化为稳定的绝对目录路径。
pub(crate) fn normalize_service_runtime_root_path(
    path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if !absolute_path.exists() {
        return Err(format!("runtime_root does not exist: {}", absolute_path.display()).into());
    }
    if !absolute_path.is_dir() {
        return Err(format!(
            "runtime_root is not a directory: {}",
            absolute_path.display()
        )
        .into());
    }
    Ok(absolute_path.canonicalize().unwrap_or(absolute_path))
}

/// Resolve one runtime root from the current directory and executable layout used by direct startup.
/// 基于直接启动使用的当前目录与可执行文件布局解析一份运行根。
fn resolve_service_runtime_root_from_layout(
    current_dir: &Path,
    exe_path: &Path,
) -> Option<PathBuf> {
    for candidate_root in current_runtime_layout_candidates(current_dir) {
        if looks_like_service_runtime_root(&candidate_root) {
            return Some(candidate_root);
        }
    }
    let exe_dir = exe_path.parent()?;
    executable_runtime_layout_candidates(exe_dir)
        .into_iter()
        .find(|candidate| looks_like_service_runtime_root(candidate))
}

/// Enumerate current-layout runtime-root candidates in stable preference order.
/// 以稳定优先级顺序枚举当前布局下的运行根候选目录。
fn current_runtime_layout_candidates(root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(root.to_path_buf());
    if root.file_name().is_some_and(|name| name == "bin") {
        if let Some(parent_dir) = root.parent() {
            candidates.push(parent_dir.to_path_buf());
        }
    }
    candidates.push(root.join("output"));
    candidates
}

/// Enumerate executable-side runtime-root candidates in stable preference order.
/// 以稳定优先级顺序枚举可执行文件侧的运行根候选目录。
fn executable_runtime_layout_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(exe_dir.to_path_buf());
    if let Some(parent_dir) = exe_dir.parent() {
        candidates.push(parent_dir.to_path_buf());
    }
    candidates.push(exe_dir.join("output"));
    candidates
}

/// Return whether one directory looks like the hosted runtime root layout.
/// 返回某个目录是否看起来像宿主运行根布局。
fn looks_like_service_runtime_root(root: &Path) -> bool {
    let has_configs = root.join("configs").is_dir();
    let has_skills = root.join("skills").is_dir();
    has_configs || has_skills
}

/// Ensure the parent directory of one file path exists before writing.
/// 在写入之前确保某个文件路径的父目录存在。
fn ensure_parent_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent directory: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    Ok(())
}

/// Capture the current local timestamp for manifest persistence.
/// 获取当前本地时间戳用于 manifest 持久化。
fn now_local() -> DateTime<Local> {
    Local::now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Build one unique temporary directory path for service-runtime layout tests.
    /// 为服务运行根布局测试构建一个唯一临时目录路径。
    fn unique_service_test_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-agent-service-service-{name}-{timestamp}"))
    }

    /// `systemctl` inactive output should be treated as a normal textual status instead of an error.
    /// `systemctl` 的 inactive 输出应被视为正常文本状态，而不是错误。
    #[test]
    fn normalize_systemd_status_value_accepts_inactive_stdout() {
        let outcome = CommandOutcome {
            success: false,
            exit_code: Some(3),
            stdout: "inactive\n".to_string(),
            stderr: String::new(),
        };
        let normalized = normalize_systemd_status_value(&outcome)
            .expect("inactive stdout should normalize successfully");
        assert_eq!(normalized, "inactive");
    }

    /// launchd not-loaded diagnostics should be recognized as a normal non-running state.
    /// launchd 的未加载诊断文本应被识别为正常的未运行状态。
    #[test]
    fn launchd_not_loaded_message_is_recognized() {
        assert!(is_launchd_not_loaded_message(
            "Could not find service \"gui/501/com.openvulcan.vulcan-agent-service\" in domain for user gui/501"
        ));
        assert!(is_launchd_not_loaded_message("No such process"));
    }

    /// Service runtime-root inference should accept the current directory when it already looks like a hosted runtime root.
    /// 服务运行根推导在当前目录本身已具备宿主运行根布局时应直接接受当前目录。
    #[test]
    fn resolve_service_runtime_root_from_layout_accepts_current_runtime_root() {
        let runtime_root = unique_service_test_dir("current-root");
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("runtime root configs directory should be created");
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        let resolved = resolve_service_runtime_root_from_layout(&runtime_root, &fake_exe)
            .expect("current runtime root should resolve");
        assert_eq!(resolved, runtime_root);
    }

    /// Service runtime-root inference should accept a `bin/` working directory and return its hosted parent root.
    /// 服务运行根推导在工作目录位于 `bin/` 时应返回其宿主父级运行根。
    #[test]
    fn resolve_service_runtime_root_from_layout_accepts_bin_working_directory() {
        let runtime_root = unique_service_test_dir("bin-root");
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("runtime root configs directory should be created");
        let bin_dir = runtime_root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin directory should be created");
        let fake_exe = bin_dir.join("vulcan-agent-service.exe");
        let resolved = resolve_service_runtime_root_from_layout(&bin_dir, &fake_exe)
            .expect("bin working directory should resolve back to runtime root");
        assert_eq!(resolved, runtime_root);
    }

    /// Service runtime-root inference should accept an executable under `debug/` and return the hosted parent root.
    /// 服务运行根推导在可执行文件位于 `debug/` 子目录时应返回其宿主父级运行根。
    #[test]
    fn resolve_service_runtime_root_from_layout_accepts_debug_executable_directory() {
        let runtime_root = unique_service_test_dir("debug-root");
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("runtime root configs directory should be created");
        let debug_dir = runtime_root.join("debug");
        std::fs::create_dir_all(&debug_dir).expect("debug directory should be created");
        let unrelated_dir = unique_service_test_dir("debug-unrelated");
        std::fs::create_dir_all(&unrelated_dir).expect("unrelated directory should be created");
        let fake_exe = debug_dir.join("vulcan-agent-service.exe");
        let resolved = resolve_service_runtime_root_from_layout(&unrelated_dir, &fake_exe)
            .expect("debug executable directory should resolve back to runtime root");
        assert_eq!(resolved, runtime_root);
    }

    /// Service runtime-root inference should fall back to the executable-side hosted layout when the current directory is unrelated.
    /// 服务运行根推导在当前目录无关时应回退到可执行文件侧的宿主布局。
    #[test]
    fn resolve_service_runtime_root_from_layout_prefers_executable_layout() {
        let runtime_root = unique_service_test_dir("exe-root");
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("runtime root configs directory should be created");
        let unrelated_dir = unique_service_test_dir("unrelated");
        std::fs::create_dir_all(&unrelated_dir).expect("unrelated directory should be created");
        let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
        let resolved = resolve_service_runtime_root_from_layout(&unrelated_dir, &fake_exe)
            .expect("executable layout should resolve when current directory is unrelated");
        assert_eq!(resolved, runtime_root);
    }
}
