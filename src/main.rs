mod config;
#[allow(dead_code)]
mod grpc_client;
mod grpc_server;
mod http_server;
mod lancedb_host;
mod lua_engine;
mod lua_skill;
#[allow(dead_code)]
mod protocol;
mod server;
#[allow(dead_code)]
mod session;
mod skill_dependency;
mod temp_maintenance;
mod tool_cache;

pub mod pb_lancedb {
    tonic::include_proto!("vldb.lancedb.v1");
}

pub mod pb_sqlite {
    tonic::include_proto!("vldb.sqlite.v1");
}

pub mod pb_vmm {
    tonic::include_proto!("vmm.v1");
}

pub mod pb_mcp {
    tonic::include_proto!("vulcan.mcp.v1");
}

use config::Config;
use lua_engine::{LuaEngine, LuaVmPoolConfig};
use serde_json::{Value, json};
use server::McpServer;
use temp_maintenance::{CleanupTrigger, maintain_runtime_temp_dir, spawn_cross_day_cleanup_task};
use tool_cache::ToolCacheConfig;
use tool_cache::configure_global_tool_cache;

/// 中文：将 `--call-tools` 结果按类型输出；基础标量原样打印，数组和对象保持 JSON 形式。
/// English: Print `--call-tools` results by type; emit scalar values verbatim while keeping arrays/objects as JSON.
fn print_call_tools_result(value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::String(text) => println!("{}", text),
        Value::Number(number) => println!("{}", number),
        Value::Bool(flag) => println!("{}", flag),
        Value::Null => println!("null"),
        Value::Array(_) | Value::Object(_) => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_mode = parse_runtime_mode()?;
    match runtime_mode {
        RuntimeMode::CallTool {
            tool_name,
            arguments,
        } => run_call_tool_mode(&tool_name, arguments),
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

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;

    configure_global_tool_cache(ToolCacheConfig {
        max_entries: cfg
            .tool_cache_max_entries
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_MAX_ENTRIES),
        default_ttl_secs: cfg
            .tool_cache_default_ttl_secs
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS),
        max_ttl_secs: cfg
            .tool_cache_max_ttl_secs
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_MAX_TTL_SECS),
    });

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
    /// English: Initialize the tool runtime only and directly invoke a single tool for local debugging.
    CallTool { tool_name: String, arguments: Value },
}

/// 中文：根据命令行参数解析运行模式。
/// 支持 `--call-tools <tool_name> [json_arguments]`。
/// English: Parse the runtime mode from CLI arguments.
/// Supports `--call-tools <tool_name> [json_arguments]`.
fn parse_runtime_mode() -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    for index in 0..args.len() {
        if args[index] == "--call-tools" {
            let tool_name = args
                .get(index + 1)
                .ok_or("--call-tools requires a tool name")?
                .clone();

            let raw_arguments = args
                .get(index + 2)
                .filter(|value| !is_reserved_cli_flag(value))
                .cloned();

            let arguments = match raw_arguments {
                Some(raw) => serde_json::from_str::<Value>(&raw)?,
                None => json!({}),
            };

            return Ok(RuntimeMode::CallTool {
                tool_name,
                arguments,
            });
        }
    }

    Ok(RuntimeMode::Serve)
}

/// 中文：判断某个 CLI token 是否属于主程序保留参数，避免把 `-config` 误当作 tool 参数 JSON。
/// English: Determine whether a CLI token is a reserved program-level flag so `-config` is not mistaken for tool-argument JSON.
fn is_reserved_cli_flag(value: &str) -> bool {
    matches!(value, "-config" | "--config" | "--call-tools")
}

/// 中文：构建并初始化 MCP Server，包括外部客户端、Lua Skills 与共享缓存。
/// English: Build and initialize the MCP server, including external clients, Lua skills, and shared cache.
async fn build_server(cfg: &Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();

    // Connect gRPC clients if configured
    if let Some(endpoint) = &cfg.lancedb {
        server = server.with_lancedb(endpoint).await?;
    }
    if let Some(endpoint) = &cfg.sqlite {
        server = server.with_sqlite(endpoint).await?;
        // Auto-enable scratchpad when sqlite is available
        server = server.with_scratchpad_from_sqlite().await?;
    }
    if let Some(endpoint) = &cfg.vmm {
        server = server.with_vmm(endpoint).await?;
    }

    // Load Lua skills from system directory, with optional user override
    let lua_skills_loaded = find_lua_skill_dirs(&cfg);
    if let Some((base_dir, override_dir)) = lua_skills_loaded {
        server = server.with_lua_skills(
            &base_dir,
            override_dir.as_deref(),
            LuaVmPoolConfig {
                min_size: cfg.lua_vm_pool_min_size.unwrap_or(1),
                max_size: cfg.lua_vm_pool_max_size.unwrap_or(4),
                idle_ttl_secs: cfg.lua_vm_pool_idle_ttl_secs.unwrap_or(300),
            },
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
/// English: Initialize Lua skills and invoke the target tool directly without starting transports, making skill loading, dependency setup, and real return values easier to debug.
fn run_call_tool_mode(tool_name: &str, arguments: Value) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load()?;

    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;

    configure_global_tool_cache(ToolCacheConfig {
        max_entries: cfg
            .tool_cache_max_entries
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_MAX_ENTRIES),
        default_ttl_secs: cfg
            .tool_cache_default_ttl_secs
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS),
        max_ttl_secs: cfg
            .tool_cache_max_ttl_secs
            .unwrap_or(tool_cache::DEFAULT_TOOL_CACHE_MAX_TTL_SECS),
    });

    add_libs_to_path();

    let (base_dir, override_dir) =
        find_lua_skill_dirs(&cfg).ok_or("Lua skill directory not found for --call-tools mode")?;

    let mut engine = LuaEngine::new(LuaVmPoolConfig {
        min_size: cfg.lua_vm_pool_min_size.unwrap_or(1),
        max_size: cfg.lua_vm_pool_max_size.unwrap_or(4),
        idle_ttl_secs: cfg.lua_vm_pool_idle_ttl_secs.unwrap_or(300),
    })?;
    engine.load_from_dirs(&base_dir, override_dir.as_deref())?;

    if !engine.is_skill(tool_name) {
        return Err(format!("Unknown Lua skill tool for --call-tools: {}", tool_name).into());
    }

    let result = engine
        .call_skill(tool_name, &arguments, None)
        .map_err(|error| format!("call-tools failed for {}: {}", tool_name, error))?;

    print_call_tools_result(&result)
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
