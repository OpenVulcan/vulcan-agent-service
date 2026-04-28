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
mod stdio_server;
mod temp_maintenance;
mod tool_config;
mod tool_result_format;

pub mod pb_vmm {
    tonic::include_proto!("vmm.v1");
}

pub mod pb_mcp {
    tonic::include_proto!("vulcan.mcp.v1");
}

use client_budget::{initialize_client_budget_runtime_root, preload_client_budget_config};
use config::Config;
use luaskills::{
    LuaEngine, LuaRuntimeHostOptions, LuaVmPoolConfig, RuntimeSkillRoot, SkillApplyResult,
    SkillInstallRequest, SkillInstallSourceType, SkillManagementAuthority, SkillManager,
    SkillManagerConfig,
};
use luaskills_host::{
    build_luaskills_cache_config, build_luaskills_engine_options, build_runtime_invocation_context,
    client_budget_snapshot_for_render, default_user_skill_root, install_luaskills_log_callback,
    normalize_skill_root_key, resolve_runtime_root_from_config, resolve_skill_config_file_path,
    resolve_skill_roots_from_config, validate_unique_skill_root_spaces,
};
use protocol::{ClientInfo, PROTOCOL_VERSION_LATEST, RequestContext, ToolCallResult};
use runtime_logging::{info as log_info, set_non_error_logging_enabled};
use serde_json::{Value, json};
use server::{McpServer, host_tool_requires_lua_engine, is_host_tool_name};
use std::fmt::Write as _;
use temp_maintenance::{
    CleanupTrigger, ensure_runtime_temp_dir, initialize_runtime_temp_root,
    maintain_runtime_temp_dir, spawn_cross_day_cleanup_task,
};
use tool_config::{initialize_tool_config_runtime_root, preload_tool_configs};
use tool_result_format::{
    HostRenderOptions, RuntimeInvocationResult, initialize_tool_result_template_roots,
    render_tool_result_text,
};

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
                ..HostRenderOptions::default()
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
        RuntimeMode::RootSkillInstall {
            source,
            source_type,
        } => run_root_skill_install_mode(&source, source_type),
        RuntimeMode::RootSkillsUpdate => run_root_skills_update_mode(),
        RuntimeMode::InternalLuaexecRequest { request_file } => {
            run_internal_luaexec_request_mode(&request_file)
        }
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
            runtime.block_on(async_main(cfg))
        }
    }
}

/// Resolve the effective runtime root for one host execution path and surface explicit runtime-root misconfiguration as an immediate error.
/// 为单条宿主执行链解析生效运行根，并把显式 runtime_root 配置错误立即上抛。
fn resolve_runtime_root_for_host(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    resolve_runtime_root_from_config(config)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
}

/// Resolve the explicit unified runtime skill-config file path used by host-owned luaskill-config operations.
/// 解析宿主自有 luaskill-config 操作使用的显式统一运行时 Skill 配置文件路径。
fn resolve_runtime_skill_config_file_path_for_host(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let Some(runtime_root) = resolve_runtime_root_for_host(config)? else {
        return Ok(None);
    };
    let file_path = resolve_skill_config_file_path(&runtime_root)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    Ok(Some(file_path))
}

/// Build one host-only MCP server surface and inject luaskill-config when the runtime root is available.
/// 构建一份仅含宿主工具面的 MCP 服务，并在运行根可用时注入 luaskill-config。
fn build_host_tool_surface_server(
    config: &Config,
) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();
    if let Some(skill_config_file_path) = resolve_runtime_skill_config_file_path_for_host(config)? {
        server = server.with_runtime_skill_config_file_path(skill_config_file_path);
    }
    Ok(server)
}

/// Initialize the shared runtime temp root from config after runtime-root validation has completed.
/// 在完成运行根校验后，基于配置初始化共享运行时临时目录根。
fn initialize_runtime_temp_root_from_config(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(config)?;
    initialize_runtime_temp_root(runtime_root.as_deref());
    Ok(runtime_root)
}

/// Async main flow that decides between starting network services and entering direct tool-debug mode.
/// 异步主流程，根据运行模式决定是启动网络服务还是直接进入 tools 调试。
async fn async_main(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    install_luaskills_log_callback();
    initialize_runtime_temp_root_from_config(&cfg)?;

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&cfg)?;

    let server = build_server(&cfg).await?;

    spawn_cross_day_cleanup_task();

    run_network_transports(server, &cfg).await?;

    Ok(())
}

/// Async stdio bootstrap flow that prepares one initialized server without starting network transports.
/// 为 stdio 模式准备一份已初始化服务且不启动网络传输的异步引导流程。
async fn async_build_stdio_server(cfg: Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    install_luaskills_log_callback();
    initialize_runtime_temp_root_from_config(&cfg)?;

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&cfg)?;

    build_server(&cfg).await
}

/// Async stdio serving flow that runs one already prepared server on stdin/stdout only.
/// 仅通过标准输入输出运行一份已准备服务实例的异步 stdio 服务流程。
async fn async_run_stdio_server(server: McpServer) -> Result<(), Box<dyn std::error::Error>> {
    spawn_cross_day_cleanup_task();
    stdio_server::run_stdio(server).await
}

/// Command-line runtime mode.
/// 命令行运行模式。
enum RuntimeMode {
    /// Start the MCP server on stdio using Content-Length framed JSON-RPC.
    /// 使用 Content-Length 分帧 JSON-RPC 的 stdio 方式启动 MCP 服务。
    Stdio,
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
    /// Install one managed LuaSkill into the host-controlled ROOT layer without starting MCP transports.
    /// 在不启动 MCP 传输服务的情况下，将单个受管 LuaSkill 安装到宿主控制的 ROOT 层。
    RootSkillInstall {
        /// Source locator such as `LuaSkills/vulcan-codekit` or a GitHub repository URL.
        /// 来源定位值，例如 `LuaSkills/vulcan-codekit` 或 GitHub 仓库 URL。
        source: String,
        /// Optional source type override parsed from the CLI.
        /// 从 CLI 解析得到的可选来源类型覆盖。
        source_type: Option<SkillInstallSourceType>,
    },
    /// Update every managed LuaSkill declared in the host-controlled ROOT layer without starting MCP transports.
    /// 在不启动 MCP 传输服务的情况下，更新宿主控制的 ROOT 层内所有受管 LuaSkill。
    RootSkillsUpdate,
    /// Internal-only luaexec subprocess execution mode.
    /// 内部专用的 luaexec 子进程执行模式。
    InternalLuaexecRequest { request_file: String },
}

/// Default simulated client name used by the `--call-tools` debug mode.
/// `--call-tools` 调试模式使用的默认模拟客户端名称。
const DEFAULT_CALL_TOOL_CLIENT_NAME: &str = "VulcanMcpTest";

/// Runtime state required by local ROOT skill-management commands.
/// 本地 ROOT 技能管理命令所需的运行时状态。
struct RootSkillCliContext {
    /// Single-VM LuaSkills engine used to execute lifecycle operations in-process.
    /// 用于在当前进程内执行生命周期操作的单 VM LuaSkills 引擎。
    engine: LuaEngine,
    /// Fully resolved formal skill-root chain used for lifecycle preflight checks.
    /// 用于生命周期预检查的完整正式技能根链。
    skill_roots: Vec<RuntimeSkillRoot>,
    /// Concrete ROOT target selected from the formal skill-root chain.
    /// 从正式技能根链中选出的具体 ROOT 目标。
    target_root: RuntimeSkillRoot,
    /// Host options cloned from the engine configuration for record inspection.
    /// 从引擎配置中克隆出的宿主选项，用于检查安装记录。
    host_options: LuaRuntimeHostOptions,
}

