use super::{
    DEFAULT_SERVICE_DESCRIPTION, DEFAULT_SERVICE_DISPLAY_NAME, ServiceInstallOptions, ServiceScope,
    ServiceStartup,
};
use std::path::{Path, PathBuf};

/// Platform-native service manager kinds supported by the host.
/// 宿主支持的平台原生服务管理器种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostServiceManager {
    /// Windows Service Control Manager.
    /// Windows 服务控制管理器。
    WindowsScm,
    /// Linux systemd manager.
    /// Linux systemd 管理器。
    Systemd,
    /// macOS launchd manager.
    /// macOS launchd 管理器。
    Launchd,
}

impl HostServiceManager {
    /// Render one stable lowercase manager token.
    /// 渲染稳定的小写管理器标记。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::WindowsScm => "windows-scm",
            Self::Systemd => "systemd",
            Self::Launchd => "launchd",
        }
    }
}

/// Prepared service install artifact shared by install, manifest, and print-definition flows.
/// 安装、manifest 与定义预览流程共享的服务安装产物。
#[derive(Clone, Debug)]
pub(crate) struct ServiceInstallArtifact {
    /// Current platform manager used by the artifact.
    /// 当前产物使用的平台管理器。
    pub(crate) manager: HostServiceManager,
    /// Stable service name used by the platform manager.
    /// 平台管理器使用的稳定服务名。
    pub(crate) service_name: String,
    /// Stable display name shown to the operator.
    /// 展示给运维人员的稳定显示名称。
    pub(crate) display_name: String,
    /// Optional human-readable description.
    /// 可选的人类可读描述。
    pub(crate) description: Option<String>,
    /// Requested installation scope.
    /// 请求的安装作用域。
    pub(crate) scope: ServiceScope,
    /// Requested startup policy.
    /// 请求的启动策略。
    pub(crate) startup: ServiceStartup,
    /// Unified runtime root required by service mode.
    /// 服务模式要求的统一运行根。
    pub(crate) runtime_root: PathBuf,
    /// Host executable path used by the service definition.
    /// 服务定义使用的宿主可执行文件路径。
    pub(crate) executable_path: PathBuf,
    /// Working directory expected by the service process.
    /// 服务进程期望使用的工作目录。
    pub(crate) working_directory: PathBuf,
    /// Full service command arguments excluding the executable path.
    /// 不含可执行文件路径的完整服务命令参数。
    pub(crate) arguments: Vec<String>,
    /// Optional on-disk service definition path.
    /// 可选的磁盘服务定义路径。
    pub(crate) definition_path: Option<PathBuf>,
    /// Optional rendered definition content for file-based managers.
    /// 文件型管理器使用的可选已渲染定义内容。
    pub(crate) rendered_definition: Option<String>,
    /// Optional platform label or unit name separate from service_name.
    /// 独立于 service_name 的可选平台标签或单元名。
    pub(crate) label_or_unit_name: Option<String>,
    /// Optional stdout log path controlled by the platform manager.
    /// 平台管理器控制的可选标准输出日志路径。
    pub(crate) stdout_log_path: Option<PathBuf>,
    /// Optional stderr log path controlled by the platform manager.
    /// 平台管理器控制的可选标准错误日志路径。
    pub(crate) stderr_log_path: Option<PathBuf>,
}

impl ServiceInstallArtifact {
    /// Render one operator-facing summary of the current service definition.
    /// 渲染当前服务定义的面向运维人员的摘要。
    pub(crate) fn render_for_cli(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("manager: {}", self.manager.as_str()));
        lines.push(format!("service_name: {}", self.service_name));
        lines.push(format!("display_name: {}", self.display_name));
        if let Some(description) = self.description.as_ref() {
            lines.push(format!("description: {}", description));
        }
        lines.push(format!("scope: {}", self.scope.as_str()));
        lines.push(format!("startup: {}", self.startup.as_str()));
        lines.push(format!("runtime_root: {}", self.runtime_root.display()));
        lines.push(format!(
            "working_directory: {}",
            self.working_directory.display()
        ));
        lines.push(format!("executable: {}", self.executable_path.display()));
        lines.push(format!("command: {}", self.render_launch_command_for_cli()));
        if let Some(label) = self.label_or_unit_name.as_ref() {
            lines.push(format!("label_or_unit_name: {}", label));
        }
        if let Some(definition_path) = self.definition_path.as_ref() {
            lines.push(format!("definition_path: {}", definition_path.display()));
        }
        if let Some(stdout_path) = self.stdout_log_path.as_ref() {
            lines.push(format!("stdout_log_path: {}", stdout_path.display()));
        }
        if let Some(stderr_path) = self.stderr_log_path.as_ref() {
            lines.push(format!("stderr_log_path: {}", stderr_path.display()));
        }
        if let Some(rendered_definition) = self.rendered_definition.as_ref() {
            lines.push(String::new());
            lines.push("definition:".to_string());
            lines.push(rendered_definition.clone());
        }
        lines.join("\n")
    }

    /// Render one Windows-compatible `binPath=` command-line payload.
    /// 渲染一份兼容 Windows `binPath=` 的命令行载荷。
    pub(crate) fn windows_bin_path_argument(&self) -> String {
        let mut segments = Vec::with_capacity(self.arguments.len() + 1);
        segments.push(quote_windows_arg(
            self.executable_path.to_string_lossy().as_ref(),
        ));
        segments.extend(self.arguments.iter().map(|value| quote_windows_arg(value)));
        segments.join(" ")
    }

    /// Render one human-readable launch command line for docs and previews.
    /// 渲染一份用于文档与预览的人类可读启动命令行。
    pub(crate) fn render_launch_command_for_cli(&self) -> String {
        let mut segments = Vec::with_capacity(self.arguments.len() + 1);
        segments.push(quote_shell_arg(
            self.executable_path.to_string_lossy().as_ref(),
        ));
        segments.extend(self.arguments.iter().map(|value| quote_shell_arg(value)));
        segments.join(" ")
    }
}

