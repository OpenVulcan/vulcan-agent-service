mod cli;
mod root_skill_cli;
mod runtime_init;
mod runtime_preload;
mod startup;
mod system_skills;

/// Run the command-line bootstrap flow for the host binary.
/// 运行宿主二进制的命令行启动编排流程。
pub use startup::run;
pub(crate) use startup::{ProcessShutdownMode, run_service_host_for_runtime_root};
