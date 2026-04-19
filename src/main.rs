mod client_budget;
mod config;
#[allow(dead_code)]
mod grpc_client;
mod grpc_server;
mod http_server;
mod luaskills_host;
#[allow(dead_code)]
mod protocol;
mod runtime_logging;
mod server;
#[allow(dead_code)]
mod session;
mod temp_maintenance;
mod tool_config;
mod tool_result_format;

pub mod pb_vmm {
    tonic::include_proto!("vmm.v1");
}

pub mod pb_mcp {
    tonic::include_proto!("vulcan.mcp.v1");
}

use client_budget::preload_client_budget_config;
use config::Config;
use luaskills_host::{
    build_luaskills_cache_config, build_luaskills_engine_options,
    build_runtime_invocation_context, client_budget_snapshot_for_render, install_luaskills_log_callback,
};
use protocol::{ClientInfo, PROTOCOL_VERSION_LATEST, RequestContext};
use runtime_logging::{info as log_info, set_non_error_logging_enabled};
use serde_json::{Value, json};
use server::McpServer;
use temp_maintenance::{
    CleanupTrigger, ensure_runtime_temp_dir, maintain_runtime_temp_dir,
    spawn_cross_day_cleanup_task,
};
use tool_config::preload_tool_configs;
use tool_result_format::{HostRenderOptions, RuntimeInvocationResult, render_tool_result_text};
use vulcan_luaskills::{LuaEngine, LuaVmPoolConfig};

/// 中文：输出 `--call-tools` 的最终结果。
/// 调试模式同样会注入模拟客户端上下文，因此预算解析也必须基于同一份请求上下文完成。
/// English: Print the final `--call-tools` result.
/// The debug mode also injects a simulated client context, so budget resolution must use the same request context.
fn print_call_tools_result(
    value: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    tool_name: Option<&str>,
    request_context: Option<&RequestContext>,
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
            },
        )
    );
    Ok(())
}

/// 中文：把客户端预算预解析摘要格式化成人可直接阅读的启动日志。
/// English: Format the resolved client-budget preview into startup logs that are easy for humans to read directly.
fn print_client_budget_preload_log(report: &client_budget::ClientBudgetLoadReport) {
    for (client_pattern, preview) in &report.resolved_previews {
        let Some(scope_object) = preview.as_object() else {
            continue;
        };
        for (scope_name, scope_value) in scope_object {
            let Some(scope_detail) = scope_value.as_object() else {
                continue;
            };
            let bytes = scope_detail
                .get("bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let source_summary = scope_detail
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let raw_tokens = scope_detail.get("raw_tokens").and_then(Value::as_i64);
            let raw_bytes = scope_detail.get("raw_bytes").and_then(Value::as_i64);
            let raw_lines = scope_detail.get("raw_lines").and_then(Value::as_i64);

            let mut output_parts = Vec::new();
            if let Some(tokens) = raw_tokens {
                let converted_bytes = if tokens == -1 {
                    report.estimation.unlimited_bytes_cap
                } else {
                    (tokens.max(0) as u64).saturating_mul(report.estimation.bytes_per_token)
                };
                let displayed_bytes = if raw_bytes.is_some() {
                    converted_bytes
                } else {
                    bytes
                };
                output_parts.push(format!(
                    "tokens:{} rate:{} => bytes:{}",
                    tokens, report.estimation.bytes_per_token, displayed_bytes
                ));
            }
            if let Some(raw_bytes_value) = raw_bytes {
                output_parts.push(format!("bytes:{}", raw_bytes_value));
                if raw_bytes_value < 0 {
                    output_parts.push(format!("effective_bytes:{}", bytes));
                }
            } else if raw_tokens.is_none() {
                output_parts.push(format!("bytes:{}", bytes));
            }
            if let Some(raw_lines_value) = raw_lines {
                output_parts.push(format!("lines:{}", raw_lines_value));
            }
            if raw_tokens.is_some() && raw_bytes.is_some() && raw_bytes != Some(bytes as i64) {
                output_parts.push(format!("effective_bytes:{}", bytes));
            }

            log_info(format!(
                "[mcp_output_limit]client:{} {}({}) src={}",
                client_pattern,
                scope_name,
                output_parts.join(", "),
                source_summary
            ));
        }
    }
}

/// 中文：把工具配置预载摘要格式化成人可直接阅读的启动日志。
/// English: Format the preloaded tool-config summary into startup logs that are directly readable by humans.
fn print_tool_config_preload_log(report: &tool_config::ToolConfigLoadReport) {
    if report.tool_count == 0 {
        log_info("[tools_config]loaded none configs,count=0");
        return;
    }

    for tool_name in &report.tool_names {
        let count = report.config_counts.get(tool_name).copied().unwrap_or(0);
        log_info(format!(
            "[tools_config]loaded {} configs,count={}",
            tool_name, count
        ));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_mode = parse_runtime_mode()?;
    match runtime_mode {
        RuntimeMode::CallTool {
            tool_name,
            arguments,
            simulated_client_name,
        } => run_call_tool_mode(&tool_name, arguments, &simulated_client_name),
        RuntimeMode::InternalLuaexecRequest { request_file } => {
            run_internal_luaexec_request_mode(&request_file)
        }
        RuntimeMode::Serve => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async_main())
        }
    }
}

