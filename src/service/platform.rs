use super::*;
use std::io::ErrorKind;

pub(super) fn install_service(
    options: ServiceInstallOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = resolve_service_runtime_root(options.runtime_root.as_deref())?;
    let artifact = build_install_artifact(&options, normalized_runtime_root.clone())?;
    preflight_runtime_config(&normalized_runtime_root)?;
    prepare_runtime_service_log_directory(&normalized_runtime_root)?;
    if options.force {
        remove_existing_service_for_reinstall(&artifact)?;
    }
    apply_install_artifact(&artifact)?;
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

/// Remove one installed service definition through the active platform manager.
/// 通过当前平台管理器卸载一份已安装的服务定义。
pub(super) fn uninstall_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if options.force {
        stop_service_if_running(&options)?;
    }
    uninstall_platform_service(&options)?;
    println!("Service uninstalled: {}", options.service_name);
    Ok(())
}

/// Start one installed service through the current platform manager.
/// 通过当前平台管理器启动一个已安装服务。
pub(super) fn start_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) fn stop_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => run_windows_service_action("stop", &options.service_name),
        HostServiceManager::Systemd => run_systemd_action("stop", &options),
        HostServiceManager::Launchd => run_launchd_stop(&options),
    }?;
    println!("Service stopped: {}", options.service_name);
    Ok(())
}

/// Stop one service only when it is running, accepting manager-specific absent or not-running states.
/// 仅在服务正在运行时停止服务，并接受各平台的不存在或未运行状态。
fn stop_service_if_running(
    options: &ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => {
            run_windows_service_stop_if_running(&options.service_name)
        }
        HostServiceManager::Systemd => run_systemd_manager_command_allowing_absent(
            options.scope,
            ["stop", options.service_name.as_str()],
        ),
        HostServiceManager::Launchd => {
            run_launchctl_bootout_if_loaded(&options.service_name, options.scope)
        }
    }
}

