use super::{
    DEFAULT_SERVICE_DESCRIPTION, DEFAULT_SERVICE_NAME, ServiceInstallOptions, ServiceScope,
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

/// Prepared service install artifact shared by install and print-definition flows.
/// 安装与定义预览流程共享的服务安装产物。
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
        .unwrap_or_else(|| options.service_name.clone());
    let description = Some(
        options
            .description
            .clone()
            .unwrap_or_else(|| DEFAULT_SERVICE_DESCRIPTION.to_string()),
    );
    let mut arguments = vec!["service".to_string(), "run".to_string()];
    if options.runtime_root.is_some() {
        arguments.push("--runtime-root".to_string());
        arguments.push(runtime_root.to_string_lossy().to_string());
    }
    if options.service_name != DEFAULT_SERVICE_NAME {
        arguments.push("--service-name".to_string());
        arguments.push(options.service_name.clone());
    }
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
        ServiceScope::User => Ok(launchd_user_domain(resolve_launchd_user_uid()?)),
    }
}

/// Resolve the current macOS user ID from the platform-owned identity command.
/// 通过平台拥有的身份命令解析当前 macOS 用户 ID。
/// Returns a validated unsigned user ID, or an explicit spawn, exit-status, UTF-8, or numeric error.
/// 返回已校验的无符号用户 ID，或显式的启动、退出状态、UTF-8 或数值错误。
fn resolve_launchd_user_uid() -> Result<u32, Box<dyn std::error::Error>> {
    // Use the fixed macOS system binary so PATH and mutable UID environment variables cannot select the launch domain.
    // 使用固定的 macOS 系统二进制文件，避免 PATH 与可变 UID 环境变量决定 launch 域。
    let output = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|error| format!("failed to execute `/usr/bin/id -u`: {error}"))?;
    parse_launchd_user_uid_output(
        output.status.success(),
        output.status.code(),
        &output.stdout,
        &output.stderr,
    )
    .map_err(Into::into)
}

/// Validate the raw result produced by `/usr/bin/id -u` without applying lossy identity conversion.
/// 校验 `/usr/bin/id -u` 产生的原始结果，不对身份值执行有损转换。
/// Parameters: `success` and `exit_code` describe the command status; `stdout` carries the UID and `stderr` carries diagnostics.
/// 参数：`success` 与 `exit_code` 描述命令状态；`stdout` 携带 UID，`stderr` 携带诊断。
/// Returns a strictly parsed decimal `u32`, or an error explaining the rejected command result.
/// 返回严格解析的十进制 `u32`，或说明命令结果被拒绝原因的错误。
fn parse_launchd_user_uid_output(
    success: bool,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<u32, String> {
    if !success {
        // Diagnostic text may be lossy because it never participates in identity selection.
        // 诊断文本可以有损转换，因为它不参与身份选择。
        let diagnostic = String::from_utf8_lossy(stderr);
        // Normalize an empty diagnostic into a stable explicit message.
        // 把空诊断规范化为稳定的显式消息。
        let diagnostic = diagnostic.trim();
        let diagnostic = if diagnostic.is_empty() {
            "no stderr output"
        } else {
            diagnostic
        };
        return Err(format!(
            "`/usr/bin/id -u` failed with code {exit_code:?}: {diagnostic}"
        ));
    }

    // Identity bytes must be valid UTF-8; replacement characters could select a different domain string.
    // 身份字节必须是有效 UTF-8；替换字符可能选中不同的域字符串。
    let uid_text = std::str::from_utf8(stdout)
        .map_err(|error| format!("`/usr/bin/id -u` returned invalid UTF-8: {error}"))?
        .trim();
    if uid_text.is_empty() {
        return Err("`/usr/bin/id -u` returned an empty uid".to_string());
    }
    if !uid_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "`/usr/bin/id -u` returned a non-decimal uid: {uid_text:?}"
        ));
    }
    uid_text
        .parse::<u32>()
        .map_err(|error| format!("`/usr/bin/id -u` returned an invalid uid: {error}"))
}