/// Build one install artifact for the current target platform.
/// 为当前目标平台构建一份安装产物。
pub(crate) fn build_install_artifact(
    options: &ServiceInstallOptions,
    runtime_root: PathBuf,
) -> Result<ServiceInstallArtifact, Box<dyn std::error::Error>> {
    let executable_path = std::env::current_exe()?;
    let display_name = options
        .display_name
        .clone()
        .unwrap_or_else(|| DEFAULT_SERVICE_DISPLAY_NAME.to_string());
    let description = Some(
        options
            .description
            .clone()
            .unwrap_or_else(|| DEFAULT_SERVICE_DESCRIPTION.to_string()),
    );
    let arguments = vec![
        "service".to_string(),
        "run".to_string(),
        "--runtime-root".to_string(),
        runtime_root.to_string_lossy().to_string(),
        "--service-name".to_string(),
        options.service_name.clone(),
    ];
    let manager = current_target_manager()?;
    let working_directory = runtime_root.clone();
    let logs_root = runtime_root.join("logs");
    match manager {
        HostServiceManager::WindowsScm => Ok(ServiceInstallArtifact {
            manager,
            service_name: options.service_name.clone(),
            display_name,
            description,
            scope: options.scope,
            startup: options.startup,
            runtime_root,
            executable_path,
            working_directory,
            arguments,
            definition_path: None,
            rendered_definition: None,
            label_or_unit_name: None,
            stdout_log_path: None,
            stderr_log_path: Some(logs_root.join("service.stderr.log")),
        }),
        HostServiceManager::Systemd => {
            let unit_name = format!("{}.service", options.service_name);
            let definition_path = systemd_unit_path(&options.service_name, options.scope)?;
            let rendered_definition = render_systemd_definition(
                &display_name,
                description.as_deref(),
                &executable_path,
                &arguments,
                &working_directory,
                options.startup,
            );
            Ok(ServiceInstallArtifact {
                manager,
                service_name: options.service_name.clone(),
                display_name,
                description,
                scope: options.scope,
                startup: options.startup,
                runtime_root,
                executable_path,
                working_directory,
                arguments,
                definition_path: Some(definition_path),
                rendered_definition: Some(rendered_definition),
                label_or_unit_name: Some(unit_name),
                stdout_log_path: None,
                stderr_log_path: None,
            })
        }
        HostServiceManager::Launchd => {
            let label = launchd_label(&options.service_name);
            let stdout_log_path = logs_root.join("service.stdout.log");
            let stderr_log_path = logs_root.join("service.stderr.log");
            let definition_path = launchd_plist_path(&options.service_name, options.scope)?;
            let rendered_definition = render_launchd_definition(
                &label,
                &executable_path,
                &arguments,
                &working_directory,
                &stdout_log_path,
                &stderr_log_path,
                options.startup,
            );
            Ok(ServiceInstallArtifact {
                manager,
                service_name: options.service_name.clone(),
                display_name,
                description,
                scope: options.scope,
                startup: options.startup,
                runtime_root,
                executable_path,
                working_directory,
                arguments,
                definition_path: Some(definition_path),
                rendered_definition: Some(rendered_definition),
                label_or_unit_name: Some(label),
                stdout_log_path: Some(stdout_log_path),
                stderr_log_path: Some(stderr_log_path),
            })
        }
    }
}

