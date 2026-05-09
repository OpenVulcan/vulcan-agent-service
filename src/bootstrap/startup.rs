#[cfg(test)]
use super::cli::{DEFAULT_CALL_TOOL_CLIENT_NAME, parse_runtime_mode_from_args};
use super::cli::{RuntimeMode, parse_runtime_mode};
use super::root_skill_cli::{run_root_skill_install_mode, run_root_skills_update_mode};
use super::runtime_init::{
    add_libs_to_path, build_call_tool_request_context, build_host_tool_surface_server,
    build_server, build_single_vm_lua_engine_for_local_mode,
    initialize_runtime_temp_root_from_config,
};
#[cfg(test)]
use super::runtime_init::{
    build_root_skill_manager_for_cli, collect_managed_root_skill_ids,
    ensure_root_skill_manager_root, ensure_skill_manager_runtime_roots,
    select_root_skill_manager_root,
};
use super::runtime_preload::preload_runtime_mcp_configs;
use crate::config::Config;
use crate::host_core::{HostRuntime, host_tool_requires_lua_engine, is_host_tool_name};
use crate::luaskills_adapter::{
    build_runtime_invocation_context, client_budget_snapshot_for_render,
    install_luaskills_log_callback,
};
use crate::service::run_service_command;
use crate::support::RuntimeRequestContext;
use crate::support::runtime_logging::set_non_error_logging_enabled;
use crate::support::temp_maintenance::{
    CleanupTrigger, ensure_runtime_temp_dir, maintain_runtime_temp_dir,
    spawn_cross_day_cleanup_task,
};
use crate::support::tool_result_format::{
    HostRenderOptions, RuntimeInvocationResult, render_tool_result_text,
};
use crate::transport;
use crate::transport::mcp::McpDispatcher;
use crate::transport::mcp::mapping::runtime_context_from_mcp;
#[cfg(test)]
use crate::transport::mcp::protocol::RequestContext;
use crate::transport::mcp::protocol::ToolCallResult;
#[cfg(test)]
use luaskills::{LuaRuntimeHostOptions, RuntimeSkillRoot, SkillInstallSourceType};
use serde_json::{Value, json};
use std::path::Path;
use tokio::sync::watch;

/// Shutdown source used by the shared long-running host service entrypoint.
/// 共享长驻宿主服务入口使用的关闭信号来源。
pub(crate) enum ProcessShutdownMode {
    /// Wait for process-level signals such as Ctrl+C or SIGTERM.
    /// 等待 Ctrl+C 或 SIGTERM 等进程级信号。
    #[cfg(not(windows))]
    ProcessSignals,
    /// Wait for an externally supplied watch receiver.
    /// 等待外部提供的 watch 接收器。
    External(watch::Receiver<bool>),
}

/// Print the final `--call-tools` result.
/// 输出 `--call-tools` 的最终结果。
/// The debug mode also injects a simulated client context, so budget resolution must use the same request context.
/// 调试模式同样会注入模拟客户端上下文，因此预算解析也必须基于同一份请求上下文完成。
fn print_call_tools_result(
    value: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    tool_name: Option<&str>,
    request_context: Option<&RuntimeRequestContext>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_budget = client_budget_snapshot_for_render(request_context, tool_name, skill_name);
    let spill_root = ensure_runtime_temp_dir()?.join("mcp").join("cache");
    println!(
        "{}",
        render_tool_result_text(
            value,
            skill_name,
            Some(&client_budget),
            &HostRenderOptions {
                spill_root: Some(spill_root),
                ..HostRenderOptions::default()
            },
        )
    );
    Ok(())
}

/// Run the selected command-line mode after parsing process arguments.
/// 解析进程参数后运行选定的命令行模式。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_mode = parse_runtime_mode()?;
    match runtime_mode {
        RuntimeMode::CallTool {
            tool_name,
            arguments,
            simulated_client_name,
        } => run_call_tool_mode(&tool_name, arguments, &simulated_client_name),
        RuntimeMode::RootSkillInstall {
            source,
            source_type,
        } => run_root_skill_install_mode(&source, source_type),
        RuntimeMode::RootSkillsUpdate => run_root_skills_update_mode(),
        RuntimeMode::InternalLuaexecRequest { request_file } => {
            run_internal_luaexec_request_mode(&request_file)
        }
        RuntimeMode::Service(command) => run_service_command(command),
        RuntimeMode::Stdio => {
            let cfg = Config::load()?;
            add_libs_to_path(&cfg)?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            let server = runtime.block_on(async_build_stdio_server(cfg))?;
            runtime.block_on(async_run_stdio_server(server.clone()))?;
            drop(server);
            Ok(())
        }
        RuntimeMode::Serve => {
            let cfg = Config::load()?;
            add_libs_to_path(&cfg)?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            // Keep one owner of the prepared host runtime outside async teardown paths so
            // transport-start failures do not drop LuaSkills controller bridges inside Tokio.
            // 在异步清理路径之外保留一份已准备宿主运行时的所有权，避免传输层启动失败时在 Tokio 内部析构 LuaSkills 控制器桥。
            let server = runtime.block_on(async_build_stdio_server(cfg.clone()))?;
            let result = runtime.block_on(async {
                spawn_cross_day_cleanup_task();
                run_network_transports(server.clone(), &cfg).await
            });
            drop(server);
            result
        }
    }
}

