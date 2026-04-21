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
    build_luaskills_cache_config, build_luaskills_engine_options, build_runtime_invocation_context,
    client_budget_snapshot_for_render, install_luaskills_log_callback,
    resolve_runtime_root_from_config, resolve_skill_roots_from_config,
};
use protocol::{ClientInfo, PROTOCOL_VERSION_LATEST, RequestContext};
use runtime_logging::{info as log_info, set_non_error_logging_enabled};
use serde_json::{Value, json};
use server::McpServer;
use temp_maintenance::{
    CleanupTrigger, ensure_runtime_temp_dir, initialize_runtime_temp_root,
    maintain_runtime_temp_dir, spawn_cross_day_cleanup_task,
};
use tool_config::preload_tool_configs;
use tool_result_format::{HostRenderOptions, RuntimeInvocationResult, render_tool_result_text};
use vulcan_luaskills::{LuaEngine, LuaVmPoolConfig};

/// Print the final `--call-tools` result.
/// 输出 `--call-tools` 的最终结果。
/// The debug mode also injects a simulated client context, so budget resolution must use the same request context.
/// 调试模式同样会注入模拟客户端上下文，因此预算解析也必须基于同一份请求上下文完成。
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

/// Format the resolved client-budget preview into startup logs that are easy for humans to read directly.
/// 把客户端预算预解析摘要格式化成人可直接阅读的启动日志。
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

/// Format the preloaded tool-config summary into startup logs that are directly readable by humans.
/// 把工具配置预载摘要格式化成人可直接阅读的启动日志。
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
            let cfg = Config::load()?;
            add_libs_to_path(&cfg);
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async_main(cfg))
        }
    }
}

/// Async main flow that decides between starting network services and entering direct tool-debug mode.
/// 异步主流程，根据运行模式决定是启动网络服务还是直接进入 tools 调试。
async fn async_main(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    install_luaskills_log_callback();
    initialize_runtime_temp_root(resolve_runtime_root_from_config(&cfg).as_deref());

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;

    let server = build_server(&cfg).await?;

    spawn_cross_day_cleanup_task();

    run_network_transports(server, &cfg).await?;

    Ok(())
}

/// Command-line runtime mode.
/// 命令行运行模式。
enum RuntimeMode {
    /// Start the regular HTTP/gRPC services.
    /// 正常启动 HTTP/gRPC 服务。
    Serve,
    /// Initialize the tool runtime only and directly invoke a single tool for local debugging.
    /// 仅初始化工具运行环境，并直接调用单个 tool 做本地调试。
    /// This mode simulates a fixed client context, does not read `config.yaml`, and does not open any ports.
    /// 该模式会模拟一个固定客户端上下文，不读取 `config.yaml`，也不启动任何端口。
    CallTool {
        tool_name: String,
        arguments: Value,
        simulated_client_name: String,
    },
    /// Internal-only luaexec subprocess execution mode.
    /// 内部专用的 luaexec 子进程执行模式。
    InternalLuaexecRequest { request_file: String },
}

/// Default simulated client name used by the `--call-tools` debug mode.
/// `--call-tools` 调试模式使用的默认模拟客户端名称。
const DEFAULT_CALL_TOOL_CLIENT_NAME: &str = "VulcanMcpTest";

