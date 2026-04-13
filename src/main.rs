#[allow(dead_code)]
mod grpc_client;
mod grpc_server;
mod http_server;
mod lua_engine;
mod lua_skill;
#[allow(dead_code)]
mod protocol;
mod server;
#[allow(dead_code)]
mod session;
mod config;

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
use server::McpServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load()?;

    // Prepend output/libs/ to PATH so C dependency DLLs are found at runtime
    add_libs_to_path();

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
        server = server.with_lua_skills(&base_dir, override_dir.as_deref())?;
    }

    let http_addr = cfg.http.unwrap_or_else(|| "127.0.0.1:19201".to_string());
    let grpc_addr = cfg.grpc.unwrap_or_else(|| "127.0.0.1:19202".to_string());

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

/// Find Lua skill base and override directories.
/// Returns (base_dir, Option<override_dir>) if skills exist.
fn find_lua_skill_dirs(cfg: &config::Config) -> Option<(std::path::PathBuf, Option<std::path::PathBuf>)> {
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
        Some(home.join(".vulcan/vulcan-mcp/lua_skills").to_string_lossy().to_string())
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
    let Ok(exe_path) = std::env::current_exe() else { return };
    let Some(exe_dir) = exe_path.parent() else { return };
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
    unsafe { std::env::set_var("PATH", new_path); }
}

#[cfg(target_os = "windows")]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}
