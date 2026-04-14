use serde::Deserialize;
use std::fs;

// ============================================================
// Configuration (loaded from YAML)
// ============================================================

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    /// HTTP transport address, e.g. "127.0.0.1:19201"
    #[serde(default = "default_http_addr")]
    pub http: Option<String>,

    /// gRPC service address for plugin/management, e.g. "127.0.0.1:19202"
    #[serde(default = "default_grpc_addr")]
    pub grpc: Option<String>,

    /// LanceDb gRPC service endpoint, e.g. "http://localhost:50051"
    pub lancedb: Option<String>,

    /// Sqlite gRPC service endpoint, e.g. "http://localhost:50052"
    pub sqlite: Option<String>,

    /// VMM (VulcanMemoryMesh) gRPC service endpoint, e.g. "http://localhost:50053"
    pub vmm: Option<String>,

    /// Custom Lua skill override directory, e.g. "~/.vulcan/vulcan-mcp/lua_skills/"
    /// When set, skills in this directory override or disable system skills.
    pub lua_skills_override: Option<String>,

    /// 中文：共享工具缓存最大条目数，默认 1000。
    /// English: Maximum number of entries in the shared tool cache. Defaults to 1000.
    pub tool_cache_max_entries: Option<usize>,

    /// 中文：共享工具缓存默认 TTL（秒），默认 1800 秒。
    /// English: Default TTL in seconds for the shared tool cache. Defaults to 1800 seconds.
    pub tool_cache_default_ttl_secs: Option<u64>,

    /// 中文：共享工具缓存允许的最大 TTL（秒），默认 1800 秒。
    /// English: Maximum allowed TTL in seconds for the shared tool cache. Defaults to 1800 seconds.
    pub tool_cache_max_ttl_secs: Option<u64>,

    /// 中文：Lua 虚拟机池最小实例数，默认 1。
    /// English: Minimum number of Lua VM instances kept warm in the pool. Defaults to 1.
    pub lua_vm_pool_min_size: Option<usize>,

    /// 中文：Lua 虚拟机池最大实例数，默认 4。
    /// English: Maximum number of Lua VM instances allowed in the pool. Defaults to 4.
    pub lua_vm_pool_max_size: Option<usize>,

    /// 中文：Lua 虚拟机空闲多久后允许销毁（秒），默认 300 秒。
    /// English: Idle lifetime in seconds before an excess Lua VM can be destroyed. Defaults to 300 seconds.
    pub lua_vm_pool_idle_ttl_secs: Option<u64>,
}

fn default_http_addr() -> Option<String> {
    Some("127.0.0.1:19201".to_string())
}

fn default_grpc_addr() -> Option<String> {
    Some("127.0.0.1:19202".to_string())
}

impl Config {
    /// Load configuration from the given path.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration with priority:
    /// 1. `-config` / `--config` CLI argument
    /// 2. `<exe_parent>/configs/config.yaml`
    /// If none found, exits with error.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = find_config_arg().or_else(find_exe_parent_config);

        match config_path {
            Some(path) => {
                let config = Self::from_file(&path)?;
                eprintln!("[Config] Loaded from: {}", path);
                Ok(config)
            }
            None => {
                eprintln!("[Config] Error: No config file found.");
                eprintln!("[Config] Searched:");
                eprintln!("[Config]   - -config flag");
                eprintln!("[Config]   - <exe_parent>/configs/config.yaml");
                eprintln!("[Config] Provide config via -config flag or place configs/config.yaml in the expected location.");
                std::process::exit(1);
            }
        }
    }
}

/// Look for -config or --config in argv.
fn find_config_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "-config" || args[i] == "--config" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
    }
    None
}

/// Find configs/config.yaml in the parent directory of the running executable.
fn find_exe_parent_config() -> Option<String> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent_dir = exe_dir.parent()?;
    let config_path = parent_dir.join("configs").join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}
