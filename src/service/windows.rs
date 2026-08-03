use super::ServiceRunOptions;
use crate::bootstrap::{ProcessShutdownMode, run_service_host_for_runtime_root};
use chrono::Local;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::service_dispatcher;

/// Maximum time advertised to SCM for graceful service shutdown.
/// 向 SCM 声明的服务优雅停止最长等待时间。
const WINDOWS_SERVICE_STOP_WAIT_HINT: Duration = Duration::from_secs(30);

/// Stable in-process storage for the parsed Windows service run options.
/// 已解析的 Windows 服务运行选项在进程内的稳定存储。
static WINDOWS_SERVICE_RUN_OPTIONS: OnceLock<ServiceRunOptions> = OnceLock::new();

windows_service::define_windows_service!(ffi_service_main, service_main_entry);

/// Start the Windows service dispatcher and fall back to foreground mode when not launched by SCM.
/// 启动 Windows 服务分发器，并要求当前入口只能由 SCM 以服务方式拉起。
pub(crate) fn run_windows_service_dispatcher(
    options: ServiceRunOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = set_windows_service_run_options(options.clone()) {
        append_windows_service_error_log(options.runtime_root.as_deref(), &error);
        return Err(error.into());
    }
    service_dispatcher::start(options.service_name.clone(), ffi_service_main).map_err(
        |error| -> Box<dyn std::error::Error> {
            append_windows_service_error_log(
                options.runtime_root.as_deref(),
                &format!(
                    "failed to start Windows service dispatcher for '{}': {}",
                    options.service_name, error
                ),
            );
            format!(
                "failed to start Windows service dispatcher for '{}': {}. `service run` must be launched by SCM-managed service startup on Windows.",
                options.service_name, error
            )
            .into()
        },
    )
}

/// Store Windows service run options in the process-global dispatcher cell.
/// 将 Windows 服务运行选项保存到进程级 dispatcher 单元。
///
/// Parameters: `options` contains the normalized runtime root and service name used by SCM callbacks.
/// 参数：`options` 包含 SCM 回调使用的已规范化运行根与服务名称。
///
/// Returns: `Ok(())` when the options are newly stored or identical to existing options.
/// 返回：选项新写入或与既有选项一致时返回 `Ok(())`。
fn set_windows_service_run_options(options: ServiceRunOptions) -> Result<(), String> {
    set_windows_service_run_options_cell(&WINDOWS_SERVICE_RUN_OPTIONS, options)
}

/// Store Windows service run options in one concrete OnceLock while rejecting conflicting options.
/// 将 Windows 服务运行选项保存到指定 OnceLock，并拒绝冲突选项。
///
/// Parameters: `cell` stores the process-global or test-local service run options.
/// 参数：`cell` 存储进程级或测试局部的服务运行选项。
///
/// Parameters: `options` is the service run options value to register.
/// 参数：`options` 是需要注册的服务运行选项。
///
/// Returns: `Ok(())` when registration is compatible, or an error describing the conflict.
/// 返回：注册兼容时返回 `Ok(())`，否则返回描述冲突的错误。
fn set_windows_service_run_options_cell(
    cell: &OnceLock<ServiceRunOptions>,
    options: ServiceRunOptions,
) -> Result<(), String> {
    if let Some(existing_options) = cell.get() {
        if same_service_run_options(existing_options, &options) {
            return Ok(());
        }
        return Err(format!(
            "Windows service run options already initialized as {}, cannot reinitialize as {}",
            describe_service_run_options(existing_options),
            describe_service_run_options(&options)
        ));
    }

    match cell.set(options) {
        Ok(()) => Ok(()),
        Err(rejected_options) => {
            let Some(existing_options) = cell.get() else {
                return Err(format!(
                    "Windows service run options initialization raced while registering {}",
                    describe_service_run_options(&rejected_options)
                ));
            };
            if same_service_run_options(existing_options, &rejected_options) {
                return Ok(());
            }
            Err(format!(
                "Windows service run options already initialized as {}, cannot reinitialize as {}",
                describe_service_run_options(existing_options),
                describe_service_run_options(&rejected_options)
            ))
        }
    }
}