/// Parse the runtime mode from CLI arguments.
/// 根据命令行参数解析运行模式。
/// Supported forms:
/// 支持以下形式：
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-client-name <name>`: set the simulated client name
/// - `--call-client-name <name>`：指定模拟客户端名称
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
                        return Err(format!("Unknown --call-tools flag: {}", value).into());
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

/// Build and initialize the MCP server, including external clients, Lua skills, and shared cache.
/// 构建并初始化 MCP Server，包括外部客户端、Lua Skills 与共享缓存。
async fn build_server(cfg: &Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();

    // Connect VMM gRPC client if configured.
    if let Some(endpoint) = &cfg.vmm {
        server = server.with_vmm(endpoint).await?;
    }

    // Load Lua skills from system directory, with optional user override
    let skill_roots = find_skill_roots(&cfg)?;
    if !skill_roots.is_empty() {
        server = server.with_lua_skills(
            cfg,
            &skill_roots,
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

/// Run the default HTTP/gRPC service mode.
/// 运行默认的 HTTP/gRPC 服务模式。
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
    initialize_runtime_temp_root(resolve_runtime_root_from_config(&config).as_deref());
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;
    add_libs_to_path(&config);
    let engine = build_single_vm_lua_engine_for_local_mode(&config)?;

    if !engine.is_skill(tool_name) {
        return Err(format!("Unknown Lua skill tool for --call-tools: {}", tool_name).into());
    }

    let skill_name = engine.skill_name_for_tool(tool_name);
    let request_context = build_call_tool_request_context(simulated_client_name);
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

/// Build a single-VM LuaEngine with fully loaded skills from the unified runtime root for local debug modes.
/// 在本地调试模式下基于统一运行根构建一个完整加载 skills 的单虚拟机 LuaEngine。
fn build_single_vm_lua_engine_for_local_mode(
    config: &Config,
) -> Result<LuaEngine, Box<dyn std::error::Error>> {
    let skill_roots = find_skill_roots(config)?;
    if skill_roots.is_empty() {
        return Err("Lua skill directory not found for local debug mode".into());
    }

    let mut engine = LuaEngine::new(build_luaskills_engine_options(
        config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )?)?;
    engine.load_from_roots(&skill_roots)?;
    Ok(engine)
}

/// Internal luaexec subprocess mode that initializes the full local runtime before executing one isolated request.
/// 内部 luaexec 子进程模式，按本地完整运行时初始化后执行单次隔离请求。
fn run_internal_luaexec_request_mode(request_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    let config = Config::load()?;
    initialize_runtime_temp_root(resolve_runtime_root_from_config(&config).as_deref());
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs()?;
    add_libs_to_path(&config);

    let request_json = std::fs::read_to_string(request_file)?;
    let engine = build_single_vm_lua_engine_for_local_mode(&config)?;
    let rendered = engine
        .execute_runlua_request_json_inline(&request_json)
        .map_err(|error| format!("internal luaexec failed: {}", error))?;
    println!("{}", rendered);
    Ok(())
}

/// Find the ordered skill-root chain for the default runtime environment.
/// 查找默认运行环境使用的有序技能根目录覆盖链。
fn find_skill_roots(
    cfg: &config::Config,
) -> Result<Vec<vulcan_luaskills::RuntimeSkillRoot>, Box<dyn std::error::Error>> {
    resolve_skill_roots_from_config(cfg)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
}

/// Build a simulated request context for `--call-tools` that stays close to a real MCP request.
/// 为 `--call-tools` 构造尽量贴近真实 MCP 请求的模拟上下文。
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

/// Preload hot-reloadable runtime config files before the host starts so configuration issues surface before the first request.
/// 在宿主启动前预载可热重载的运行时配置文件，避免首次请求时才暴露配置问题。
fn preload_runtime_mcp_configs() -> Result<(), Box<dyn std::error::Error>> {
    let client_budget_report = preload_client_budget_config()
        .map_err(|error| format!("Failed to preload client budgets: {}", error))?;
    let tool_config_report = preload_tool_configs()
        .map_err(|error| format!("Failed to preload tool configs: {}", error))?;

    print_client_budget_preload_log(&client_budget_report);
    print_tool_config_preload_log(&tool_config_report);
    Ok(())
}

/// Prepend runtime-root libs/ to PATH so C dependency DLLs (zlib1.dll, etc.) are discoverable when Lua C modules load via FFI.
/// 将运行根下的 libs/ 前置到 PATH，保证 Lua C 模块通过 FFI 加载时能找到依赖 DLL。
fn add_libs_to_path(config: &Config) {
    let Some(runtime_root) = resolve_runtime_root_from_config(config) else {
        return;
    };
    let libs_dir = runtime_root.join("libs");

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