/// Async stdio bootstrap flow that prepares one initialized server without starting network transports.
/// 为 stdio 模式准备一份已初始化服务且不启动网络传输的异步引导流程。
async fn async_build_stdio_server(cfg: Config) -> Result<HostRuntime, Box<dyn std::error::Error>> {
    install_luaskills_log_callback();
    initialize_runtime_temp_root_from_config(&cfg)?;

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&cfg)?;

    build_server(&cfg).await
}

/// Async stdio serving flow that runs one already prepared server on stdin/stdout only.
/// 仅通过标准输入输出运行一份已准备服务实例的异步 stdio 服务流程。
async fn async_run_stdio_server(server: HostRuntime) -> Result<(), Box<dyn std::error::Error>> {
    spawn_cross_day_cleanup_task();
    transport::stdio::run_stdio(server).await
}

/// Run the shared host service body from an explicit runtime root and shutdown mode.
/// 基于显式运行根与关闭模式运行共享宿主服务主体。
pub(crate) fn run_service_host_for_runtime_root(
    runtime_root: &Path,
    shutdown_mode: ProcessShutdownMode,
) -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_current_dir(runtime_root)?;
    let cfg = load_service_config_from_runtime_root(runtime_root)?;
    add_libs_to_path(&cfg)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let server = runtime.block_on(async_build_stdio_server(cfg.clone()))?;
    let shutdown_rx = match shutdown_mode {
        #[cfg(not(windows))]
        ProcessShutdownMode::ProcessSignals => {
            let (shutdown_tx, shutdown_rx) = watch::channel(false);
            runtime.spawn(async move {
                wait_for_process_shutdown_signal().await;
                let _ = shutdown_tx.send(true);
            });
            shutdown_rx
        }
        ProcessShutdownMode::External(shutdown_rx) => shutdown_rx,
    };
    let result = runtime.block_on(async {
        spawn_cross_day_cleanup_task();
        run_network_transports_with_shutdown(server.clone(), &cfg, shutdown_rx).await
    });
    drop(server);
    result
}

/// Load service config directly from one runtime root without relying on CLI fallback discovery.
/// 直接从某个运行根加载服务配置，而不依赖 CLI 回退发现逻辑。
fn load_service_config_from_runtime_root(
    runtime_root: &Path,
) -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = runtime_root.join("configs").join("config.yaml");
    let mut cfg = Config::from_file(&config_path.to_string_lossy())?;
    cfg.runtime_root = Some(runtime_root.to_string_lossy().to_string());
    Ok(cfg)
}