/// 中文：异步主流程，根据运行模式决定是启动网络服务还是直接进入 tools 调试。
/// English: Async main flow that decides between starting network services and entering direct tool-debug mode.
async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load()?;

    install_luaskills_log_callback();

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;

    // Prepend output/libs/ to PATH so C dependency DLLs are found at runtime
    add_libs_to_path();

    let server = build_server(&cfg).await?;

    spawn_cross_day_cleanup_task();

    run_network_transports(server, &cfg).await?;

    Ok(())
}

/// 中文：命令行运行模式。
/// English: Command-line runtime mode.
enum RuntimeMode {
    /// 中文：正常启动 HTTP/gRPC 服务。
    /// English: Start the regular HTTP/gRPC services.
    Serve,
    /// 中文：仅初始化工具运行环境，并直接调用单个 tool 做本地调试。
    /// 该模式会模拟一个固定客户端上下文，不读取 `config.yaml`，也不启动任何端口。
    /// English: Initialize the tool runtime only and directly invoke a single tool for local debugging.
    /// This mode simulates a fixed client context, does not read `config.yaml`, and does not open any ports.
    CallTool {
        tool_name: String,
        arguments: Value,
        simulated_client_name: String,
    },
    /// 中文：内部专用的 luaexec 子进程执行模式。
    /// English: Internal-only luaexec subprocess execution mode.
    InternalLuaexecRequest { request_file: String },
}

/// 中文：`--call-tools` 调试模式使用的默认模拟客户端名称。
/// English: Default simulated client name used by the `--call-tools` debug mode.
const DEFAULT_CALL_TOOL_CLIENT_NAME: &str = "VulcanMcpTest";

/// 中文：根据命令行参数解析运行模式。
/// 支持：
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-client-name <name>`：指定模拟客户端名称
/// English: Parse the runtime mode from CLI arguments.
/// Supported forms:
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-client-name <name>`: set the simulated client name
fn parse_runtime_mode() -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    for index in 0..args.len() {
        if args[index] == "--internal-luaexec-request" {
            let request_file = args
                .get(index + 1)
                .ok_or("--internal-luaexec-request requires a file path")?
                .clone();
            return Ok(RuntimeMode::InternalLuaexecRequest { request_file });
        }
    }
    for index in 0..args.len() {
        if args[index] == "--call-tools" {
            let tool_name = args
                .get(index + 1)
                .ok_or("--call-tools requires a tool name")?
                .clone();

            let mut arguments = json!({});
            let mut simulated_client_name = DEFAULT_CALL_TOOL_CLIENT_NAME.to_string();
            let mut cursor = index + 2;
            while cursor < args.len() {
                match args[cursor].as_str() {
                    "--call-client-name" => {
                        let client_name = args
                            .get(cursor + 1)
                            .ok_or("--call-client-name requires a value")?;
                        simulated_client_name = client_name.clone();
                        cursor += 2;
                    }
                    "--call-tools" => {
                        break;
                    }
                    "-config" | "--config" => {
                        cursor += 2;
                    }
                    value if value.starts_with("--") => {
                        return Err(format!(
                            "Unknown --call-tools flag: {} / 未知的 --call-tools 调试参数: {}",
                            value, value
                        )
                        .into());
                    }
                    raw_json => {
                        arguments = serde_json::from_str::<Value>(raw_json)?;
                        cursor += 1;
                    }
                }
            }

            return Ok(RuntimeMode::CallTool {
                tool_name,
                arguments,
                simulated_client_name,
            });
        }
    }

    Ok(RuntimeMode::Serve)
}

