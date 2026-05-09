use super::ServiceRunOptions;
use crate::bootstrap::{ProcessShutdownMode, run_service_host_for_runtime_root};
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
    let run_result = run_service_host_for_runtime_root(
        &options.runtime_root,
        ProcessShutdownMode::External(shutdown_rx),
    );
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