/// Wait for the current process shutdown signal in a transport-friendly async form.
/// 以适合传输层的异步形式等待当前进程的关闭信号。
#[cfg(not(windows))]
async fn wait_for_process_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate_signal = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(terminate_signal) = terminate_signal.as_mut() {
                    let _ = terminate_signal.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Run the default HTTP/gRPC service mode.
/// 运行默认的 HTTP/gRPC 服务模式。
async fn run_network_transports(
    server: HostRuntime,
    cfg: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let http_addr = cfg
        .http
        .clone()
        .unwrap_or_else(|| "127.0.0.1:19201".to_string());
    let grpc_addr = cfg
        .grpc
        .clone()
        .unwrap_or_else(|| "127.0.0.1:19202".to_string());

    // Clone server for parallel transports
    let server_for_http = server.clone();
    let server_for_grpc = server.clone();

    // Run both servers concurrently — wrap errors into String for Send safety
    let http_task = tokio::spawn(async move {
        transport::http::run_http(server_for_http, &http_addr)
            .await
            .map_err(|e| format!("[HTTP] {e}"))
    });
    let grpc_task = tokio::spawn(async move {
        transport::grpc::run_grpc(server_for_grpc, &grpc_addr)
            .await
            .map_err(|e| format!("[gRPC] {e}"))
    });

    // Wait for either to finish (they run until shutdown)
    let (http_result, grpc_result) = tokio::join!(http_task, grpc_task);
    http_result??;
    grpc_result??;

    Ok(())
}

/// Run the shared HTTP and gRPC transports until the supplied shutdown receiver is triggered.
/// 运行共享 HTTP 与 gRPC 传输层，直到提供的关闭接收器被触发。
async fn run_network_transports_with_shutdown(
    server: HostRuntime,
    cfg: &Config,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let http_addr = cfg
        .http
        .clone()
        .unwrap_or_else(|| "127.0.0.1:19201".to_string());
    let grpc_addr = cfg
        .grpc
        .clone()
        .unwrap_or_else(|| "127.0.0.1:19202".to_string());

    let server_for_http = server.clone();
    let server_for_grpc = server.clone();
    let http_shutdown_rx = shutdown_rx.clone();
    let grpc_shutdown_rx = shutdown_rx;
    let http_task = tokio::spawn(async move {
        transport::http::run_http_with_shutdown(server_for_http, &http_addr, http_shutdown_rx)
            .await
            .map_err(|error| format!("[HTTP] {error}"))
    });
    let grpc_task = tokio::spawn(async move {
        transport::grpc::run_grpc_with_shutdown(server_for_grpc, &grpc_addr, grpc_shutdown_rx)
            .await
            .map_err(|error| format!("[gRPC] {error}"))
    });
    let (http_result, grpc_result) = tokio::join!(http_task, grpc_task);
    http_result??;
    grpc_result??;
    Ok(())
}

/// Initialize Lua skills and invoke the target tool directly without starting transports.
/// 在不启动服务的情况下，直接初始化 Lua skill 并调用目标 tool，便于调试技能加载、依赖初始化与实际返回值。
/// This mode always uses a single VM and simulates a complete MCP request context.
/// 该模式固定使用单 VM，并模拟一个完整的 MCP 请求上下文。
fn run_call_tool_mode(
    tool_name: &str,
    arguments: Value,
    simulated_client_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    let config = Config::load()?;
    initialize_runtime_temp_root_from_config(&config)?;
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&config)?;
    add_libs_to_path(&config)?;
    if is_host_tool_name(tool_name) {
        return run_call_host_tool_mode(config, tool_name, arguments, simulated_client_name);
    }
    let engine = build_single_vm_lua_engine_for_local_mode(&config)?;

    if !engine.is_skill(tool_name) {
        return run_call_host_tool_mode(config, tool_name, arguments, simulated_client_name);
    }

    let skill_name = engine.skill_name_for_tool(tool_name);
    let mcp_request_context = build_call_tool_request_context(simulated_client_name);
    let request_context = runtime_context_from_mcp(&mcp_request_context);
    let invocation_context = build_runtime_invocation_context(
        Some(&request_context),
        Some(tool_name),
        skill_name.as_deref(),
    );
    let result = engine
        .call_skill(tool_name, &arguments, Some(&invocation_context))
        .map_err(|error| format!("call-tools failed for {}: {}", tool_name, error))?;

    print_call_tools_result(
        &result,
        skill_name.as_deref(),
        Some(tool_name),
        Some(&request_context),
    )
}

/// Invoke one host-owned MCP tool through the full server path during `--call-tools` local debug mode.
/// 在 `--call-tools` 本地调试模式下，经由完整服务路径调用单个宿主自有 MCP 工具。
fn run_call_host_tool_mode(
    config: Config,
    tool_name: &str,
    arguments: Value,
    simulated_client_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let server = if is_host_tool_name(tool_name) && !host_tool_requires_lua_engine(tool_name) {
        build_host_tool_surface_server(&config)?
    } else {
        runtime.block_on(async_build_stdio_server(config))?
    };
    // Route the local JSON-RPC simulation through the MCP dispatcher boundary.
    // 通过 MCP dispatcher 边界路由本地 JSON-RPC 模拟调用。
    let dispatcher = McpDispatcher::new(server);
    let request_context = build_call_tool_request_context(simulated_client_name);
    let response = runtime
        .block_on(dispatcher.handle_message_with_context(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments,
                }
            }),
            request_context,
        ))
        .ok_or_else(|| format!("call-tools returned no response for {}", tool_name))?;
    print_host_call_tool_response(tool_name, &response)
}

/// Print one host-tool `tools/call` JSON-RPC response in the same local-debug workflow used by `--call-tools`.
/// 按 `--call-tools` 使用的同一本地调试工作流打印一份宿主工具 `tools/call` JSON-RPC 响应。
fn print_host_call_tool_response(
    tool_name: &str,
    response: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown host tool error");
        return Err(format!("call-tools failed for {}: {}", tool_name, message).into());
    }

    let result_value = response
        .get("result")
        .cloned()
        .ok_or_else(|| format!("call-tools returned no result payload for {}", tool_name))?;
    let tool_result: ToolCallResult = serde_json::from_value(result_value)?;
    let rendered_text = tool_result
        .content
        .iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    if tool_result.is_error == Some(true) {
        return Err(format!("call-tools failed for {}: {}", tool_name, rendered_text).into());
    }

    println!("{}", rendered_text);
    Ok(())
}

/// Internal luaexec subprocess mode that initializes the full local runtime before executing one isolated request.
/// 内部 luaexec 子进程模式，按本地完整运行时初始化后执行单次隔离请求。
fn run_internal_luaexec_request_mode(request_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    let config = Config::load()?;
    initialize_runtime_temp_root_from_config(&config)?;
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&config)?;
    add_libs_to_path(&config)?;

    let request_json = std::fs::read_to_string(request_file)?;
    let engine = build_single_vm_lua_engine_for_local_mode(&config)?;
    let rendered = engine
        .execute_runlua_request_json_inline(&request_json)
        .map_err(|error| format!("internal luaexec failed: {}", error))?;
    println!("{}", rendered);
    Ok(())
}

#[cfg(test)]
mod tests;