/// Return whether two Windows service run option values describe the same service entrypoint.
/// 判断两份 Windows 服务运行选项是否描述同一个服务入口。
///
/// Parameters: `left` is the existing service run option value.
/// 参数：`left` 是既有服务运行选项。
///
/// Parameters: `right` is the candidate service run option value.
/// 参数：`right` 是候选服务运行选项。
///
/// Returns: `true` when runtime root and service name both match.
/// 返回：当运行根与服务名称均一致时返回 `true`。
fn same_service_run_options(left: &ServiceRunOptions, right: &ServiceRunOptions) -> bool {
    left.runtime_root == right.runtime_root && left.service_name == right.service_name
}

/// Render one Windows service run option value for conflict diagnostics.
/// 将一份 Windows 服务运行选项渲染为冲突诊断文本。
///
/// Parameters: `options` is the service run option value to describe.
/// 参数：`options` 是需要描述的服务运行选项。
///
/// Returns: a compact string containing service name and runtime root.
/// 返回：包含服务名称与运行根的紧凑字符串。
fn describe_service_run_options(options: &ServiceRunOptions) -> String {
    let runtime_root = options
        .runtime_root
        .as_ref()
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| "<none>".to_string());
    format!(
        "service_name='{}', runtime_root='{}'",
        options.service_name, runtime_root
    )
}

/// Windows SCM callback entrypoint that forwards into the typed service main body.
/// Windows SCM 回调入口，并转发到具类型的服务主体函数。
fn service_main_entry(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_windows_service_main() {
        append_windows_service_error_log(
            current_windows_service_runtime_root(),
            &format!("service startup failed: {}", error),
        );
        eprintln!("[Service] Windows service failed: {}", error);
    }
}

/// Run the real Windows service body and bridge SCM stop requests into the shared host runner.
/// 运行真正的 Windows 服务主体，并把 SCM 停止请求桥接到共享宿主运行器。
fn run_windows_service_main() -> Result<(), Box<dyn std::error::Error>> {
    let options = WINDOWS_SERVICE_RUN_OPTIONS
        .get()
        .cloned()
        .ok_or("missing Windows service run options")?;
    let runtime_root = options
        .runtime_root
        .as_ref()
        .ok_or("missing Windows service runtime root")?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_log_root = options.runtime_root.clone();
    let status_handle_slot: Arc<Mutex<Option<ServiceStatusHandle>>> = Arc::new(Mutex::new(None));
    let status_handle_slot_for_handler = Arc::clone(&status_handle_slot);
    let status_handle = service_control_handler::register(
        options.service_name.clone(),
        move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Ok(status_guard) = status_handle_slot_for_handler.lock()
                    && let Some(status_handle) = status_guard.as_ref()
                    && let Err(error) = set_windows_service_status(
                        status_handle,
                        ServiceState::StopPending,
                        ServiceControlAccept::empty(),
                        1,
                        WINDOWS_SERVICE_STOP_WAIT_HINT,
                    )
                {
                    append_windows_service_error_log(
                        shutdown_log_root.as_deref(),
                        &format!("failed to report StopPending before shutdown: {}", error),
                    );
                }
                if !request_windows_service_shutdown(&shutdown_tx) {
                    append_windows_service_error_log(
                        shutdown_log_root.as_deref(),
                        "Windows service shutdown receiver was already dropped before SCM stop request could be delivered",
                    );
                }
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    )?;
    *status_handle_slot
        .lock()
        .map_err(|_| "Windows service status handle slot was poisoned")? = Some(status_handle);
    let status_handle = status_handle_slot
        .lock()
        .map_err(|_| "Windows service status handle slot was poisoned")?
        .as_ref()
        .copied()
        .ok_or("Windows service status handle was not stored")?;
    set_windows_service_status(
        &status_handle,
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        1,
        WINDOWS_SERVICE_STOP_WAIT_HINT,
    )?;
    set_windows_service_status(
        &status_handle,
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        0,
        Duration::default(),
    )?;
    let run_result =
        run_service_host_for_runtime_root(runtime_root, ProcessShutdownMode::External(shutdown_rx));
    set_windows_service_status(
        &status_handle,
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
        2,
        Duration::from_secs(5),
    )?;
    set_windows_service_stopped(&status_handle, run_result.is_ok())?;
    run_result
}

/// Request shutdown through the Windows service control watch channel.
/// 通过 Windows 服务控制 watch 通道请求关闭。
///
/// Parameters: `shutdown_tx` is the sender paired with the shared host service runner.
/// 参数：`shutdown_tx` 是与共享宿主服务运行器配对的发送端。
///
/// Returns: `true` when at least one receiver accepted the stop request.
/// 返回：当至少一个接收端接受停止请求时返回 `true`。
fn request_windows_service_shutdown(shutdown_tx: &watch::Sender<bool>) -> bool {
    shutdown_tx.send(true).is_ok()
}

