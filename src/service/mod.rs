#[cfg(not(windows))]
use crate::bootstrap::{ProcessShutdownMode, run_service_host_for_runtime_root};
use crate::config::Config;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

mod definition;
mod manifest;
#[cfg(windows)]
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
/// 安装一份平台原生服务定义，并持久化对应 manifest。
fn install_service(options: ServiceInstallOptions) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = validate_runtime_root(&options.runtime_root)?;
    let artifact = build_install_artifact(&options, normalized_runtime_root.clone())?;
    prepare_runtime_service_directories(&normalized_runtime_root)?;
    preflight_runtime_config(&normalized_runtime_root)?;
    if options.force {
        let _ = remove_existing_service_if_possible(&artifact);
    }
    apply_install_artifact(&artifact)?;
    let manifest = HostServiceManifest::from_artifact(&artifact, now_local());
    write_manifest_file(&manifest)?;
    if options.start_immediately {
        start_service(ServiceTargetOptions {
            service_name: artifact.service_name.clone(),
            scope: artifact.scope,
            force: options.force,
        })?;
    }
    println!(
        "Service installed: {} ({})",
        artifact.service_name,
        artifact.manager.as_str()
    );
    if let Some(definition_path) = &artifact.definition_path {
        println!("Definition path: {}", definition_path.display());
    }
    println!("Runtime root: {}", normalized_runtime_root.display());
    Ok(())
}

/// Remove one installed service definition and delete the persisted manifest when present.
/// 卸载一份已安装的服务定义，并在存在时删除持久化 manifest。
fn uninstall_service(options: ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = HostServiceManifest::load_best_effort(&options.service_name, options.scope)?;
    if options.force {
        let _ = stop_service(options.clone());
    }
    uninstall_platform_service(&options)?;
    if let Some(existing_manifest) = manifest.as_ref() {
        remove_manifest_file(
            &existing_manifest.runtime_root,
            &existing_manifest.service_name,
        )?;
    }
    println!("Service uninstalled: {}", options.service_name);
    Ok(())
}

/// Start one installed service through the current platform manager.
/// 通过当前平台管理器启动一个已安装服务。
fn start_service(options: ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => {
            run_windows_service_action("start", &options.service_name)
        }
        HostServiceManager::Systemd => run_systemd_action("start", &options),
        HostServiceManager::Launchd => run_launchd_start(&options),
    }?;
    println!("Service started: {}", options.service_name);
    Ok(())
}

/// Stop one running service through the current platform manager.
/// 通过当前平台管理器停止一个正在运行的服务。
fn stop_service(options: ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => run_windows_service_action("stop", &options.service_name),
        HostServiceManager::Systemd => run_systemd_action("stop", &options),
        HostServiceManager::Launchd => run_launchd_stop(&options),
    }?;
    println!("Service stopped: {}", options.service_name);
    Ok(())
}

/// Restart one installed service through the current platform manager.
/// 通过当前平台管理器重启一个已安装服务。
fn restart_service(options: ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => {
            let _ = run_windows_service_action("stop", &options.service_name);
            run_windows_service_action("start", &options.service_name)?;
        }
        HostServiceManager::Systemd => run_systemd_action("restart", &options)?,
        HostServiceManager::Launchd => run_launchd_restart(&options)?,
    }
    println!("Service restarted: {}", options.service_name);
    Ok(())
}

/// Print current service status information from the active platform manager.
/// 从当前平台管理器打印当前服务状态信息。
fn print_service_status(options: ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    let status_text = match current_service_manager()? {
        HostServiceManager::WindowsScm => capture_windows_service_status(&options.service_name)?,
        HostServiceManager::Systemd => capture_systemd_status(&options)?,
        HostServiceManager::Launchd => capture_launchd_status(&options)?,
    };
    println!("{}", status_text);
    Ok(())
}