/// 中文：构建并初始化 MCP Server，包括外部客户端、Lua Skills 与共享缓存。
/// English: Build and initialize the MCP server, including external clients, Lua skills, and shared cache.
async fn build_server(cfg: &Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();

    // Connect VMM gRPC client if configured.
    if let Some(endpoint) = &cfg.vmm {
        server = server.with_vmm(endpoint).await?;
    }

    // Load Lua skills from system directory, with optional user override
    let lua_skills_loaded = find_lua_skill_dirs(&cfg);
    if let Some((base_dir, override_dir)) = lua_skills_loaded {
        server = server.with_lua_skills(
            cfg,
            &base_dir,
            override_dir.as_deref(),
            LuaVmPoolConfig {
                min_size: cfg.lua_vm_pool_min_size.unwrap_or(1),
                max_size: cfg.lua_vm_pool_max_size.unwrap_or(4),
                idle_ttl_secs: cfg.lua_vm_pool_idle_ttl_secs.unwrap_or(300),
            },
            build_luaskills_cache_config(
                cfg.tool_cache_max_entries,
                cfg.tool_cache_default_ttl_secs,
                cfg.tool_cache_max_ttl_secs,
            ),
        )?;
    }

    Ok(server)
}

/// 中文：运行默认的 HTTP/gRPC 服务模式。
/// English: Run the default HTTP/gRPC service mode.
async fn run_network_transports(
    server: McpServer,
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
        http_server::run_http(server_for_http, &http_addr)
            .await
            .map_err(|e| format!("[HTTP] {e}"))
    });
    let grpc_task = tokio::spawn(async move {
        grpc_server::run_grpc(server_for_grpc, &grpc_addr)
            .await
            .map_err(|e| format!("[gRPC] {e}"))
    });

    // Wait for either to finish (they run until shutdown)
    let (http_result, grpc_result) = tokio::join!(http_task, grpc_task);
    http_result??;
    grpc_result??;

    Ok(())
}

/// 中文：在不启动服务的情况下，直接初始化 Lua skill 并调用目标 tool，便于调试技能加载、依赖初始化与实际返回值。
/// 该模式固定使用单 VM，并模拟一个完整的 MCP 请求上下文。
/// English: Initialize Lua skills and invoke the target tool directly without starting transports.
/// This mode always uses a single VM and simulates a complete MCP request context.
fn run_call_tool_mode(
    tool_name: &str,
    arguments: Value,
    simulated_client_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;

    add_libs_to_path();
    let engine = build_single_vm_lua_engine_for_local_mode()?;

    if !engine.is_skill(tool_name) {
        return Err(format!("Unknown Lua skill tool for --call-tools: {}", tool_name).into());
    }

    let skill_name = engine.skill_name_for_tool(tool_name);
    let request_context = build_call_tool_request_context(simulated_client_name);
    let invocation_context =
        build_runtime_invocation_context(Some(&request_context), Some(tool_name), skill_name.as_deref());
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

/// 中文：在本地调试模式下构建一个完整加载 skills 的单虚拟机 LuaEngine。
/// English: Build a single-VM LuaEngine with fully loaded skills for local debug modes.
fn build_single_vm_lua_engine_for_local_mode() -> Result<LuaEngine, Box<dyn std::error::Error>> {
    let (base_dir, override_dir) = find_lua_skill_dirs_for_call_tools()
        .ok_or("Lua skill directory not found for local debug mode")?;
    let config = Config::load()?;

    let mut engine = LuaEngine::new(build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )?)?;
    engine.load_from_dirs(&base_dir, override_dir.as_deref())?;
    Ok(engine)
}

/// 中文：内部 luaexec 子进程模式，按本地完整运行时初始化后执行单次隔离请求。
/// English: Internal luaexec subprocess mode that initializes the full local runtime before executing one isolated request.
fn run_internal_luaexec_request_mode(request_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;

    add_libs_to_path();

    let request_json = std::fs::read_to_string(request_file)?;
    let engine = build_single_vm_lua_engine_for_local_mode()?;
    let rendered = engine
        .execute_runlua_request_json_inline(&request_json)
        .map_err(|error| format!("internal luaexec failed: {}", error))?;
    println!("{}", rendered);
    Ok(())
}