/// Restart one installed service through the current platform manager.
/// 通过当前平台管理器重启一个已安装服务。
pub(super) fn restart_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    match current_service_manager()? {
        HostServiceManager::WindowsScm => {
            run_windows_service_stop_if_running(&options.service_name)?;
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
pub(super) fn print_service_status(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) fn run_service_entrypoint(
    options: ServiceRunOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = resolve_service_runtime_root(options.runtime_root.as_deref())?;
    #[cfg(windows)]
    {
        windows::run_windows_service_dispatcher(ServiceRunOptions {
            runtime_root: Some(normalized_runtime_root.clone()),
            service_name: options.service_name.clone(),
        })?;
        Ok(())
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
pub(super) fn print_service_definition(
    options: ServiceInstallOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let normalized_runtime_root = resolve_service_runtime_root(options.runtime_root.as_deref())?;
    let artifact = build_install_artifact(&options, normalized_runtime_root)?;
    println!("{}", artifact.render_for_cli());
    Ok(())
}

/// Prepare the service log directory after runtime configuration preflight succeeds.
/// 在运行时配置预检成功后准备服务日志目录。
/// Parameters: `runtime_root` is the validated runtime root that owns the log directory.
/// 参数：`runtime_root` 是拥有日志目录的已校验运行根。
/// Returns success after the directory exists, or the filesystem creation error.
/// 日志目录存在后返回成功，否则返回文件系统创建错误。
fn prepare_runtime_service_log_directory(
    runtime_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(runtime_root.join("logs"))?;
    Ok(())
}

/// Preflight the runtime config so service installation fails before writing platform state.
/// 预检运行时配置，使服务安装在写入平台状态之前就能失败。
fn preflight_runtime_config(runtime_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = runtime_root.join("configs").join("config.yaml");
    if !inspect_optional_service_file(&config_path, "service config file")? {
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

/// Remove the current platform service before a force reinstall.
/// 在强制重装前移除当前平台服务。
/// Parameters: `artifact` supplies the exact service name and scope selected for reinstallation.
/// 参数：`artifact` 提供重装所选的确切服务名与作用域。
/// Returns success after precise platform-specific absence handling, or the original uninstall error.
/// 在按平台精确处理服务缺席后返回成功，否则返回原始卸载错误。
fn remove_existing_service_for_reinstall(
    artifact: &ServiceInstallArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = ServiceTargetOptions {
        service_name: artifact.service_name.clone(),
        scope: artifact.scope,
        force: true,
    };
    uninstall_service(target)
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
        HostServiceManager::WindowsScm if options.force => {
            run_windows_service_delete_if_present(&options.service_name)
        }
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
        OsString::from("binPath="),
        OsString::from(artifact.windows_bin_path_argument()),
        OsString::from("start="),
        OsString::from(start_mode),
        OsString::from("DisplayName="),
        OsString::from(artifact.display_name.clone()),
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
    run_systemd_manager_command_allowing_absent(
        options.scope,
        ["disable", options.service_name.as_str()],
    )?;
    run_systemd_manager_command_allowing_absent(
        options.scope,
        ["stop", options.service_name.as_str()],
    )?;
    remove_optional_service_file(&unit_path, "systemd unit file")?;
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
    run_launchctl_bootout_if_loaded(&options.service_name, options.scope)?;
    remove_optional_service_file(&plist_path, "launchd plist file")?;
    Ok(())
}

/// Inspect one optional service-managed file and reject non-file shapes or metadata errors.
/// 检查一个可选的服务管理文件，并拒绝非文件形态或元数据错误。
/// Parameters: `path` is the service-managed file path to inspect.
/// 参数：`path` 是需要检查的服务管理文件路径。
/// Parameters: `file_label` names the file kind in diagnostics.
/// 参数：`file_label` 用于在诊断中标识文件类型。
/// Returns `true` when the file exists, `false` when absent, or an inspection error.
/// 文件存在时返回 `true`，缺失时返回 `false`，否则返回路径检查错误。
fn inspect_optional_service_file(
    path: &Path,
    file_label: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // Only NotFound is a valid optional-file absence; everything else means the service state is ambiguous.
    // 只有 NotFound 是合法的可选文件缺失；其他情况都表示服务状态不明确。
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to inspect {} path {}: {}",
                file_label,
                path.display(),
                error
            )
            .into());
        }
    };
    if !metadata.is_file() {
        return Err(format!("{} path is not a file: {}", file_label, path.display()).into());
    }
    Ok(true)
}

/// Remove one optional service-managed file after explicit metadata inspection.
/// 在显式元数据检查后删除一个可选的服务管理文件。
/// Parameters: `path` is the service-managed file path to remove when present.
/// 参数：`path` 是存在时需要删除的服务管理文件路径。
/// Parameters: `file_label` names the file kind in diagnostics.
/// 参数：`file_label` 用于在诊断中标识文件类型。
/// Returns `Ok(())` when the file is absent or removed, otherwise a path-aware deletion error.
/// 文件缺失或已删除时返回 `Ok(())`，否则返回包含路径的删除错误。
fn remove_optional_service_file(
    path: &Path,
    file_label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !inspect_optional_service_file(path, file_label)? {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|error| {
        format!(
            "failed to remove {} {}: {}",
            file_label,
            path.display(),
            error
        )
    })?;
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

/// Stop one Windows service and accept the idempotent already-stopped or absent states.
/// 停止一个 Windows 服务，并接受已经停止或不存在这类幂等状态。
fn run_windows_service_stop_if_running(
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    run_windows_service_action_with_allowed_diagnostic(
        "stop",
        service_name,
        is_windows_service_absent_or_stopped_message,
    )
}

/// Delete one Windows service and accept the idempotent absent state.
/// 删除一个 Windows 服务，并接受服务不存在这一幂等状态。
fn run_windows_service_delete_if_present(
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    run_windows_service_action_with_allowed_diagnostic(
        "delete",
        service_name,
        is_windows_service_absent_message,
    )
}

/// Run one Windows service action while allowing a narrow idempotent diagnostic.
/// 执行一个 Windows 服务动作，同时允许一类狭窄的幂等诊断。
fn run_windows_service_action_with_allowed_diagnostic(
    action: &str,
    service_name: &str,
    allowed_diagnostic: fn(&str) -> bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let outcome = capture_command_outcome(
        "sc.exe",
        &[OsString::from(action), OsString::from(service_name)],
    )?;
    if outcome.success || allowed_diagnostic(outcome.primary_diagnostic()) {
        return Ok(());
    }
    Err(format!(
        "sc.exe {} failed with code {:?}: {}",
        action,
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
}

/// Return whether a Windows SCM diagnostic means the service does not exist.
/// 判断 Windows SCM 诊断是否表示服务不存在。
fn is_windows_service_absent_message(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("does not exist as an installed service")
        || lowered.contains("openservice failed 1060")
        || lowered.contains("the specified service does not exist")
}

/// Return whether a Windows SCM diagnostic means the service is absent or already stopped.
/// 判断 Windows SCM 诊断是否表示服务不存在或已经停止。
fn is_windows_service_absent_or_stopped_message(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    is_windows_service_absent_message(&lowered)
        || lowered.contains("service has not been started")
        || lowered.contains("the service has not been started")
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

/// Execute one `systemctl` command while accepting idempotent absent or not-loaded diagnostics.
/// 执行一条 `systemctl` 命令，同时接受不存在或未加载这类幂等诊断。
fn run_systemd_manager_command_allowing_absent<const N: usize>(
    scope: ServiceScope,
    tail: [&str; N],
) -> Result<(), Box<dyn std::error::Error>> {
    let rendered_tail = tail.join(" ");
    let outcome = capture_systemd_manager_outcome(scope, tail)?;
    if outcome.success || is_systemd_absent_or_not_loaded_message(outcome.primary_diagnostic()) {
        return Ok(());
    }
    Err(format!(
        "systemctl {} failed with code {:?}: {}",
        rendered_tail,
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
}

/// Return whether a systemd diagnostic means the unit is absent or not loaded.
/// 判断 systemd 诊断是否表示 unit 不存在或未加载。
fn is_systemd_absent_or_not_loaded_message(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("not loaded")
        || lowered.contains("not found")
        || lowered.contains("could not be found")
        || lowered.contains("no such file")
        || lowered.contains("does not exist")
        || lowered.contains("unit file") && lowered.contains("does not exist")
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

/// Boot one launchd job out while accepting the idempotent not-loaded state.
/// 移除一个 launchd 作业，同时接受未加载这一幂等状态。
fn run_launchctl_bootout_if_loaded(
    service_name: &str,
    scope: ServiceScope,
) -> Result<(), Box<dyn std::error::Error>> {
    let domain = definition::launchd_domain(scope)?;
    let label = definition::launchd_label(service_name);
    let outcome = capture_command_outcome(
        "launchctl",
        &[
            OsString::from("bootout"),
            OsString::from(format!("{}/{}", domain, label)),
        ],
    )?;
    if outcome.success || is_launchd_not_loaded_message(outcome.primary_diagnostic()) {
        return Ok(());
    }
    Err(format!(
        "launchctl bootout failed with code {:?}: {}",
        outcome.exit_code,
        outcome.primary_diagnostic()
    )
    .into())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one unique temporary directory path for a platform test case.
    /// 为 platform 测试用例构建一个唯一临时目录路径。
    fn unique_platform_test_dir(name: &str) -> PathBuf {
        let unique = format!(
            "vulcan-agent-service-platform-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    /// Remove one platform test directory while accepting already-absent cleanup targets.
    /// 删除一个 platform 测试目录，并接受清理目标已经缺失的情况。
    /// Parameters: `path` is the temporary platform test directory to remove.
    /// 参数：`path` 是需要删除的临时 platform 测试目录。
    /// Returns nothing and panics with path context when cleanup cannot complete.
    /// 不返回值；清理无法完成时携带路径上下文 panic。
    fn remove_platform_test_dir(path: &Path) {
        match std::fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                panic!(
                    "temporary platform test directory should be removed: {}: {}",
                    path.display(),
                    error
                );
            }
        }
    }

    /// Runtime config preflight should report a missing required config file explicitly.
    /// 运行时配置预检应显式报告缺失的必需配置文件。
    #[test]
    fn preflight_runtime_config_reports_missing_config_file() {
        let temp_dir = unique_platform_test_dir("missing-config");
        let error = preflight_runtime_config(&temp_dir)
            .expect_err("missing runtime config should fail preflight");
        let message = error.to_string();
        assert!(
            message.contains("config file not found"),
            "missing config error should be explicit: {message}"
        );
        remove_platform_test_dir(&temp_dir);
    }

    /// Runtime config preflight should reject a directory where the config file is required.
    /// 运行时配置预检应拒绝必需配置文件位置出现目录。
    #[test]
    fn preflight_runtime_config_rejects_directory_shaped_config_file() {
        let temp_dir = unique_platform_test_dir("directory-config");
        let config_path = temp_dir.join("configs").join("config.yaml");
        std::fs::create_dir_all(&config_path)
            .expect("directory-shaped config path should be created");
        let error = preflight_runtime_config(&temp_dir)
            .expect_err("directory-shaped runtime config should fail preflight");
        let message = error.to_string();
        assert!(
            message.contains("service config file path is not a file"),
            "directory-shaped config error should be explicit: {message}"
        );
        remove_platform_test_dir(&temp_dir);
    }

    /// Optional service file removal should ignore a missing cleanup target.
    /// 可选服务文件删除应忽略缺失的清理目标。
    #[test]
    fn remove_optional_service_file_ignores_missing_file() {
        let temp_dir = unique_platform_test_dir("missing-optional-file");
        let file_path = temp_dir.join("missing.service");
        remove_optional_service_file(&file_path, "test service file")
            .expect("missing optional service file should be ignored");
        remove_platform_test_dir(&temp_dir);
    }

    /// Optional service file removal should delete an existing regular file.
    /// 可选服务文件删除应删除已经存在的普通文件。
    #[test]
    fn remove_optional_service_file_removes_existing_file() {
        let temp_dir = unique_platform_test_dir("remove-existing-file");
        let file_path = temp_dir.join("existing.service");
        std::fs::create_dir_all(&temp_dir).expect("temporary platform directory should be created");
        std::fs::write(&file_path, "unit").expect("temporary service file should be written");
        remove_optional_service_file(&file_path, "test service file")
            .expect("existing optional service file should be removed");
        let metadata_result = std::fs::metadata(&file_path);
        assert!(
            matches!(metadata_result, Err(error) if error.kind() == ErrorKind::NotFound),
            "removed service file should be absent"
        );
        remove_platform_test_dir(&temp_dir);
    }

    /// Optional service file removal should reject a directory-shaped cleanup target.
    /// 可选服务文件删除应拒绝目录形态的清理目标。
    #[test]
    fn remove_optional_service_file_rejects_directory_path() {
        let temp_dir = unique_platform_test_dir("directory-optional-file");
        let file_path = temp_dir.join("directory.service");
        std::fs::create_dir_all(&file_path)
            .expect("directory-shaped optional service file path should be created");
        let error = remove_optional_service_file(&file_path, "test service file")
            .expect_err("directory-shaped optional service file should fail removal");
        let message = error.to_string();
        assert!(
            message.contains("test service file path is not a file"),
            "directory-shaped optional file error should be explicit: {message}"
        );
        remove_platform_test_dir(&temp_dir);
    }

    /// Windows SCM absence diagnostics should be accepted for force cleanup paths.
    /// Windows SCM 的服务缺席诊断应被 force 清理路径接受。
    #[test]
    fn windows_service_absence_diagnostic_is_idempotent() {
        assert!(is_windows_service_absent_message(
            "[SC] OpenService FAILED 1060:\n\nThe specified service does not exist as an installed service."
        ));
    }

    /// Windows SCM already-stopped diagnostics should be accepted for stop-before-start paths.
    /// Windows SCM 的已停止诊断应被先停再启路径接受。
    #[test]
    fn windows_service_stopped_diagnostic_is_idempotent_for_stop() {
        assert!(is_windows_service_absent_or_stopped_message(
            "The service has not been started."
        ));
    }

    /// Systemd absence diagnostics should be accepted for uninstall cleanup paths.
    /// systemd 的 unit 缺席诊断应被卸载清理路径接受。
    #[test]
    fn systemd_absence_diagnostic_is_idempotent() {
        assert!(is_systemd_absent_or_not_loaded_message(
            "Failed to disable unit: Unit file vulcan-agent-service.service does not exist."
        ));
        assert!(is_systemd_absent_or_not_loaded_message(
            "Failed to stop vulcan-agent-service.service: Unit vulcan-agent-service.service not loaded."
        ));
    }
}