/// Set one intermediate Windows service status on the SCM handle.
/// 在 SCM 句柄上设置一条中间态 Windows 服务状态。
fn set_windows_service_status(
    status_handle: &windows_service::service_control_handler::ServiceStatusHandle,
    state: ServiceState,
    controls_accepted: ServiceControlAccept,
    checkpoint: u32,
    wait_hint: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint,
        wait_hint,
        process_id: None,
    })?;
    Ok(())
}

/// Set the final Windows service stopped status with a generic success or failure code.
/// 设置最终的 Windows 服务停止状态，并附带通用成功或失败退出码。
fn set_windows_service_stopped(
    status_handle: &windows_service::service_control_handler::ServiceStatusHandle,
    succeeded: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: if succeeded {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(1)
        },
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;
    Ok(())
}

/// Return the current Windows service runtime root when dispatcher options are available.
/// 当服务分发器选项可用时返回当前 Windows 服务运行根。
fn current_windows_service_runtime_root() -> Option<&'static Path> {
    WINDOWS_SERVICE_RUN_OPTIONS
        .get()
        .and_then(|options| options.runtime_root.as_deref())
}

/// Append one Windows service failure line into the runtime-root log file for post-mortem diagnosis.
/// 为事后诊断把一条 Windows 服务失败信息追加到运行根日志文件。
fn append_windows_service_error_log(runtime_root: Option<&Path>, message: &str) {
    let Some(runtime_root) = runtime_root else {
        return;
    };
    let logs_dir = runtime_root.join("logs");
    if std::fs::create_dir_all(&logs_dir).is_err() {
        return;
    }
    let log_path = logs_dir.join("service.stderr.log");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(log_path)
    else {
        return;
    };
    ensure_log_entry_starts_on_new_line(&mut file);
    let _ = writeln!(
        file,
        "[{}] [WindowsService] {}",
        Local::now().to_rfc3339(),
        message
    );
}