/// Format one validated user ID as a launchd GUI domain.
/// 把一个已校验用户 ID 格式化为 launchd GUI 域。
/// Parameters: `uid` is the typed user ID returned by the strict identity resolver.
/// 参数：`uid` 是严格身份解析器返回的类型化用户 ID。
/// Returns the stable `gui/<uid>` launchd domain string.
/// 返回稳定的 `gui/<uid>` launchd 域字符串。
fn launchd_user_domain(uid: u32) -> String {
    format!("gui/{uid}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Build one unique temporary runtime-root path for service-definition tests.
    /// 为服务定义测试构建一个唯一临时运行根路径。
    fn unique_definition_test_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vulcan-agent-service-definition-{name}-{timestamp}"
        ))
    }

    /// Default service installation should omit redundant runtime-root and service-name arguments so the hosted layout resolves like direct startup.
    /// 默认服务安装应省略冗余的运行根与服务名参数，使宿主布局像直接启动一样自行解析。
    #[test]
    fn build_install_artifact_omits_default_runtime_arguments() {
        let runtime_root = unique_definition_test_dir("default-args");
        let options = ServiceInstallOptions {
            runtime_root: None,
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            display_name: None,
            description: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            start_immediately: false,
            force: false,
        };
        let artifact = build_install_artifact(&options, runtime_root.clone())
            .expect("default artifact should build");
        assert_eq!(artifact.runtime_root, runtime_root);
        assert_eq!(artifact.display_name, DEFAULT_SERVICE_NAME);
        assert_eq!(
            artifact.arguments,
            vec!["service".to_string(), "run".to_string()]
        );
    }

    /// Custom service installation should still forward the explicit service name so SCM dispatch matches the registered service.
    /// 自定义服务安装仍应转发显式服务名，以便 SCM 分发与已注册服务保持一致。
    #[test]
    fn build_install_artifact_preserves_custom_service_name_argument() {
        let runtime_root = unique_definition_test_dir("custom-args");
        let options = ServiceInstallOptions {
            runtime_root: None,
            service_name: "CustomAgentService".to_string(),
            display_name: None,
            description: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            start_immediately: false,
            force: false,
        };
        let artifact =
            build_install_artifact(&options, runtime_root).expect("custom artifact should build");
        assert_eq!(artifact.display_name, "CustomAgentService");
        assert_eq!(
            artifact.arguments,
            vec![
                "service".to_string(),
                "run".to_string(),
                "--service-name".to_string(),
                "CustomAgentService".to_string(),
            ]
        );
    }

    /// Explicit runtime-root overrides should be preserved in the generated service command so detached executable layouts keep pointing at the selected runtime.
    /// 显式 runtime_root 覆盖应被保留在生成的服务命令中，从而让脱离运行根的可执行文件布局仍指向选定运行根。
    #[test]
    fn build_install_artifact_preserves_explicit_runtime_root_argument() {
        let runtime_root = unique_definition_test_dir("explicit-runtime-root");
        let options = ServiceInstallOptions {
            runtime_root: Some(PathBuf::from("D:/custom/runtime-root")),
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            display_name: None,
            description: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            start_immediately: false,
            force: false,
        };
        let artifact = build_install_artifact(&options, runtime_root.clone())
            .expect("explicit runtime-root artifact should build");
        assert_eq!(
            artifact.arguments,
            vec![
                "service".to_string(),
                "run".to_string(),
                "--runtime-root".to_string(),
                runtime_root.to_string_lossy().to_string(),
            ]
        );
    }

    /// Successful UID command output should accept decimal stdout and ignore non-fatal stderr diagnostics.
    /// UID 命令成功时应接受十进制 stdout，并忽略不致命的 stderr 诊断。
    #[test]
    fn parse_launchd_user_uid_output_accepts_valid_decimal_stdout() {
        // Parse the same newline-terminated shape emitted by `/usr/bin/id -u`.
        // 解析与 `/usr/bin/id -u` 输出一致的换行结尾形态。
        let uid = parse_launchd_user_uid_output(true, Some(0), b"501\n", b"notice")
            .expect("valid uid output should parse");

        assert_eq!(uid, 501);
    }

    /// UID parsing should reject empty, signed, non-decimal, overflowing, and invalid UTF-8 identity bytes.
    /// UID 解析应拒绝空值、带符号值、非十进制值、溢出值与非法 UTF-8 身份字节。
    #[test]
    fn parse_launchd_user_uid_output_rejects_invalid_identity_values() {
        // Cover every value class that the former string-only check accepted or converted lossily.
        // 覆盖旧字符串检查会接受或有损转换的每类值。
        let invalid_outputs = [
            Vec::new(),
            b"   \n".to_vec(),
            b"-1\n".to_vec(),
            b"+501\n".to_vec(),
            b"abc\n".to_vec(),
            b"4294967296\n".to_vec(),
            vec![0xff, b'\n'],
        ];

        for stdout in invalid_outputs {
            // Validate each raw byte sequence through the production parser.
            // 通过生产解析器校验每个原始字节序列。
            let error = parse_launchd_user_uid_output(true, Some(0), &stdout, b"")
                .expect_err("invalid uid output should be rejected");

            assert!(!error.is_empty());
        }
    }

    /// A failed identity command should be rejected even when stdout contains a plausible UID.
    /// 即使 stdout 包含看似合法的 UID，身份命令失败也必须被拒绝。
    #[test]
    fn parse_launchd_user_uid_output_rejects_nonzero_exit() {
        // Preserve exit-code and stderr evidence in the returned diagnostic.
        // 在返回诊断中保留退出码与 stderr 证据。
        let error = parse_launchd_user_uid_output(false, Some(7), b"501\n", b"identity failed")
            .expect_err("nonzero identity command should fail");

        assert!(error.contains("Some(7)"));
        assert!(error.contains("identity failed"));
    }

    /// Launchd domain formatting should keep the system token independent and use only typed user IDs for GUI domains.
    /// launchd 域格式化应保持 system 标记独立，并仅用类型化用户 ID 生成 GUI 域。
    #[test]
    fn launchd_domain_formats_system_and_typed_user_domains() {
        // System scope must not execute the user identity command on any platform.
        // System 作用域在任何平台都不应执行用户身份命令。
        let system_domain = launchd_domain(ServiceScope::System)
            .expect("system launchd domain should resolve without uid lookup");

        assert_eq!(system_domain, "system");
        assert_eq!(launchd_user_domain(501), "gui/501");
    }
}