/// Find Lua skill base and override directories.
/// Returns (base_dir, Option<override_dir>) if skills exist.
fn find_lua_skill_dirs(
    cfg: &config::Config,
) -> Option<(std::path::PathBuf, Option<std::path::PathBuf>)> {
    // Base directory: <exe_parent>/lua_skills/
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let base_dir = parent.join("lua_skills");

    if !base_dir.exists() {
        return None;
    }

    // Override directory: from config or default ~/.vulcan/vulcan-mcp/lua_skills/
    let override_dir = cfg.lua_skills_override.clone().or_else(|| {
        let home = home_dir()?;
        Some(
            home.join(".vulcan/vulcan-mcp/lua_skills")
                .to_string_lossy()
                .to_string(),
        )
    });

    let override_path = override_dir.and_then(|p| {
        let path = std::path::PathBuf::from(p);
        if path.exists() { Some(path) } else { None }
    });

    Some((base_dir, override_path))
}

/// 中文：为 `--call-tools` 构造尽量贴近真实 MCP 请求的模拟上下文。
/// English: Build a simulated request context for `--call-tools` that stays close to a real MCP request.
fn build_call_tool_request_context(client_name: &str) -> RequestContext {
    RequestContext {
        transport: Some("call_tools".to_string()),
        session_id: Some("call-tools-local".to_string()),
        protocol_version: Some(PROTOCOL_VERSION_LATEST.to_string()),
        client_info: Some(ClientInfo {
            name: client_name.trim().to_string(),
            version: "local-debug".to_string(),
        }),
        client_capabilities: json!({}),
    }
}

/// 中文：`--call-tools` 调试模式专用的 skill 目录查找逻辑。
/// 该模式不读取 `config.yaml`，只使用运行时输出目录与默认用户覆盖目录。
/// English: Dedicated skill-directory discovery for `--call-tools`.
/// This mode does not read `config.yaml`; it only uses the runtime output directory and the default user override directory.
fn find_lua_skill_dirs_for_call_tools() -> Option<(std::path::PathBuf, Option<std::path::PathBuf>)>
{
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let runtime_output_dir = parent.join("lua_skills");
    let repository_dir = std::path::Path::new("runtime").join("lua_skills");
    let base_dir = if runtime_output_dir.exists() {
        runtime_output_dir
    } else if repository_dir.exists() {
        repository_dir
    } else {
        return None;
    };

    let override_path = home_dir().and_then(|home| {
        let path = home.join(".vulcan/vulcan-mcp/lua_skills");
        if path.exists() { Some(path) } else { None }
    });

    Some((base_dir, override_path))
}

/// 中文：在宿主启动前预载可热重载的运行时配置文件，避免首次请求时才暴露配置问题。
/// English: Preload hot-reloadable runtime config files before the host starts so configuration issues surface before the first request.
fn preload_runtime_mcp_configs() -> Result<(), Box<dyn std::error::Error>> {
    let client_budget_report = preload_client_budget_config()
        .map_err(|error| format!("Failed to preload client budgets: {}", error))?;
    let tool_config_report = preload_tool_configs()
        .map_err(|error| format!("Failed to preload tool configs: {}", error))?;

    print_client_budget_preload_log(&client_budget_report);
    print_tool_config_preload_log(&tool_config_report);
    Ok(())
}

/// Prepend output/libs/ to PATH so C dependency DLLs (zlib1.dll, etc.)
/// are discoverable when Lua C modules load via FFI.
fn add_libs_to_path() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe_path.parent() else {
        return;
    };
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let libs_dir = parent.join("libs");

    if !libs_dir.exists() {
        return;
    }

    let libs_str = libs_dir.to_string_lossy().to_string();
    let current_path = std::env::var("PATH").unwrap_or_default();

    #[cfg(windows)]
    let separator = ";";
    #[cfg(not(windows))]
    let separator = ":";

    let new_path = format!("{}{}{}", libs_str, separator, current_path);
    unsafe {
        std::env::set_var("PATH", new_path);
    }
}

#[cfg(target_os = "windows")]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}