/// Ensure a new service log entry never gets concatenated onto an older partial line.
/// 确保新的服务日志条目不会和旧的残缺行直接粘连。
fn ensure_log_entry_starts_on_new_line(file: &mut std::fs::File) {
    let Ok(metadata) = file.metadata() else {
        return;
    };
    if metadata.len() == 0 {
        return;
    }
    let mut last_byte = [0_u8; 1];
    if file.seek(SeekFrom::End(-1)).is_err() {
        return;
    }
    if file.read_exact(&mut last_byte).is_err() {
        return;
    }
    if last_byte[0] == b'\n' {
        return;
    }
    if file.seek(SeekFrom::End(0)).is_err() {
        return;
    }
    let _ = file.write_all(b"\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build representative Windows service run options for dispatcher option tests.
    /// 为 dispatcher 选项测试构建代表性的 Windows 服务运行选项。
    ///
    /// Parameters: `service_name` is the service name stored in the options.
    /// 参数：`service_name` 是写入选项的服务名称。
    ///
    /// Parameters: `runtime_root` is the runtime root text stored in the options.
    /// 参数：`runtime_root` 是写入选项的运行根文本。
    ///
    /// Returns a service run options value with the provided identity fields.
    /// 返回包含指定身份字段的服务运行选项。
    fn test_service_run_options(service_name: &str, runtime_root: &str) -> ServiceRunOptions {
        ServiceRunOptions {
            runtime_root: Some(std::path::PathBuf::from(runtime_root)),
            service_name: service_name.to_string(),
        }
    }

    /// Windows service shutdown notification should send a true update while a receiver is alive.
    /// Windows 服务关闭通知应在接收端存活时发送 true 更新。
    #[tokio::test]
    async fn request_windows_service_shutdown_sends_true_update() {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        assert!(request_windows_service_shutdown(&shutdown_tx));
        shutdown_rx
            .changed()
            .await
            .expect("shutdown receiver should observe the update");

        assert!(*shutdown_rx.borrow());
    }

    /// Windows service shutdown notification should report when every receiver has already dropped.
    /// Windows 服务关闭通知应在所有接收端均已丢弃时报告失败。
    #[test]
    fn request_windows_service_shutdown_reports_dropped_receiver() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        drop(shutdown_rx);

        assert!(!request_windows_service_shutdown(&shutdown_tx));
    }

    /// Windows service options registration should accept repeated identical options.
    /// Windows 服务选项注册应接受完全相同选项的重复写入。
    #[test]
    fn set_windows_service_run_options_cell_accepts_same_options() {
        // Build a local OnceLock so this test does not initialize the real service dispatcher cell.
        // 构造局部 OnceLock，避免测试初始化真实服务 dispatcher 单元。
        let cell = OnceLock::new();
        // Build one service option value used for both registrations.
        // 构造一份用于两次注册的服务选项。
        let options = test_service_run_options("vas-demo", "C:\\runtime-a");

        set_windows_service_run_options_cell(&cell, options.clone())
            .expect("first service options registration should succeed");
        set_windows_service_run_options_cell(&cell, options.clone())
            .expect("same service options registration should remain compatible");

        let stored_options = cell
            .get()
            .expect("service options should be stored after registration");
        assert!(same_service_run_options(stored_options, &options));
    }

    /// Windows service options registration should reject a different runtime root for the same service.
    /// Windows 服务选项注册应拒绝同一服务名称下的不同运行根。
    #[test]
    fn set_windows_service_run_options_cell_rejects_different_runtime_root() {
        // Build a local OnceLock so the conflict does not affect other service tests.
        // 构造局部 OnceLock，避免冲突影响其它服务测试。
        let cell = OnceLock::new();
        // Build the initially accepted service options.
        // 构造首次接受的服务选项。
        let first_options = test_service_run_options("vas-demo", "C:\\runtime-a");
        // Build conflicting options that only change runtime root.
        // 构造仅修改运行根的冲突选项。
        let second_options = test_service_run_options("vas-demo", "C:\\runtime-b");

        set_windows_service_run_options_cell(&cell, first_options)
            .expect("first service options registration should succeed");
        let error = set_windows_service_run_options_cell(&cell, second_options)
            .expect_err("different runtime root should be rejected");

        assert!(error.contains("Windows service run options already initialized as"));
        assert!(error.contains("runtime-a"));
        assert!(error.contains("runtime-b"));
    }

    /// Windows service options registration should reject a different service name for the same runtime root.
    /// Windows 服务选项注册应拒绝同一运行根下的不同服务名称。
    #[test]
    fn set_windows_service_run_options_cell_rejects_different_service_name() {
        // Build a local OnceLock so the conflict assertion remains isolated.
        // 构造局部 OnceLock，确保冲突断言保持隔离。
        let cell = OnceLock::new();
        // Build the initially accepted service options.
        // 构造首次接受的服务选项。
        let first_options = test_service_run_options("vas-demo-a", "C:\\runtime-a");
        // Build conflicting options that only change service name.
        // 构造仅修改服务名称的冲突选项。
        let second_options = test_service_run_options("vas-demo-b", "C:\\runtime-a");

        set_windows_service_run_options_cell(&cell, first_options)
            .expect("first service options registration should succeed");
        let error = set_windows_service_run_options_cell(&cell, second_options)
            .expect_err("different service name should be rejected");

        assert!(error.contains("Windows service run options already initialized as"));
        assert!(error.contains("vas-demo-a"));
        assert!(error.contains("vas-demo-b"));
    }

    /// Build one unique temporary root used by Windows-service log tests.
    /// 为 Windows 服务日志测试构建唯一临时根目录。
    fn unique_log_test_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-agent-service-windows-log-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ))
    }

    /// Verify that a new service error line starts on its own line even when the file ended without a newline.
    /// 验证当日志文件末尾没有换行时，新服务错误行也会从独立新行开始。
    #[test]
    fn append_windows_service_error_log_separates_partial_previous_line() {
        let runtime_root = unique_log_test_root("newline-boundary");
        let logs_dir = runtime_root.join("logs");
        let log_path = logs_dir.join("service.stderr.log");
        std::fs::create_dir_all(&logs_dir).expect("log directory should be created");
        std::fs::write(&log_path, "partial previous line")
            .expect("seed log file should be written");

        append_windows_service_error_log(Some(runtime_root.as_path()), "fresh service error");

        let content = std::fs::read_to_string(&log_path).expect("log file should be readable");
        assert!(content.contains("partial previous line\n["));
        assert!(content.contains("fresh service error"));

        let _ = std::fs::remove_dir_all(runtime_root);
    }
}