/// Run the internal service host entrypoint for the current platform.
/// 运行当前平台的内部服务宿主入口。
fn run_service_entrypoint(options: ServiceRunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = validate_runtime_root(&options.runtime_root)?;
    #[cfg(windows)]
    {
        if let Err(error) = windows::run_windows_service_dispatcher(ServiceRunOptions {
            runtime_root: normalized_runtime_root.clone(),
            service_name: options.service_name.clone(),
        }) {
            return Err(error);
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        run_service_host_for_runtime_root(
            &normalized_runtime_root,
            ProcessShutdownMode::ProcessSignals,
        )
    }
}

/// Print the current platform service definition without modifying the system.
/// 在不修改系统的前提下打印当前平台服务定义。
fn print_service_definition(
    options: ServiceInstallOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = validate_runtime_root(&options.runtime_root)?;
    let artifact = build_install_artifact(&options, normalized_runtime_root)?;
    println!("{}", artifact.render_for_cli());
    Ok(())
}

/// Validate one runtime root and normalize it into a stable absolute path.
/// 校验一个运行根，并将其规范化为稳定的绝对路径。
fn validate_runtime_root(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
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

/// Prepare shared service state and log directories under the runtime root.
/// 在运行根下准备共享服务状态目录与日志目录。
fn prepare_runtime_service_directories(
    runtime_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(runtime_root.join("logs"))?;
    std::fs::create_dir_all(runtime_root.join("state").join("service"))?;
    Ok(())
}

/// Preflight the runtime config so service installation fails before writing platform state.
/// 预检运行时配置，使服务安装在写入平台状态之前就能失败。
fn preflight_runtime_config(runtime_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = runtime_root.join("configs").join("config.yaml");
    if !config_path.exists() {
        return Err(format!("config file not found: {}", config_path.display()).into());
    }
    let mut config = Config::from_file(&config_path.to_string_lossy())?;
    config.runtime_root = Some(runtime_root.to_string_lossy().to_string());
    if config.skill_roots.is_none() {
        eprintln!(
            "[Service] Warning: service mode is using implicit skill_roots; consider configuring explicit ROOT/USER layers under the runtime root."
        );
    }
    Ok(())
}

/// Best-effort removal used by force-reinstall flows before applying new definitions.
/// force 重装流程在写入新定义前尝试执行的尽力清理。
fn remove_existing_service_if_possible(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = ServiceTargetOptions {
        service_name: artifact.service_name.clone(),
        scope: artifact.scope,
        force: true,
    };
    let _ = uninstall_service(target);
    Ok(())
}

/// Apply one prepared install artifact to the current platform service manager.
/// 将一份已准备好的安装产物应用到当前平台服务管理器。
fn apply_install_artifact(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    match artifact.manager {
        HostServiceManager::WindowsScm => install_windows_service(artifact),
        HostServiceManager::Systemd => install_systemd_service(artifact),
        HostServiceManager::Launchd => install_launchd_service(artifact),
    }
}

/// Remove one installed service from the current platform manager.
/// 从当前平台管理器中移除一个已安装服务。
fn uninstall_platform_service(
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => {
            run_windows_service_action("delete", &options.service_name)
        }
        HostServiceManager::Systemd => uninstall_systemd_service(options),
        HostServiceManager::Launchd => uninstall_launchd_service(options),
    }
}

/// Install a Windows SCM service definition by invoking the native service manager.
/// 通过调用原生服务管理器安装 Windows SCM 服务定义。
fn install_windows_service(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_scope_supported(artifact.scope, HostServiceManager::WindowsScm)?;
    let start_mode = match artifact.startup {
        ServiceStartup::Auto => "auto",
        ServiceStartup::Manual => "demand",
    };
    let mut create_args = vec![
        OsString::from("create"),
        OsString::from(artifact.service_name.clone()),
        OsString::from(format!("binPath= {}", artifact.windows_bin_path_argument())),
        OsString::from(format!("start= {}", start_mode)),
        OsString::from(format!("DisplayName= {}", artifact.display_name)),
    ];
    run_command_checked("sc.exe", &create_args)?;
    if let Some(description) = artifact.description.as_ref() {
        create_args.clear();
        run_command_checked(
            "sc.exe",
            &[
                OsString::from("description"),
                OsString::from(artifact.service_name.clone()),
                OsString::from(description.clone()),
            ],
        )?;
    }
    Ok(())
}

/// Install one systemd unit into the selected scope and reload the manager state.
/// 将一份 systemd unit 安装到选定作用域，并重新加载管理器状态。
fn install_systemd_service(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    let definition_path = artifact
        .definition_path
        .as_ref()
        .ok_or("systemd install requires a definition path")?;
    let rendered_definition = artifact
        .rendered_definition
        .as_ref()
        .ok_or("systemd install requires rendered definition text")?;
    ensure_parent_directory(definition_path)?;
    std::fs::write(definition_path, rendered_definition)?;
    run_systemd_manager_command(artifact.scope, ["daemon-reload"])?;
    if artifact.startup == ServiceStartup::Auto {
        run_systemd_manager_command(artifact.scope, ["enable", artifact.service_name.as_str()])?;
    }
    Ok(())
}

/// Remove one installed systemd unit and reload the manager state.
/// 移除一份已安装的 systemd unit，并重新加载管理器状态。
fn uninstall_systemd_service(
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let unit_path = definition::systemd_unit_path(&options.service_name, options.scope)?;
    let _ = run_systemd_manager_command(options.scope, ["disable", options.service_name.as_str()]);
    let _ = run_systemd_manager_command(options.scope, ["stop", options.service_name.as_str()]);
    if unit_path.exists() {
        std::fs::remove_file(unit_path)?;
    }
    run_systemd_manager_command(options.scope, ["daemon-reload"])?;
    Ok(())
}

/// Install one launchd plist into the selected scope and optionally bootstrap it later.
/// 将一份 launchd plist 安装到选定作用域，并在之后按需引导加载。
fn install_launchd_service(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    let definition_path = artifact
        .definition_path
        .as_ref()
        .ok_or("launchd install requires a definition path")?;
    let rendered_definition = artifact
        .rendered_definition
        .as_ref()
        .ok_or("launchd install requires rendered definition text")?;
    ensure_parent_directory(definition_path)?;
    std::fs::write(definition_path, rendered_definition)?;
    if artifact.startup == ServiceStartup::Auto {
        run_launchctl_bootstrap(definition_path, artifact.scope)?;
    }
    Ok(())
}

/// Remove one installed launchd plist and boot it out when still loaded.
/// 移除一份已安装的 launchd plist，并在仍已加载时执行卸载。
fn uninstall_launchd_service(
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let plist_path = definition::launchd_plist_path(&options.service_name, options.scope)?;
    let _ = run_launchctl_bootout(&options.service_name, options.scope);
    if plist_path.exists() {
        std::fs::remove_file(plist_path)?;
    }
    Ok(())
}

/// Run a direct Windows service lifecycle action through `sc.exe`.
/// 通过 `sc.exe` 执行直接的 Windows 服务生命周期动作。
fn run_windows_service_action(
    action: &str,
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    run_command_checked(
        "sc.exe",
        &[OsString::from(action), OsString::from(service_name)],
    )?;
    Ok(())
}

/// Capture Windows service status output through `sc.exe query`.
/// 通过 `sc.exe query` 捕获 Windows 服务状态输出。
fn capture_windows_service_status(
    service_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    capture_command_stdout(
        "sc.exe",
        &[OsString::from("query"), OsString::from(service_name)],
    )
}

/// Run one systemd lifecycle action through `systemctl`.
/// 通过 `systemctl` 执行一条 systemd 生命周期动作。
fn run_systemd_action(
    action: &str,
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    run_systemd_manager_command(options.scope, [action, options.service_name.as_str()])?;
    Ok(())
}

/// Capture systemd status output in a compact text form.
/// 以紧凑文本形式捕获 systemd 状态输出。
fn capture_systemd_status(
    options: &ServiceTargetOptions,
) -> Result<String, Box<dyn std::error::Error>> {
    let active_outcome = capture_systemd_manager_outcome(
        options.scope,
        ["is-active", options.service_name.as_str()],
    )?;
    let enabled_outcome = capture_systemd_manager_outcome(
        options.scope,
        ["is-enabled", options.service_name.as_str()],
    )?;
    let active = normalize_systemd_status_value(&active_outcome)?;
    let enabled = normalize_systemd_status_value(&enabled_outcome)?;
    Ok(format!(
        "manager: systemd\nservice: {}\nactive: {}\nenabled: {}",
        options.service_name, active, enabled
    ))
}

/// Start one launchd job by bootstrapping its plist into the selected domain.
/// 通过按需加载再显式拉起来启动一个 launchd 作业。
fn run_launchd_start(options: &ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    ensure_launchd_job_loaded(options)?;
    let domain = definition::launchd_domain(options.scope)?;
    let label = definition::launchd_label(&options.service_name);
    run_launchctl_kickstart(&domain, &label, false)
}

/// Restart one launchd job by ensuring it is loaded and then forcing a kickstart.
/// 通过确保作业已加载并执行强制 kickstart 来重启一个 launchd 作业。
fn run_launchd_restart(options: &ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    ensure_launchd_job_loaded(options)?;
    let domain = definition::launchd_domain(options.scope)?;
    let label = definition::launchd_label(&options.service_name);
    run_launchctl_kickstart(&domain, &label, true)
}

/// Ensure one launchd job has been bootstrapped into the selected domain before lifecycle actions.
/// 在执行生命周期动作前，确保某个 launchd 作业已经被引导到选定域中。
fn ensure_launchd_job_loaded(
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let domain = definition::launchd_domain(options.scope)?;
    let label = definition::launchd_label(&options.service_name);
    if is_launchd_job_loaded(&domain, &label)? {
        return Ok(());
    }
    let plist_path = definition::launchd_plist_path(&options.service_name, options.scope)?;
    run_launchctl_bootstrap(&plist_path, options.scope)
}

/// Stop one launchd job by booting it out of the selected domain.
/// 通过把作业从选定域移除来停止一个 launchd 作业。
fn run_launchd_stop(options: &ServiceTargetOptions) -> Result<(), Box<dyn std::error::Error>> {
    run_launchctl_bootout(&options.service_name, options.scope)
}

/// Capture launchd status output from the selected launch domain.
/// 从选定 launch 域捕获 launchd 状态输出。
fn capture_launchd_status(
    options: &ServiceTargetOptions,
) -> Result<String, Box<dyn std::error::Error>> {
    let label = definition::launchd_label(&options.service_name);
    let domain = definition::launchd_domain(options.scope)?;
    let outcome = capture_launchd_print_outcome(&domain, &label)?;
    if outcome.success {
        return Ok(outcome.stdout.trim().to_string());
    }
    let diagnostic_text = outcome.primary_diagnostic();
    if is_launchd_not_loaded_message(diagnostic_text) {
        return Ok(format!(
            "manager: launchd\nservice: {}\nloaded: false\nstatus: not-loaded",
            options.service_name
        ));
    }
    Err(format!(
        "launchctl print failed with code {:?}: {}",
        outcome.exit_code, diagnostic_text
    )
    .into())
}

/// Execute one `systemctl` command in either system or user scope.
/// 在 system 或 user 作用域下执行一条 `systemctl` 命令。
fn run_systemd_manager_command<const N: usize>(
    scope: ServiceScope,
    tail: [&str; N],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Vec::new();
    if scope == ServiceScope::User {
        args.push(OsString::from("--user"));
    }
    args.extend(tail.into_iter().map(OsString::from));
    run_command_checked("systemctl", &args)?;
    Ok(())
}

/// Capture one `systemctl` command outcome in either system or user scope without interpreting non-zero exits as fatal.
/// 在 system 或 user 作用域下捕获一条 `systemctl` 命令的执行结果，并且不把非零退出直接视为致命错误。
fn capture_systemd_manager_outcome<const N: usize>(
    scope: ServiceScope,
    tail: [&str; N],
) -> Result<CommandOutcome, Box<dyn std::error::Error>> {
    let mut args = Vec::new();
    if scope == ServiceScope::User {
        args.push(OsString::from("--user"));
    }
    args.extend(tail.into_iter().map(OsString::from));
    capture_command_outcome("systemctl", &args)
}

/// Bootstrap one launchd plist into the selected launch domain.
/// 将一个 launchd plist 引导到选定的 launch 域。
fn run_launchctl_bootstrap(
    plist_path: &Path,
    scope: ServiceScope,
) -> Result<(), Box<dyn std::error::Error>> {
    let domain = definition::launchd_domain(scope)?;
    run_command_checked(
        "launchctl",
        &[
            OsString::from("bootstrap"),
            OsString::from(domain),
            plist_path.as_os_str().to_os_string(),
        ],
    )?;
    Ok(())
}

/// Force launchd to start or restart one loaded job through `kickstart`.
/// 通过 `kickstart` 强制 launchd 启动或重启一个已加载作业。
fn run_launchctl_kickstart(
    domain: &str,
    label: &str,
    kill_existing: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = vec![OsString::from("kickstart")];
    if kill_existing {
        args.push(OsString::from("-k"));
    }
    args.push(OsString::from(format!("{}/{}", domain, label)));
    run_command_checked("launchctl", &args)?;
    Ok(())
}

/// Boot one launchd job out of the selected launch domain.
/// 将一个 launchd 作业从选定的 launch 域移除。
fn run_launchctl_bootout(
    service_name: &str,
    scope: ServiceScope,
) -> Result<(), Box<dyn std::error::Error>> {
    let domain = definition::launchd_domain(scope)?;
    let label = definition::launchd_label(service_name);
    run_command_checked(
        "launchctl",
        &[
            OsString::from("bootout"),
            OsString::from(format!("{}/{}", domain, label)),
        ],
    )?;
    Ok(())
}

/// Capture the raw `launchctl print` outcome for one domain-qualified job label.
/// 捕获某个按域限定的作业标签对应的原始 `launchctl print` 结果。
fn capture_launchd_print_outcome(
    domain: &str,
    label: &str,
) -> Result<CommandOutcome, Box<dyn std::error::Error>> {
    capture_command_outcome(
        "launchctl",
        &[
            OsString::from("print"),
            OsString::from(format!("{}/{}", domain, label)),
        ],
    )
}

/// Return whether one launchd job is currently loaded into its target domain.
/// 判断某个 launchd 作业当前是否已经加载到目标域中。
fn is_launchd_job_loaded(domain: &str, label: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let outcome = capture_launchd_print_outcome(domain, label)?;
    if outcome.success {
        return Ok(true);
    }
    if is_launchd_not_loaded_message(outcome.primary_diagnostic()) {
        return Ok(false);
    }
    Err(format!(
        "launchctl print failed with code {:?}: {}",
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
}

/// Execute one external command and surface non-zero exits as rich errors.
/// 执行一条外部命令，并把非零退出码上抛为带上下文的错误。
fn run_command_checked(program: &str, args: &[OsString]) -> Result<(), Box<dyn std::error::Error>> {
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
fn capture_command_stdout(
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
struct CommandOutcome {
    /// Whether the command exited successfully.
    /// 命令是否成功退出。
    success: bool,
    /// Optional numeric exit code returned by the process.
    /// 进程返回的可选数字退出码。
    exit_code: Option<i32>,
    /// Captured standard output text.
    /// 捕获到的标准输出文本。
    stdout: String,
    /// Captured standard error text.
    /// 捕获到的标准错误文本。
    stderr: String,
}

impl CommandOutcome {
    /// Return the most useful diagnostic text from stderr first and stdout second.
    /// 优先返回标准错误，其次返回标准输出中的最有用诊断文本。
    fn primary_diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr;
        }
        self.stdout.trim()
    }
}

/// Execute one external command and keep the full outcome for callers that need to normalize non-zero states.
/// 执行一条外部命令，并为需要归一化非零状态的调用方保留完整结果。
fn capture_command_outcome(
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
fn normalize_systemd_status_value(
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
fn is_launchd_not_loaded_message(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("could not find service")
        || lowered.contains("service could not be found")
        || lowered.contains("service not found")
        || lowered.contains("no such process")
        || lowered.contains("not loaded")
}

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
