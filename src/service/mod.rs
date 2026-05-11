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
pub(crate) const DEFAULT_SERVICE_NAME: &str = "vulcan-agent-service";

/// Default display name exposed to platform service managers.
/// 暴露给平台服务管理器的默认展示名称。
pub(crate) const DEFAULT_SERVICE_DISPLAY_NAME: &str = "Vulcan Agent Service";

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
    /// Unified runtime root required by service mode.
    /// 服务模式要求的统一运行根。
    pub(crate) runtime_root: PathBuf,
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
    /// Unified runtime root required by service mode.
    /// 服务模式要求的统一运行根。
    pub(crate) runtime_root: PathBuf,
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
}