/// Resolve one current-target service manager from compile-time OS metadata.
/// 根据编译期操作系统元数据解析当前目标服务管理器。
fn current_target_manager() -> Result<HostServiceManager, Box<dyn std::error::Error>> {
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

/// Render one systemd unit definition.
/// 渲染一份 systemd unit 定义。
fn render_systemd_definition(
    display_name: &str,
    description: Option<&str>,
    executable_path: &Path,
    arguments: &[String],
    working_directory: &Path,
    startup: ServiceStartup,
) -> String {
    let description_line = description.unwrap_or(display_name);
    let exec_start = render_exec_start(executable_path, arguments);
    let wanted_by = if startup == ServiceStartup::Auto {
        "multi-user.target"
    } else {
        "default.target"
    };
    format!(
        "[Unit]\nDescription={description_line}\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={exec_start}\nWorkingDirectory={working_directory}\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy={wanted_by}\n",
        working_directory = working_directory.display()
    )
}

/// Render one launchd plist definition.
/// 渲染一份 launchd plist 定义。
fn render_launchd_definition(
    label: &str,
    executable_path: &Path,
    arguments: &[String],
    working_directory: &Path,
    stdout_log_path: &Path,
    stderr_log_path: &Path,
    startup: ServiceStartup,
) -> String {
    let mut program_arguments = String::new();
    program_arguments.push_str("    <array>\n");
    program_arguments.push_str(&format!(
        "      <string>{}</string>\n",
        xml_escape(executable_path.to_string_lossy().as_ref())
    ));
    for argument in arguments {
        program_arguments.push_str(&format!(
            "      <string>{}</string>\n",
            xml_escape(argument)
        ));
    }
    program_arguments.push_str("    </array>");
    let keep_alive = if startup == ServiceStartup::Auto {
        "<true/>"
    } else {
        "<false/>"
    };
    let run_at_load = if startup == ServiceStartup::Auto {
        "<true/>"
    } else {
        "<false/>"
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n  <dict>\n    <key>Label</key>\n    <string>{label}</string>\n    <key>ProgramArguments</key>\n{program_arguments}\n    <key>WorkingDirectory</key>\n    <string>{working_directory}</string>\n    <key>RunAtLoad</key>\n    {run_at_load}\n    <key>KeepAlive</key>\n    {keep_alive}\n    <key>StandardOutPath</key>\n    <string>{stdout_log_path}</string>\n    <key>StandardErrorPath</key>\n    <string>{stderr_log_path}</string>\n  </dict>\n</plist>\n",
        label = xml_escape(label),
        working_directory = xml_escape(working_directory.to_string_lossy().as_ref()),
        stdout_log_path = xml_escape(stdout_log_path.to_string_lossy().as_ref()),
        stderr_log_path = xml_escape(stderr_log_path.to_string_lossy().as_ref()),
    )
}

/// Resolve the systemd unit path for the selected scope.
/// 解析选定作用域下的 systemd unit 路径。
pub(crate) fn systemd_unit_path(
    service_name: &str,
    scope: ServiceScope,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_name = format!("{}.service", service_name);
    match scope {
        ServiceScope::System => Ok(PathBuf::from("/etc/systemd/system").join(file_name)),
        ServiceScope::User => Ok(resolve_home_dir()?
            .join(".config")
            .join("systemd")
            .join("user")
            .join(file_name)),
    }
}

/// Resolve the launchd plist path for the selected scope.
/// 解析选定作用域下的 launchd plist 路径。
pub(crate) fn launchd_plist_path(
    service_name: &str,
    scope: ServiceScope,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_name = format!("{}.plist", service_name);
    match scope {
        ServiceScope::System => Ok(PathBuf::from("/Library/LaunchDaemons").join(file_name)),
        ServiceScope::User => Ok(resolve_home_dir()?
            .join("Library")
            .join("LaunchAgents")
            .join(file_name)),
    }
}

/// Resolve the launchd label used by the service definition.
/// 解析服务定义使用的 launchd 标签。
pub(crate) fn launchd_label(service_name: &str) -> String {
    format!("com.openvulcan.{}", service_name)
}

/// Resolve the launchd domain token for the selected scope.
/// 解析选定作用域使用的 launchd 域标记。
pub(crate) fn launchd_domain(scope: ServiceScope) -> Result<String, Box<dyn std::error::Error>> {
    match scope {
        ServiceScope::System => Ok("system".to_string()),
        ServiceScope::User => {
            let uid = if let Ok(uid) = std::env::var("UID") {
                uid
            } else {
                let output = std::process::Command::new("id").arg("-u").output()?;
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            };
            if uid.is_empty() {
                return Err("failed to resolve uid for launchd user domain".into());
            }
            Ok(format!("gui/{}", uid))
        }
    }
}

/// Quote one command argument for human-readable shell previews.
/// 为人类可读的 shell 预览引用一个命令参数。
fn quote_shell_arg(value: &str) -> String {
    if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | ':' | '\\')
    }) {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\\\""))
}

/// Quote one command argument for Windows service `binPath=` payloads.
/// 为 Windows 服务 `binPath=` 载荷引用一个命令参数。
fn quote_windows_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

/// Render one executable path plus argument vector into a systemd ExecStart value.
/// 将可执行文件路径和参数向量渲染为 systemd ExecStart 值。
fn render_exec_start(executable_path: &Path, arguments: &[String]) -> String {
    let mut parts = Vec::with_capacity(arguments.len() + 1);
    parts.push(quote_shell_arg(executable_path.to_string_lossy().as_ref()));
    parts.extend(arguments.iter().map(|value| quote_shell_arg(value)));
    parts.join(" ")
}

/// Resolve the current home directory for user-scoped definition paths.
/// 为用户级定义路径解析当前家目录。
fn resolve_home_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            "failed to resolve home directory for user-scoped service installation".into()
        })
}

/// Escape XML-special characters for launchd plist values.
/// 为 launchd plist 值转义 XML 特殊字符。
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
