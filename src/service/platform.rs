use super::*;

pub(super) fn install_service(
    options: ServiceInstallOptions,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) fn uninstall_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
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

/// Restart one installed service through the current platform manager.
/// 通过当前平台管理器重启一个已安装服务。
pub(super) fn restart_service(
    options: ServiceTargetOptions,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) fn print_service_definition(
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