/// Return whether one raw CLI token still uses the removed `--config` / `-config` entrypoint, including inline `--config=...` forms.
/// 返回某个原始 CLI 片段是否仍在使用已移除的 `--config` / `-config` 入口，包含内联 `--config=...` 形式。
fn is_removed_config_flag_arg(arg: &str) -> bool {
    arg == "-config"
        || arg == "--config"
        || arg.starts_with("-config=")
        || arg.starts_with("--config=")
}

/// Return whether one raw CLI token carries an inline `--runtime-root=value` or `-runtime-root=value` assignment.
/// 返回某个原始 CLI 片段是否携带内联 `--runtime-root=value` 或 `-runtime-root=value` 赋值。
fn is_inline_runtime_root_flag_arg(arg: &str) -> bool {
    arg.starts_with("-runtime-root=") || arg.starts_with("--runtime-root=")
}

/// Parse the runtime mode from CLI arguments.
/// 根据命令行参数解析运行模式。
/// Supported forms:
/// 支持以下形式：
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-client-name <name>`: set the simulated client name
/// - `--call-client-name <name>`：指定模拟客户端名称
/// - `--install-root-skill <source> [--source-type github|url]`
/// - `--install-root-skill <source> [--source-type github|url]`
/// - `--update-root-skills`
/// - `--update-root-skills`
fn parse_runtime_mode() -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    parse_runtime_mode_from_args(&args)
}

/// Parse the runtime mode from an explicit argv slice so CLI behavior stays unit-testable.
/// 从显式 argv 切片解析运行模式，以保证 CLI 行为可被单元测试覆盖。
fn parse_runtime_mode_from_args(
    args: &[String],
) -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    for index in 0..args.len() {
        if args[index] == "--internal-luaexec-request" {
            let request_file = args
                .get(index + 1)
                .ok_or("--internal-luaexec-request requires a file path")?
                .clone();
            return Ok(RuntimeMode::InternalLuaexecRequest { request_file });
        }
    }
    for argument in args {
        if argument == "--stdio" {
            return Ok(RuntimeMode::Stdio);
        }
    }
    for index in 0..args.len() {
        if args[index] == "--install-root-skill" {
            // Extract the install source before parsing command-local optional flags.
            // 在解析命令局部可选标志前提取安装来源。
            let source = args
                .get(index + 1)
                .filter(|value| !value.starts_with('-'))
                .ok_or("--install-root-skill requires a source")?
                .clone();
            // Parse the optional install source type after the source locator.
            // 在来源定位值之后解析可选安装来源类型。
            let source_type = parse_root_skill_install_source_type_from_args(args, index + 2)?;
            return Ok(RuntimeMode::RootSkillInstall {
                source,
                source_type,
            });
        }
    }
    for index in 0..args.len() {
        if args[index] == "--update-root-skills" {
            validate_root_skills_update_args(args, index + 1)?;
            return Ok(RuntimeMode::RootSkillsUpdate);
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
                    value if is_removed_config_flag_arg(value) => {
                        return Err("Unsupported CLI flag: -config/--config. Use --runtime-root and place config at <runtime_root>/configs/config.yaml.".into());
                    }
                    value if is_inline_runtime_root_flag_arg(value) => {
                        if value.ends_with('=') {
                            return Err("--runtime-root requires a value".into());
                        }
                        cursor += 1;
                    }
                    "-runtime-root" | "--runtime-root" => {
                        require_cli_flag_value(args, cursor, args[cursor].as_str())?;
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

/// Require one value after a CLI flag so malformed call-tools invocations fail early.
/// 要求某个 CLI 标志后必须跟随一个值，以便尽早拒绝格式错误的 call-tools 调用。
fn require_cli_flag_value(
    args: &[String],
    index: usize,
    flag: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(next_value) = args.get(index + 1) else {
        return Err(format!("{flag} requires a value").into());
    };
    if next_value.starts_with("--") || next_value.starts_with('-') {
        return Err(format!("{flag} requires a value").into());
    }
    Ok(())
}

/// Parse optional `--source-type` arguments accepted by the ROOT install command.
/// 解析 ROOT 安装命令接受的可选 `--source-type` 参数。
fn parse_root_skill_install_source_type_from_args(
    args: &[String],
    start_index: usize,
) -> Result<Option<SkillInstallSourceType>, Box<dyn std::error::Error>> {
    // Track the optional source type while scanning command-local flags.
    // 扫描命令局部标志时跟踪可选来源类型。
    let mut source_type = None;
    // Walk only the suffix after the install source so global flags before the command stay valid.
    // 仅遍历安装来源之后的参数后缀，使命令之前的全局标志仍然有效。
    let mut cursor = start_index;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--source-type" => {
                require_cli_flag_value(args, cursor, "--source-type")?;
                // Parse the explicit source type value immediately after the flag.
                // 解析紧跟在标志后的显式来源类型值。
                let raw_source_type = args
                    .get(cursor + 1)
                    .ok_or("--source-type requires a value")?;
                source_type = Some(parse_skill_install_source_type(raw_source_type)?);
                cursor += 2;
            }
            "-runtime-root" | "--runtime-root" => {
                require_cli_flag_value(args, cursor, args[cursor].as_str())?;
                cursor += 2;
            }
            value if is_inline_runtime_root_flag_arg(value) => {
                if value.ends_with('=') {
                    return Err("--runtime-root requires a value".into());
                }
                cursor += 1;
            }
            value if is_removed_config_flag_arg(value) => {
                return Err("Unsupported CLI flag: -config/--config. Use --runtime-root and place config at <runtime_root>/configs/config.yaml.".into());
            }
            value if value.starts_with("--") => {
                return Err(format!("Unknown --install-root-skill flag: {}", value).into());
            }
            value => {
                return Err(format!(
                    "Unexpected --install-root-skill argument after source: {}",
                    value
                )
                .into());
            }
        }
    }
    Ok(source_type)
}

/// Validate command-local arguments accepted by the ROOT update-all command.
/// 校验 ROOT 全量更新命令接受的命令局部参数。
fn validate_root_skills_update_args(
    args: &[String],
    start_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Walk only the suffix after `--update-root-skills` so global flags before the command stay valid.
    // 仅遍历 `--update-root-skills` 之后的参数后缀，使命令之前的全局标志仍然有效。
    let mut cursor = start_index;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-runtime-root" | "--runtime-root" => {
                require_cli_flag_value(args, cursor, args[cursor].as_str())?;
                cursor += 2;
            }
            value if is_inline_runtime_root_flag_arg(value) => {
                if value.ends_with('=') {
                    return Err("--runtime-root requires a value".into());
                }
                cursor += 1;
            }
            value if is_removed_config_flag_arg(value) => {
                return Err("Unsupported CLI flag: -config/--config. Use --runtime-root and place config at <runtime_root>/configs/config.yaml.".into());
            }
            value if value.starts_with("--") => {
                return Err(format!("Unknown --update-root-skills flag: {}", value).into());
            }
            value => {
                return Err(format!("Unexpected --update-root-skills argument: {}", value).into());
            }
        }
    }
    Ok(())
}

/// Parse one CLI source-type token into the LuaSkills install source enum.
/// 将单个 CLI 来源类型片段解析为 LuaSkills 安装来源枚举。
fn parse_skill_install_source_type(
    value: &str,
) -> Result<SkillInstallSourceType, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "github" => Ok(SkillInstallSourceType::Github),
        "url" => Ok(SkillInstallSourceType::Url),
        _ => Err(format!(
            "unsupported source type '{}'; expected github or url",
            value
        )
        .into()),
    }
}

/// Build and initialize the MCP server, including external clients, Lua skills, and shared cache.
/// 构建并初始化 MCP Server，包括外部客户端、Lua Skills 与共享缓存。
async fn build_server(cfg: &Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = build_host_tool_surface_server(cfg)?;

    // Connect the VMM gRPC client only when explicitly enabled.
    // 仅在显式启用时连接 VMM gRPC 客户端。
    if cfg.vmm_enable {
        let endpoint = cfg
            .vmm
            .as_deref()
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .ok_or("vmm_enable=true requires a non-empty vmm endpoint")?;
        server = server.with_vmm(endpoint).await?;
    }

    // Load Lua skills from system directory, with optional user override
    let runtime_root = resolve_runtime_root_for_host(cfg)?;
    let mut skill_roots = find_skill_roots(&cfg)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    let resources_root = runtime_root.as_ref().map(|root| root.join("resources"));
    initialize_tool_result_template_roots(
        &skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect::<Vec<_>>(),
        resources_root.as_deref(),
    )
    .map_err(|error| format!("Failed to initialize tool result template roots: {}", error))?;
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

/// Ensure the formal runtime root chain contains the MCP-managed ROOT and USER layers when possible.
/// 在可行时确保正式运行根链包含 MCP 托管的 ROOT 与 USER 层。
fn ensure_skill_manager_runtime_roots(
    runtime_root: Option<&std::path::Path>,
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_root_skill_manager_root(runtime_root, skill_roots)?;
    if !skill_roots.is_empty()
        && !skill_roots
            .iter()
            .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        return Err("ROOT skill root is required before loading LuaSkills".into());
    }
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        ensure_user_skill_manager_root(skill_roots)?;
        sort_skill_manager_formal_roots(skill_roots)
            .map_err(|error| format!("Failed to order skill-manager runtime roots: {}", error))?;
        validate_unique_skill_root_spaces(skill_roots).map_err(|error| {
            format!("Failed to validate skill-manager runtime roots: {}", error)
        })?;
    }
    Ok(())
}

/// Ensure the default USER skills directory is present as the ordinary mutable layer.
/// 确保默认 USER skills 目录作为普通可变层存在。
fn ensure_user_skill_manager_root(
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "USER")
    {
        return Ok(());
    }
    let Some(user_skills_dir) = default_user_skill_root() else {
        return Ok(());
    };
    std::fs::create_dir_all(&user_skills_dir).map_err(|error| {
        format!(
            "Failed to create USER skills directory {}: {}",
            user_skills_dir.display(),
            error
        )
    })?;
    let user_root_key = normalize_skill_root_key(&user_skills_dir);
    let mut candidate_roots = skill_roots.clone();
    candidate_roots.retain(|root| normalize_skill_root_key(&root.skills_dir) != user_root_key);
    candidate_roots.push(RuntimeSkillRoot {
        name: "USER".to_string(),
        skills_dir: user_skills_dir,
    });
    sort_skill_manager_formal_roots(&mut candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager USER skills directory: {}",
            error
        )
    })?;
    validate_unique_skill_root_spaces(&candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager USER skills directory: {}",
            error
        )
    })?;
    *skill_roots = candidate_roots;
    Ok(())
}

/// Ensure the host-managed ROOT skills directory is present when no explicit ROOT layer exists.
/// 当不存在显式 ROOT 层时，确保宿主管理的 ROOT skills 目录存在。
fn ensure_root_skill_manager_root(
    runtime_root: Option<&std::path::Path>,
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        sort_skill_manager_formal_roots(skill_roots).map_err(|error| {
            format!("Failed to order configured skill-manager roots: {}", error)
        })?;
        return Ok(());
    }
    let Some(runtime_root) = runtime_root else {
        return Ok(());
    };
    let root_skills_dir = runtime_root.join("skills");
    std::fs::create_dir_all(&root_skills_dir).map_err(|error| {
        format!(
            "Failed to create ROOT skills directory {}: {}",
            root_skills_dir.display(),
            error
        )
    })?;
    let managed_root_key = normalize_skill_root_key(&root_skills_dir);
    let mut candidate_roots = skill_roots.clone();
    candidate_roots.retain(|root| normalize_skill_root_key(&root.skills_dir) != managed_root_key);
    candidate_roots.push(RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: root_skills_dir,
    });
    sort_skill_manager_formal_roots(&mut candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager ROOT skills directory: {}",
            error
        )
    })?;
    validate_unique_skill_root_spaces(&candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager ROOT skills directory: {}",
            error
        )
    })?;
    *skill_roots = candidate_roots;
    Ok(())
}

/// Normalize one skill-manager layer label for local chain construction.
/// 为本地根链构造规范化单个 skill-manager 层级标签。
fn normalize_skill_manager_layer_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

/// Return the fixed runtime priority rank for one skill-manager formal layer.
/// 返回单个 skill-manager 正式层级的固定运行时优先级。
fn skill_manager_layer_rank(name: &str) -> Result<usize, String> {
    match normalize_skill_manager_layer_name(name).as_str() {
        "ROOT" => Ok(0),
        "PROJECT" => Ok(1),
        "USER" => Ok(2),
        _ => Err(format!(
            "unsupported skill root label '{}'; expected ROOT, PROJECT, or USER",
            name.trim()
        )),
    }
}

/// Sort one root chain into ROOT -> PROJECT -> USER and normalize labels to uppercase.
/// 将根链排序为 ROOT -> PROJECT -> USER，并把标签规范化为大写。
fn sort_skill_manager_formal_roots(skill_roots: &mut [RuntimeSkillRoot]) -> Result<(), String> {
    for root in skill_roots.iter_mut() {
        root.name = normalize_skill_manager_layer_name(&root.name);
        skill_manager_layer_rank(&root.name)?;
    }
    skill_roots.sort_by_key(|root| skill_manager_layer_rank(&root.name).unwrap_or(usize::MAX));
    Ok(())
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

/// Install one managed LuaSkill into ROOT from the local CLI without starting MCP transports.
/// 在不启动 MCP 传输服务的情况下，从本地 CLI 将单个受管 LuaSkill 安装到 ROOT。
fn run_root_skill_install_mode(
    source: &str,
    source_type: Option<SkillInstallSourceType>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare configuration, cache, PATH, and LuaSkills callbacks for a local lifecycle command.
    // 为本地生命周期命令准备配置、缓存、PATH 与 LuaSkills 回调。
    let config = initialize_root_skill_cli_config()?;
    // Build the single-process ROOT lifecycle context after runtime paths are ready.
    // 在运行路径就绪后构建单进程 ROOT 生命周期上下文。
    let mut context = build_root_skill_cli_context(&config)?;
    // Infer the install source type only when the caller did not supply an override.
    // 仅在调用方未提供覆盖时推导安装来源类型。
    let source_type = infer_root_skill_install_source_type(source, source_type);
    // Keep the CLI request shape aligned with the LuaSkills managed install API.
    // 保持 CLI 请求形态与 LuaSkills 受管安装 API 对齐。
    let request = SkillInstallRequest {
        skill_id: None,
        source: Some(source.to_string()),
        source_type,
    };
    // Execute through the system authority so ROOT remains inaccessible to delegated tools.
    // 通过 system 权限执行，确保 ROOT 仍不会暴露给委托工具。
    let result = context.engine.system_install_skill_in_root(
        &context.skill_roots,
        &context.target_root,
        SkillManagementAuthority::System,
        &request,
    )?;

    println!("{}", render_root_skill_apply_result("install", &result));
    Ok(())
}

/// Update every managed LuaSkill declared in ROOT from the local CLI without starting MCP transports.
/// 在不启动 MCP 传输服务的情况下，从本地 CLI 更新 ROOT 中声明的全部受管 LuaSkill。
fn run_root_skills_update_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Prepare configuration, cache, PATH, and LuaSkills callbacks for a local lifecycle command.
    // 为本地生命周期命令准备配置、缓存、PATH 与 LuaSkills 回调。
    let config = initialize_root_skill_cli_config()?;
    // Build the single-process ROOT lifecycle context after runtime paths are ready.
    // 在运行路径就绪后构建单进程 ROOT 生命周期上下文。
    let mut context = build_root_skill_cli_context(&config)?;
    // Build a manager for reading ROOT install records before attempting updates.
    // 构建用于在尝试更新前读取 ROOT 安装记录的管理器。
    let manager = build_root_skill_manager_for_cli(&context.target_root, &context.host_options)?;
    // Collect only managed ROOT skills, because unmanaged directories have no update source.
    // 仅收集受管 ROOT 技能，因为非受管目录没有可用的更新来源。
    let managed_skill_ids = collect_managed_root_skill_ids(&context.target_root, &manager)?;
    // Render a single command summary so partial failures remain visible to shell callers.
    // 渲染单份命令摘要，确保 shell 调用方能看到局部失败。
    let mut rendered = String::new();
    writeln!(&mut rendered, "# root-skill-manager update-all")
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- layer: ROOT").expect("writing to String should not fail");
    writeln!(
        &mut rendered,
        "- target_root: {}",
        context.target_root.skills_dir.display()
    )
    .expect("writing to String should not fail");

    if managed_skill_ids.is_empty() {
        writeln!(&mut rendered, "- status: no_managed_skills")
            .expect("writing to String should not fail");
        writeln!(
            &mut rendered,
            "- message: no managed ROOT LuaSkills are installed"
        )
        .expect("writing to String should not fail");
        println!("{}", rendered);
        return Ok(());
    }

    // Count failed updates so the command can return a non-zero process status after printing details.
    // 统计失败更新数量，以便命令打印详情后返回非零进程状态。
    let mut failure_count = 0usize;
    for skill_id in managed_skill_ids {
        // Build one update request from the persisted managed install record identity.
        // 根据持久化受管安装记录标识构建单个更新请求。
        let request = SkillInstallRequest {
            skill_id: Some(skill_id.clone()),
            source: None,
            source_type: SkillInstallSourceType::Github,
        };
        // Execute each update through the system authority so ROOT writes stay host-controlled.
        // 每个更新都通过 system 权限执行，确保 ROOT 写入保持宿主控制。
        let result = context.engine.system_update_skill_in_root(
            &context.skill_roots,
            &context.target_root,
            SkillManagementAuthority::System,
            &request,
        );
        match result {
            Ok(result) => append_root_skill_update_result(&mut rendered, &result),
            Err(error) => {
                failure_count += 1;
                append_root_skill_update_error(&mut rendered, &skill_id, error.as_ref());
            }
        }
    }

    println!("{}", rendered);
    if failure_count > 0 {
        return Err(format!("ROOT skill update failed for {} skill(s)", failure_count).into());
    }
    Ok(())
}

/// Initialize shared runtime state for local ROOT lifecycle commands.
/// 为本地 ROOT 生命周期命令初始化共享运行时状态。
fn initialize_root_skill_cli_config() -> Result<Config, Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    // Load config through the normal runtime-root discovery path so CLI behavior stays consistent.
    // 通过标准 runtime-root 发现路径加载配置，保持 CLI 行为一致。
    let config = Config::load()?;
    initialize_runtime_temp_root_from_config(&config)?;
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&config)?;
    add_libs_to_path(&config)?;
    Ok(config)
}

/// Build the single-VM LuaSkills context used by local ROOT lifecycle commands.
/// 构建本地 ROOT 生命周期命令使用的单 VM LuaSkills 上下文。
fn build_root_skill_cli_context(
    config: &Config,
) -> Result<RootSkillCliContext, Box<dyn std::error::Error>> {
    // Resolve runtime root first so implicit ROOT/USER layers match normal service startup.
    // 先解析运行根，确保隐式 ROOT/USER 层与正常服务启动保持一致。
    let runtime_root = resolve_runtime_root_for_host(config)?;
    // Resolve and normalize the complete formal skill-root chain before selecting ROOT.
    // 在选择 ROOT 前解析并规范化完整正式技能根链。
    let mut skill_roots = find_skill_roots(config)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    // Select a concrete ROOT target and fail explicitly when none exists.
    // 选择具体 ROOT 目标，并在不存在时给出明确失败。
    let target_root = select_root_skill_manager_root(&skill_roots)?;
    // Build engine options once so the manager and engine share identical host paths.
    // 只构建一次引擎选项，确保管理器与引擎共享完全一致的宿主路径。
    let engine_options = build_luaskills_engine_options(
        config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )?;
    // Clone host options before moving the full options into LuaEngine.
    // 在完整选项移入 LuaEngine 前克隆宿主选项。
    let host_options = engine_options.host_options.clone();
    // Load existing skills so lifecycle preflight sees the same declared runtime state as service mode.
    // 加载现有技能，使生命周期预检查看到与服务模式一致的声明运行状态。
    let mut engine = LuaEngine::new(engine_options)?;
    engine.load_from_roots(&skill_roots)?;

    Ok(RootSkillCliContext {
        engine,
        skill_roots,
        target_root,
        host_options,
    })
}

/// Select the ROOT layer from an already normalized formal skill-root chain.
/// 从已经规范化的正式技能根链中选择 ROOT 层。
fn select_root_skill_manager_root(
    roots: &[RuntimeSkillRoot],
) -> Result<RuntimeSkillRoot, Box<dyn std::error::Error>> {
    roots
        .iter()
        .find(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
        .cloned()
        .ok_or_else(|| "ROOT skill root is not configured for local skill management.".into())
}

/// Build one SkillManager for ROOT install-record inspection using the engine host options.
/// 使用引擎宿主选项构建一个用于检查 ROOT 安装记录的 SkillManager。
fn build_root_skill_manager_for_cli(
    root: &RuntimeSkillRoot,
    host_options: &LuaRuntimeHostOptions,
) -> Result<SkillManager, Box<dyn std::error::Error>> {
    // Derive the lifecycle root exactly like the MCP skill-manager tool does.
    // 按照 MCP skill-manager 工具的方式推导生命周期根目录。
    let runtime_root = root
        .skills_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.skills_dir.clone());
    // Reuse the host download-cache fallback so update checks share cached archives.
    // 复用宿主下载缓存回退逻辑，让更新检查共享归档缓存。
    let download_cache_root = host_options.download_cache_root.clone().unwrap_or_else(|| {
        host_options
            .temp_dir
            .clone()
            .unwrap_or_else(|| runtime_root.join("temp"))
            .join("downloads")
    });
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root: root.clone(),
        lifecycle_root: runtime_root.join(host_options.state_dir_name.as_str()),
        download_cache_root,
        allow_network_download: host_options.allow_network_download,
        github_base_url: host_options.github_base_url.clone(),
        github_api_base_url: host_options.github_api_base_url.clone(),
    }))
}

/// Collect ROOT skill ids that are managed by install records and therefore updateable.
/// 收集由安装记录管理、因此可更新的 ROOT 技能标识。
fn collect_managed_root_skill_ids(
    root: &RuntimeSkillRoot,
    manager: &SkillManager,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !root.skills_dir.exists() {
        return Ok(Vec::new());
    }
    // Keep output deterministic regardless of filesystem iteration order.
    // 无论文件系统迭代顺序如何，都保持输出稳定。
    let mut skill_ids = Vec::new();
    for entry in std::fs::read_dir(&root.skills_dir)? {
        // Read one directory entry from the ROOT skills directory.
        // 从 ROOT skills 目录读取单个目录项。
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        // Convert the directory name into the skill id used by LuaSkills records.
        // 将目录名转换为 LuaSkills 记录使用的技能标识。
        let Some(skill_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !entry.path().join("skill.yaml").exists() {
            continue;
        }
        // Only managed records carry an update source; unmanaged ROOT directories are intentionally skipped.
        // 只有受管记录携带更新来源；非受管 ROOT 目录会被有意跳过。
        let Some(record) = manager.install_record(&skill_id)? else {
            continue;
        };
        if record.managed {
            skill_ids.push(skill_id);
        }
    }
    skill_ids.sort();
    Ok(skill_ids)
}

/// Infer the ROOT install source type when the CLI caller did not provide an explicit override.
/// 当 CLI 调用方未提供显式覆盖时推导 ROOT 安装来源类型。
fn infer_root_skill_install_source_type(
    source: &str,
    explicit: Option<SkillInstallSourceType>,
) -> SkillInstallSourceType {
    if let Some(source_type) = explicit {
        return source_type;
    }
    // Treat non-GitHub HTTP(S) locators as URL sources and everything else as GitHub.
    // 将非 GitHub HTTP(S) 定位值视为 URL 来源，其余视为 GitHub 来源。
    let normalized_source = source.trim().to_ascii_lowercase();
    if (normalized_source.starts_with("http://") || normalized_source.starts_with("https://"))
        && !normalized_source.contains("github.com/")
    {
        SkillInstallSourceType::Url
    } else {
        SkillInstallSourceType::Github
    }
}

/// Render one ROOT install or update result as compact command-line Markdown.
/// 将单个 ROOT 安装或更新结果渲染为紧凑的命令行 Markdown。
fn render_root_skill_apply_result(action: &str, result: &SkillApplyResult) -> String {
    // Build output in the same high-signal shape as the MCP skill-manager result.
    // 使用与 MCP skill-manager 结果相同的高信号形态构建输出。
    let mut rendered = String::new();
    writeln!(&mut rendered, "# root-skill-manager {}", action)
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- layer: ROOT").expect("writing to String should not fail");
    append_root_skill_apply_fields(&mut rendered, result);
    rendered
}

/// Append common apply-result fields shared by ROOT install and update output.
/// 追加 ROOT 安装与更新输出共享的应用结果字段。
fn append_root_skill_apply_fields(rendered: &mut String, result: &SkillApplyResult) {
    writeln!(rendered, "- skill_id: {}", result.skill_id)
        .expect("writing to String should not fail");
    writeln!(rendered, "- status: {}", result.status).expect("writing to String should not fail");
    if let Some(version) = result.version.as_deref() {
        writeln!(rendered, "- version: {}", version).expect("writing to String should not fail");
    }
    if let Some(source_type) = result.source_type {
        writeln!(
            rendered,
            "- source_type: {}",
            render_root_skill_install_source_type(source_type)
        )
        .expect("writing to String should not fail");
    }
    if let Some(source_locator) = result.source_locator.as_deref() {
        writeln!(rendered, "- source: {}", source_locator)
            .expect("writing to String should not fail");
    }
    writeln!(rendered, "- message: {}", result.message).expect("writing to String should not fail");
}

/// Append one successful ROOT update result to the update-all command summary.
/// 将单个成功的 ROOT 更新结果追加到全量更新命令摘要。
fn append_root_skill_update_result(rendered: &mut String, result: &SkillApplyResult) {
    writeln!(rendered).expect("writing to String should not fail");
    writeln!(rendered, "## {}", result.skill_id).expect("writing to String should not fail");
    append_root_skill_apply_fields(rendered, result);
}

/// Append one failed ROOT update result to the update-all command summary.
/// 将单个失败的 ROOT 更新结果追加到全量更新命令摘要。
fn append_root_skill_update_error(
    rendered: &mut String,
    skill_id: &str,
    error: &dyn std::error::Error,
) {
    writeln!(rendered).expect("writing to String should not fail");
    writeln!(rendered, "## {}", skill_id).expect("writing to String should not fail");
    writeln!(rendered, "- skill_id: {}", skill_id).expect("writing to String should not fail");
    writeln!(rendered, "- status: failed").expect("writing to String should not fail");
    writeln!(rendered, "- message: {}", error).expect("writing to String should not fail");
}

/// Render one skill install source type as a stable CLI string.
/// 将单个技能安装来源类型渲染为稳定的 CLI 字符串。
fn render_root_skill_install_source_type(source_type: SkillInstallSourceType) -> &'static str {
    match source_type {
        SkillInstallSourceType::Github => "github",
        SkillInstallSourceType::Url => "url",
    }
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
    let request_context = build_call_tool_request_context(simulated_client_name);
    let response = runtime
        .block_on(server.handle_message_with_context(
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

/// Build a single-VM LuaEngine with fully loaded skills from the unified runtime root for local debug modes.
/// 在本地调试模式下基于统一运行根构建一个完整加载 skills 的单虚拟机 LuaEngine。
fn build_single_vm_lua_engine_for_local_mode(
    config: &Config,
) -> Result<LuaEngine, Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(config)?;
    let mut skill_roots = find_skill_roots(config)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    if !skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        return Err("Lua skill directory not found for local debug mode".into());
    }
    let resources_root = runtime_root.as_ref().map(|root| root.join("resources"));
    initialize_tool_result_template_roots(
        &skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect::<Vec<_>>(),
        resources_root.as_deref(),
    )
    .map_err(|error| format!("Failed to initialize tool result template roots: {}", error))?;

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

/// Find the ordered skill-root chain for the default runtime environment.
/// 查找默认运行环境使用的有序技能根目录覆盖链。
fn find_skill_roots(
    cfg: &config::Config,
) -> Result<Vec<luaskills::RuntimeSkillRoot>, Box<dyn std::error::Error>> {
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
        client_match_name_override: None,
        client_capabilities: json!({}),
    }
}

/// Preload hot-reloadable runtime config files before the host starts so configuration issues surface before the first request.
/// 在宿主启动前预载可热重载的运行时配置文件，避免首次请求时才暴露配置问题。
fn preload_runtime_mcp_configs(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(cfg)?;
    initialize_client_budget_runtime_root(runtime_root.as_deref())
        .map_err(|error| format!("Failed to initialize client budget runtime root: {}", error))?;
    initialize_tool_config_runtime_root(runtime_root.as_deref())
        .map_err(|error| format!("Failed to initialize tool config runtime root: {}", error))?;
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
fn add_libs_to_path(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let Some(runtime_root) = resolve_runtime_root_for_host(config)? else {
        return Ok(());
    };
    let libs_dir = runtime_root.join("libs");

    if !libs_dir.exists() {
        return Ok(());
    }
    if !libs_dir.is_dir() {
        return Err(format!(
            "runtime libs path is not a directory: {}",
            libs_dir.display()
        )
        .into());
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Return one shared mutex used to serialize PATH-dependent tests.
    /// 返回一个共享互斥锁，用于串行化依赖 PATH 的测试。
    fn environment_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Build one unique temporary directory path for one main-module test case.
    /// 为 main 模块单个测试用例构建唯一临时目录路径。
    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let unique = format!(
            "vulcan-mcp-main-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    /// Write one minimal ROOT skill directory used by local CLI tests.
    /// 写入一个供本地 CLI 测试使用的最小 ROOT 技能目录。
    fn write_minimal_root_skill(skill_root: &std::path::Path, skill_id: &str) {
        // Create the skill directory before writing the manifest.
        // 写入清单前先创建技能目录。
        let skill_dir = skill_root.join(skill_id);
        std::fs::create_dir_all(&skill_dir).expect("skill directory should be created");
        std::fs::write(
            skill_dir.join("skill.yaml"),
            format!("name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nentries: []\n"),
        )
        .expect("skill manifest should be written");
    }

    /// Write one managed install record under the ROOT lifecycle state directory.
    /// 在 ROOT 生命周期状态目录下写入一条受管安装记录。
    fn write_root_install_record(runtime_root: &std::path::Path, skill_id: &str) {
        // Match the lifecycle layout derived by build_root_skill_manager_for_cli.
        // 匹配 build_root_skill_manager_for_cli 推导出的生命周期布局。
        let install_record_root = runtime_root.join("state").join("installs");
        std::fs::create_dir_all(&install_record_root)
            .expect("install record directory should be created");
        // Persist a GitHub-managed record so update-all considers the skill updateable.
        // 持久化 GitHub 受管记录，使全量更新认为该技能可更新。
        let record = luaskills::InstalledSkillRecord {
            skill_id: skill_id.to_string(),
            version: "0.1.0".to_string(),
            managed: true,
            source: luaskills::InstalledSkillSourceRecord {
                source_type: SkillInstallSourceType::Github,
                locator: format!("LuaSkills/{skill_id}"),
                tag: Some("v0.1.0".to_string()),
            },
            installed_at_unix_ms: 1,
        };
        std::fs::write(
            install_record_root.join(format!("{skill_id}.yaml")),
            serde_yaml::to_string(&record).expect("record should serialize"),
        )
        .expect("install record should be written");
    }

    /// Call-tools mode should accept --runtime-root so isolated runtime validation can use the same CLI entrypoint.
    /// call-tools 模式应当接受 --runtime-root，以便隔离运行根验证复用同一 CLI 入口。
    #[test]
    fn parse_runtime_mode_allows_runtime_root_in_call_tools_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--runtime-root".to_string(),
            "runtime".to_string(),
            "{\"ok\":true}".to_string(),
        ];
        let mode = parse_runtime_mode_from_args(&args).expect("call-tools mode should parse");
        match mode {
            RuntimeMode::Stdio => {}
            RuntimeMode::CallTool {
                tool_name,
                arguments,
                simulated_client_name,
            } => {
                assert_eq!(tool_name, "demo-tool");
                assert_eq!(arguments, json!({ "ok": true }));
                assert_eq!(simulated_client_name, DEFAULT_CALL_TOOL_CLIENT_NAME);
            }
            RuntimeMode::Serve
            | RuntimeMode::RootSkillInstall { .. }
            | RuntimeMode::RootSkillsUpdate
            | RuntimeMode::InternalLuaexecRequest { .. } => {
                panic!("expected call-tools runtime mode");
            }
        }
    }

    /// Stdio mode should be selectable directly so MCP can run over stdin/stdout without opening ports.
    /// stdio 模式应可被直接选中，以便 MCP 通过标准输入输出运行而无需打开端口。
    #[test]
    fn parse_runtime_mode_accepts_stdio_mode() {
        let args = vec!["vulcan-mcp.exe".to_string(), "--stdio".to_string()];
        let mode = parse_runtime_mode_from_args(&args).expect("stdio mode should parse");
        match mode {
            RuntimeMode::Stdio => {}
            RuntimeMode::Serve
            | RuntimeMode::CallTool { .. }
            | RuntimeMode::RootSkillInstall { .. }
            | RuntimeMode::RootSkillsUpdate
            | RuntimeMode::InternalLuaexecRequest { .. } => {
                panic!("expected stdio runtime mode");
            }
        }
    }

    /// ROOT install mode should parse as a local command instead of falling through to service mode.
    /// ROOT 安装模式应解析为本地命令，而不是落回服务模式。
    #[test]
    fn parse_runtime_mode_accepts_root_install_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--install-root-skill".to_string(),
            "LuaSkills/vulcan-codekit".to_string(),
            "--source-type".to_string(),
            "github".to_string(),
            "--runtime-root".to_string(),
            "output".to_string(),
        ];
        let mode = parse_runtime_mode_from_args(&args).expect("root install mode should parse");
        match mode {
            RuntimeMode::RootSkillInstall {
                source,
                source_type,
            } => {
                assert_eq!(source, "LuaSkills/vulcan-codekit");
                assert_eq!(source_type, Some(SkillInstallSourceType::Github));
            }
            RuntimeMode::Serve
            | RuntimeMode::Stdio
            | RuntimeMode::CallTool { .. }
            | RuntimeMode::RootSkillsUpdate
            | RuntimeMode::InternalLuaexecRequest { .. } => {
                panic!("expected root skill install runtime mode");
            }
        }
    }

    /// ROOT update-all mode should parse as a local command that does not start transports.
    /// ROOT 全量更新模式应解析为不启动传输服务的本地命令。
    #[test]
    fn parse_runtime_mode_accepts_root_update_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--update-root-skills".to_string(),
            "--runtime-root=output".to_string(),
        ];
        let mode = parse_runtime_mode_from_args(&args).expect("root update mode should parse");
        match mode {
            RuntimeMode::RootSkillsUpdate => {}
            RuntimeMode::Serve
            | RuntimeMode::Stdio
            | RuntimeMode::CallTool { .. }
            | RuntimeMode::RootSkillInstall { .. }
            | RuntimeMode::InternalLuaexecRequest { .. } => {
                panic!("expected root skills update runtime mode");
            }
        }
    }

    /// Call-tools mode should reject runtime-root flags that do not carry a concrete value.
    /// call-tools 模式应拒绝未携带实际取值的 runtime-root 标志。
    #[test]
    fn parse_runtime_mode_rejects_missing_runtime_root_value_in_call_tools_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--runtime-root".to_string(),
            "-config".to_string(),
        ];
        let error = match parse_runtime_mode_from_args(&args) {
            Ok(_) => panic!("missing value should fail"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("--runtime-root requires a value"),
            "unexpected error: {error}"
        );
    }

    /// Call-tools mode should reject the removed legacy config flag and redirect callers to runtime-root based config discovery.
    /// call-tools 模式应拒绝已移除的历史 config 标志，并引导调用方改用基于 runtime-root 的配置发现。
    #[test]
    fn parse_runtime_mode_rejects_legacy_config_flag_in_call_tools_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--config".to_string(),
            "runtime/configs/config.yaml".to_string(),
        ];
        let error = match parse_runtime_mode_from_args(&args) {
            Ok(_) => panic!("legacy config flag should fail"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("Unsupported CLI flag"),
            "unexpected error: {error}"
        );
    }

    /// Call-tools mode should reject inline `--config=...` forms too so the removed config entrypoint is blocked consistently.
    /// call-tools 模式也应拒绝内联 `--config=...` 形式，保证已移除的配置入口被一致封死。
    #[test]
    fn parse_runtime_mode_rejects_inline_legacy_config_flag_in_call_tools_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--config=runtime/configs/config.yaml".to_string(),
        ];
        let error = match parse_runtime_mode_from_args(&args) {
            Ok(_) => panic!("inline legacy config flag should fail"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("Unsupported CLI flag"),
            "unexpected error: {error}"
        );
    }

    /// Call-tools mode should accept inline `--runtime-root=...` forms so local debug CLI behavior matches the main config loader.
    /// call-tools 模式应接受内联 `--runtime-root=...` 形式，从而让本地调试 CLI 行为与主配置加载器保持一致。
    #[test]
    fn parse_runtime_mode_allows_inline_runtime_root_in_call_tools_mode() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--runtime-root=output".to_string(),
        ];
        let mode = parse_runtime_mode_from_args(&args).expect("inline runtime-root should parse");
        match mode {
            RuntimeMode::CallTool { tool_name, .. } => {
                assert_eq!(tool_name, "demo-tool");
            }
            RuntimeMode::Serve
            | RuntimeMode::Stdio
            | RuntimeMode::RootSkillInstall { .. }
            | RuntimeMode::RootSkillsUpdate
            | RuntimeMode::InternalLuaexecRequest { .. } => {
                panic!("expected call-tools runtime mode");
            }
        }
    }

    /// Call-tools mode should reject inline `--runtime-root=` forms when the value is empty.
    /// call-tools 模式在内联 `--runtime-root=` 取值为空时应拒绝调用。
    #[test]
    fn parse_runtime_mode_rejects_empty_inline_runtime_root_value() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--call-tools".to_string(),
            "demo-tool".to_string(),
            "--runtime-root=".to_string(),
        ];
        let error = match parse_runtime_mode_from_args(&args) {
            Ok(_) => panic!("empty inline runtime-root should fail"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("--runtime-root requires a value"),
            "unexpected error: {error}"
        );
    }

    /// Host-only reload tools should still run even when configured skill roots are invalid, because they no longer require preloading the Lua engine.
    /// 仅宿主侧的 reload 工具即使在技能根配置无效时也应能运行，因为它们不再要求预先加载 Lua 引擎。
    #[test]
    fn run_call_host_tool_mode_supports_reload_without_loading_invalid_skill_roots() {
        let root = unique_test_dir("reload-host-tool");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let missing_skill_root = root.join("missing-skills");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![crate::config::SkillRootConfigEntry::Path(
                missing_skill_root.to_string_lossy().to_string(),
            )]),
            ..Config::default()
        };

        preload_runtime_mcp_configs(&config).expect("host runtime config preload should succeed");
        run_call_host_tool_mode(
            config,
            "reload_vulcan_mcp_configs",
            json!({}),
            DEFAULT_CALL_TOOL_CLIENT_NAME,
        )
        .expect("reload host tool should succeed without loading invalid skill roots");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Host-only luaskill-config should still be exposed through the built server even when no Lua skill roots are available.
    /// 即使没有任何 Lua 技能根，构建出的服务也应继续对外暴露宿主侧 luaskill-config。
    #[test]
    fn build_server_exposes_luaskill_config_without_skill_roots() {
        let root = unique_test_dir("luaskill-config-build-server");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![]),
            ..Config::default()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        let server = runtime
            .block_on(build_server(&config))
            .expect("build_server should succeed without skills");
        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }),
                RequestContext::default(),
            ))
            .expect("tools/list should return one response");
        let tool_names = response
            .get("result")
            .and_then(|value| value.get("tools"))
            .and_then(Value::as_array)
            .expect("tools array should exist in result payload")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(tool_names.contains(&"luaskill-config"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Empty runtime roots should still expose skill-manager and create the ROOT skills directory for first-run installs.
    /// 空运行根仍应暴露 skill-manager，并为首次运行安装创建 ROOT skills 目录。
    #[test]
    fn build_server_exposes_skill_manager_without_existing_skills() {
        let root = unique_test_dir("skill-manager-empty-runtime");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![]),
            ..Config::default()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        let server = runtime
            .block_on(build_server(&config))
            .expect("build_server should succeed without preinstalled skills");
        assert!(
            root.join("skills").is_dir(),
            "ROOT skills directory should be created for skill-manager"
        );

        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }),
                RequestContext::default(),
            ))
            .expect("tools/list should return one response");
        let tool_names = response
            .get("result")
            .and_then(|value| value.get("tools"))
            .and_then(Value::as_array)
            .expect("tools array should exist in result payload")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(tool_names.contains(&"skill-manager"));

        let list_response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager list should return one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            list_response
                .get("result")
                .cloned()
                .expect("skill-manager list should return result"),
        )
        .expect("skill-manager list result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();
        assert_eq!(rendered, "No LuaSkills are installed in the USER layer.");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Skill-manager update should report a tool error when USER does not contain the skill.
    /// 当 USER 不包含该技能时，skill-manager update 应报告工具错误。
    #[test]
    fn skill_manager_update_missing_skill_returns_tool_error() {
        let root = unique_test_dir("skill-manager-update-missing");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![]),
            ..Config::default()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        let server = runtime
            .block_on(build_server(&config))
            .expect("build_server should succeed without preinstalled skills");
        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "update",
                            "skill_id": "vulcan-codekit"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager update should return one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .expect("skill-manager update should return result"),
        )
        .expect("skill-manager update result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert_eq!(tool_result.is_error, Some(true));
        assert!(rendered.contains("skill-manager update failed"));
        assert!(rendered.contains("not installed"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The configured ROOT layer should remain the single system root when it is already present.
    /// 当已存在显式 ROOT 层时，应保留它作为唯一系统根。
    #[test]
    fn skill_manager_root_uses_configured_root_layer_without_duplicate() {
        let root = unique_test_dir("skill-manager-root-name-collision");
        let configured_skills_dir = root.join("configured-skills");
        let runtime_root = root.join("runtime");
        std::fs::create_dir_all(&configured_skills_dir)
            .expect("failed to create configured skills dir");
        let mut skill_roots = vec![RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: configured_skills_dir.clone(),
        }];

        ensure_root_skill_manager_root(Some(&runtime_root), &mut skill_roots)
            .expect("configured ROOT should be preserved");

        assert_eq!(skill_roots.len(), 1);
        assert_eq!(skill_roots[0].name, "ROOT");
        assert_eq!(skill_roots[0].skills_dir, configured_skills_dir);
        assert!(!runtime_root.join("skills").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Runtime root construction should keep formal ROOT before USER regardless of insertion order.
    /// 运行根构造应保持正式 ROOT 位于 USER 之前，不受插入顺序影响。
    #[test]
    fn skill_manager_roots_are_ordered_by_formal_layers() {
        let root = unique_test_dir("skill-manager-formal-order");
        let runtime_root = root.join("runtime");
        std::fs::create_dir_all(runtime_root.join("configs"))
            .expect("failed to create runtime config dir");
        let mut skill_roots = Vec::new();

        ensure_skill_manager_runtime_roots(Some(&runtime_root), &mut skill_roots)
            .expect("formal roots should be created");

        assert!(skill_roots.len() >= 2);
        assert_eq!(skill_roots[0].name, "ROOT");
        assert_eq!(skill_roots[1].name, "USER");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// ROOT CLI selection should choose the system layer even when ordinary layers are also present.
    /// 即使普通层同时存在，ROOT CLI 选择逻辑也应选中系统层。
    #[test]
    fn root_skill_cli_selection_uses_root_layer() {
        let roots = vec![
            RuntimeSkillRoot {
                name: "USER".to_string(),
                skills_dir: std::path::PathBuf::from("D:/user/skills"),
            },
            RuntimeSkillRoot {
                name: "ROOT".to_string(),
                skills_dir: std::path::PathBuf::from("D:/runtime/skills"),
            },
        ];

        let root = select_root_skill_manager_root(&roots).expect("ROOT layer should resolve");

        assert_eq!(root.name, "ROOT");
        assert_eq!(
            root.skills_dir,
            std::path::PathBuf::from("D:/runtime/skills")
        );
    }

    /// ROOT update-all discovery should include only skill directories with managed install records.
    /// ROOT 全量更新发现逻辑应只包含带受管安装记录的技能目录。
    #[test]
    fn collect_managed_root_skill_ids_skips_unmanaged_skills() {
        let root = unique_test_dir("root-managed-skill-ids");
        let runtime_root = root.join("runtime");
        let root_layer = RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: runtime_root.join("skills"),
        };
        write_minimal_root_skill(&root_layer.skills_dir, "managed-skill");
        write_minimal_root_skill(&root_layer.skills_dir, "unmanaged-skill");
        write_root_install_record(&runtime_root, "managed-skill");
        let host_options = LuaRuntimeHostOptions {
            temp_dir: Some(runtime_root.join("temp")),
            state_dir_name: "state".to_string(),
            allow_network_download: false,
            ..LuaRuntimeHostOptions::default()
        };
        let manager = build_root_skill_manager_for_cli(&root_layer, &host_options)
            .expect("ROOT manager should build");

        let skill_ids = collect_managed_root_skill_ids(&root_layer, &manager)
            .expect("managed skill ids should be collected");

        assert_eq!(skill_ids, vec!["managed-skill".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The host-managed root must not bypass sibling runtime-space uniqueness after it is appended.
    /// 宿主管理根追加后不得绕过同级运行空间唯一性约束。
    #[test]
    fn skill_manager_root_rejects_sibling_runtime_space_collision() {
        let root = unique_test_dir("skill-manager-root-space-collision");
        let runtime_root = root.join("runtime");
        let configured_skills_dir = runtime_root.join("custom-skills");
        std::fs::create_dir_all(&configured_skills_dir)
            .expect("failed to create configured skills dir");
        let mut skill_roots = vec![RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: configured_skills_dir.clone(),
        }];

        let error = ensure_root_skill_manager_root(Some(&runtime_root), &mut skill_roots)
            .expect_err("managed root should reject sibling runtime-space collisions");

        assert!(
            error
                .to_string()
                .contains("shares the same sibling runtime space"),
            "unexpected error: {error}"
        );
        assert_eq!(
            skill_roots,
            vec![RuntimeSkillRoot {
                name: "USER".to_string(),
                skills_dir: configured_skills_dir,
            }]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Skill-manager target mutations should reject missing skill ids before touching the runtime.
    /// skill-manager 目标变更应在触碰运行时前拒绝缺失的技能标识。
    #[test]
    fn skill_manager_update_requires_skill_id() {
        let root = unique_test_dir("skill-manager-update-skill-id");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![]),
            ..Config::default()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");
        let server = runtime
            .block_on(build_server(&config))
            .expect("build_server should succeed without preinstalled skills");

        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "update"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager update should return one response");
        let message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(message.contains("requires parameter: skill_id"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Host-only luaskill-config should still run even when configured skill roots are invalid, because it should bypass Lua engine loading.
    /// 宿主侧 luaskill-config 即使在技能根配置无效时也应能运行，因为它应跳过 Lua 引擎加载。
    #[test]
    fn run_call_host_tool_mode_supports_luaskill_config_without_loading_invalid_skill_roots() {
        let root = unique_test_dir("luaskill-config-host-tool");
        std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
        let missing_skill_root = root.join("missing-skills");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_roots: Some(vec![crate::config::SkillRootConfigEntry::Path(
                missing_skill_root.to_string_lossy().to_string(),
            )]),
            ..Config::default()
        };

        preload_runtime_mcp_configs(&config).expect("host runtime config preload should succeed");
        run_call_host_tool_mode(
            config,
            "luaskill-config",
            json!({
                "action": "set",
                "skill_id": "demo-skill",
                "key": "api_token",
                "value": "sk-local"
            }),
            DEFAULT_CALL_TOOL_CLIENT_NAME,
        )
        .expect("luaskill-config host tool should succeed without loading invalid skill roots");

        let persisted: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("configs").join("skill_config.json"))
                .expect("luaskill-config file should be created"),
        )
        .expect("persisted luaskill-config JSON should parse");
        assert_eq!(persisted["skills"]["demo-skill"]["api_token"], "sk-local");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Explicit invalid runtime_root values should fail before host-side reload logic falls back to implicit runtime discovery.
    /// 显式无效的 runtime_root 应在宿主 reload 逻辑回退到隐式运行根发现之前直接失败。
    #[test]
    fn preload_runtime_mcp_configs_rejects_invalid_explicit_runtime_root() {
        let root = unique_test_dir("invalid-runtime-root");
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let config = Config {
            runtime_root: Some(root.join("missing-runtime").to_string_lossy().to_string()),
            ..Config::default()
        };

        let error = preload_runtime_mcp_configs(&config)
            .expect_err("invalid explicit runtime_root should fail");
        assert!(
            error
                .to_string()
                .contains("configured runtime_root does not exist"),
            "unexpected error: {error}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// File-shaped runtime libs paths should be rejected before PATH mutation begins.
    /// 文件形态的运行时 libs 路径应在修改 PATH 之前被拒绝。
    #[test]
    fn add_libs_to_path_rejects_file_shaped_runtime_libs_dir() {
        let _guard = environment_lock().lock().expect("lock should succeed");
        let root = unique_test_dir("runtime-libs-file");
        std::fs::create_dir_all(&root).expect("failed to create runtime root");
        let libs_file = root.join("libs");
        std::fs::write(&libs_file, b"not-a-directory").expect("failed to create libs file");
        let original_path = std::env::var("PATH").unwrap_or_default();
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };

        let error = add_libs_to_path(&config).expect_err("file-shaped libs path should fail");
        assert!(
            error
                .to_string()
                .contains("runtime libs path is not a directory"),
            "unexpected error: {error}"
        );
        assert_eq!(
            std::env::var("PATH").unwrap_or_default(),
            original_path,
            "PATH should remain unchanged when libs path is invalid"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
