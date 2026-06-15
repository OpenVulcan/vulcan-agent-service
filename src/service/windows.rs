use super::ServiceRunOptions;
use crate::bootstrap::{ProcessShutdownMode, run_service_host_for_runtime_root};
use chrono::Local;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::watch;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

/// Stable in-process storage for the parsed Windows service run options.
/// 已解析的 Windows 服务运行选项在进程内的稳定存储。
static WINDOWS_SERVICE_RUN_OPTIONS: OnceLock<ServiceRunOptions> = OnceLock::new();

windows_service::define_windows_service!(ffi_service_main, service_main_entry);

/// Start the Windows service dispatcher and fall back to foreground mode when not launched by SCM.
/// 启动 Windows 服务分发器，并要求当前入口只能由 SCM 以服务方式拉起。
pub(crate) fn run_windows_service_dispatcher(
    options: ServiceRunOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = WINDOWS_SERVICE_RUN_OPTIONS.set(options.clone());
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
    let status_handle =
        service_control_handler::register(options.service_name.clone(), move |control_event| {
            match control_event {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    let _ = shutdown_tx.send(true);
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        })?;
    set_windows_service_status(
        &status_handle,
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
    )?;
    set_windows_service_status(
        &status_handle,
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
    )?;
    let run_result =
        run_service_host_for_runtime_root(runtime_root, ProcessShutdownMode::External(shutdown_rx));
    set_windows_service_status(
        &status_handle,
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
    )?;
    set_windows_service_stopped(&status_handle, run_result.is_ok())?;
    run_result
}

/// Set one intermediate Windows service status on the SCM handle.
/// 在 SCM 句柄上设置一条中间态 Windows 服务状态。
fn set_windows_service_status(
    status_handle: &windows_service::service_control_handler::ServiceStatusHandle,
    state: ServiceState,
    controls_accepted: ServiceControlAccept,
) -> Result<(), Box<dyn std::error::Error>> {
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
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
