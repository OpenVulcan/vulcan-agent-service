use serde::Deserialize;
use std::fs;

// ============================================================
// Configuration (loaded from YAML)
// ============================================================

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    /// 中文：HTTP 传输监听地址，例如 "127.0.0.1:19201"。
    /// English: HTTP transport listen address, for example "127.0.0.1:19201".
    #[serde(default = "default_http_addr")]
    pub http: Option<String>,

    /// 中文：gRPC 管理/插件服务监听地址，例如 "127.0.0.1:19202"。
    /// English: gRPC service listen address for plugin/management, for example "127.0.0.1:19202".
    #[serde(default = "default_grpc_addr")]
    pub grpc: Option<String>,

    /// 中文：VMM（VulcanMemoryMesh）gRPC 服务地址，例如 "http://localhost:50053"。
    /// English: VMM (VulcanMemoryMesh) gRPC service endpoint, for example "http://localhost:50053".
    pub vmm: Option<String>,

    /// 中文：自定义 Lua Skill 覆盖目录，例如 "~/.vulcan/vulcan-mcp/lua_skills/"；
    /// 设置后，该目录中的技能可覆盖或禁用系统内置技能。
    /// English: Custom Lua skill override directory, for example "~/.vulcan/vulcan-mcp/lua_skills/".
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
    /// 中文：从指定 YAML 文件路径加载配置。
    /// English: Load configuration from the given YAML file path.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// 中文：按优先级加载配置：
    /// 1. `-config` / `--config` 命令行参数；
    /// 2. `<exe_parent>/configs/config.yaml` 运行时输出目录配置。
    /// 仓库内默认模板文件位于 `runtime/configs/config.yaml`，构建时会同步到输出目录。
    /// 如果未找到配置，则直接退出。
    /// English: Load configuration with the following priority:
    /// 1. `-config` / `--config` CLI argument;
    /// 2. `<exe_parent>/configs/config.yaml` in the runtime output directory.
    /// The repository template lives at `runtime/configs/config.yaml` and is synced during build.
    /// Exit immediately if no config file is found.
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
                eprintln!("[Config] Template source in repository: runtime/configs/config.yaml");
                eprintln!(
                    "[Config] Provide config via -config flag or place the built config file at <exe_parent>/configs/config.yaml."
                );
                std::process::exit(1);
            }
        }
    }
}

/// 中文：在命令行参数中查找 -config 或 --config。
/// English: Look for -config or --config in argv.
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

/// 中文：在运行中可执行文件的上级输出目录中查找 configs/config.yaml。
/// 仓库模板文件位于 runtime/configs/config.yaml，构建后会复制到这里。
/// English: Find configs/config.yaml in the parent output directory of the running executable.
/// The repository template lives in runtime/configs/config.yaml and is copied here during build.
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
