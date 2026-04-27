use serde::Deserialize;
use std::fs;

/// Controller process mode selected when the MCP host auto-spawns one local controller.
/// 当 MCP 宿主自动拉起本地控制器时选择的进程模式。
#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpaceControllerProcessModeConfig {
    /// Keep the controller alive until an explicit external stop happens.
    /// 保持控制器持续运行，直到外部显式停止。
    Service,
    /// Allow the controller to stop itself after idle timeouts.
    /// 允许控制器在空闲超时后自行停止。
    #[default]
    Managed,
}

/// Host-level controller configuration forwarded into the controller-only LuaSkills runtime.
/// 转发给 controller-only LuaSkills 运行时的宿主级控制器配置。
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpaceControllerConfig {
    /// Optional explicit controller endpoint.
    /// 可选的显式控制器端点。
    pub endpoint: Option<String>,
    /// Whether the host may auto-spawn the controller when the endpoint is unavailable.
    /// 当控制器端点不可用时宿主是否允许自动拉起控制器。
    #[serde(default = "default_true")]
    pub auto_spawn: bool,
    /// Optional local executable path for one copied controller binary.
    /// 本地复制后的控制器可执行文件可选路径。
    pub executable_path: Option<String>,
    /// Process mode used for one auto-spawned controller process.
    /// 自动拉起控制器进程时使用的进程模式。
    #[serde(default)]
    pub process_mode: SpaceControllerProcessModeConfig,
    /// Optional minimum uptime in seconds.
    /// 可选的最小存活秒数。
    pub minimum_uptime_secs: Option<u64>,
    /// Optional idle timeout in seconds.
    /// 可选的空闲超时秒数。
    pub idle_timeout_secs: Option<u64>,
    /// Optional default lease TTL in seconds.
    /// 可选的默认租约 TTL 秒数。
    pub default_lease_ttl_secs: Option<u64>,
    /// Optional connect timeout in seconds.
    /// 可选的连接超时秒数。
    pub connect_timeout_secs: Option<u64>,
    /// Optional startup timeout in seconds.
    /// 可选的启动超时秒数。
    pub startup_timeout_secs: Option<u64>,
    /// Optional startup retry interval in milliseconds.
    /// 可选的启动重试间隔毫秒数。
    pub startup_retry_interval_ms: Option<u64>,
    /// Optional lease renew interval in seconds.
    /// 可选的租约续约间隔秒数。
    pub lease_renew_interval_secs: Option<u64>,
}

impl Default for SpaceControllerConfig {
    /// Return one safe-by-default controller configuration matching the shared managed controller model.
    /// 返回一套默认安全且匹配共享托管控制器模型的控制器配置。
    fn default() -> Self {
        Self {
            endpoint: None,
            auto_spawn: true,
            executable_path: None,
            process_mode: SpaceControllerProcessModeConfig::Managed,
            minimum_uptime_secs: None,
            idle_timeout_secs: None,
            default_lease_ttl_secs: None,
            connect_timeout_secs: None,
            startup_timeout_secs: None,
            startup_retry_interval_ms: None,
            lease_renew_interval_secs: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SkillRootConfigEntry {
    Named(NamedSkillRootConfig),
    Path(String),
}

#[derive(Deserialize, Debug, Clone)]
pub struct NamedSkillRootConfig {
    /// Stable skill-root name such as ROOT, USER, or one project identifier.
    /// 技能根的稳定名称，例如 ROOT、USER 或某个项目标识符。
    pub name: String,
    /// Physical skills root directory path.
    /// 技能根目录路径。
    pub path: String,
}

/// Optional config block that overrides the dedicated isolated `runlua` VM pool.
/// 用于覆盖隔离 `runlua` 专用虚拟机池的可选配置段。
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct RunLuaPoolConfigSection {
    /// Minimum number of isolated runlua VMs kept warm. When omitted, the upstream default is used.
    /// 隔离 runlua 虚拟机的最小常驻数量；缺失时使用上游默认值。
    pub min_size: Option<usize>,
    /// Maximum number of isolated runlua VMs allowed in the pool. When omitted, the upstream default is used.
    /// 隔离 runlua 虚拟机池允许存在的最大数量；缺失时使用上游默认值。
    pub max_size: Option<usize>,
    /// Idle TTL in seconds before excess isolated runlua VMs may be reclaimed. When omitted, the upstream default is used.
    /// 多余隔离 runlua 虚拟机允许被回收前的空闲秒数；缺失时使用上游默认值。
    pub idle_ttl_secs: Option<u64>,
}

// ============================================================
// Configuration (loaded from YAML)
// ============================================================

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Config {
    /// HTTP transport listen address, for example "127.0.0.1:19201".
    /// HTTP 传输监听地址，例如 "127.0.0.1:19201"。
    #[serde(default = "default_http_addr")]
    pub http: Option<String>,

    /// gRPC service listen address for plugin/management, for example "127.0.0.1:19202".
    /// gRPC 管理/插件服务监听地址，例如 "127.0.0.1:19202"。
    #[serde(default = "default_grpc_addr")]
    pub grpc: Option<String>,

    /// VMM (VulcanMemoryMesh) gRPC service endpoint, for example "http://localhost:50053".
    /// VMM（VulcanMemoryMesh）gRPC 服务地址，例如 "http://localhost:50053"。
    pub vmm: Option<String>,

    /// Whether the host enables the configured VMM gRPC client integration.
    /// 宿主是否启用已配置的 VMM gRPC 客户端集成。
    #[serde(default)]
    pub vmm_enable: bool,

    /// Formal skill roots for the default runtime environment, limited to ROOT, PROJECT, and USER.
    /// 默认运行环境使用的正式技能根目录，仅限 ROOT、PROJECT 与 USER。
    pub skill_roots: Option<Vec<SkillRootConfigEntry>>,

    /// Optional runtime root directory that owns configs, skills, dependencies, databases, temp, libs, and lua_packages.
    /// 宿主完整运行根目录，可统一承载 configs、skills、dependencies、databases、temp、libs 与 lua_packages。
    pub runtime_root: Option<String>,

    /// Maximum number of entries in the shared tool cache. Defaults to 1000.
    /// 共享工具缓存最大条目数，默认 1000。
    pub tool_cache_max_entries: Option<usize>,

    /// Default TTL in seconds for the shared tool cache. Defaults to 1800 seconds.
    /// 共享工具缓存默认 TTL（秒），默认 1800 秒。
    pub tool_cache_default_ttl_secs: Option<u64>,

    /// Maximum allowed TTL in seconds for the shared tool cache. Defaults to 1800 seconds.
    /// 共享工具缓存允许的最大 TTL（秒），默认 1800 秒。
    pub tool_cache_max_ttl_secs: Option<u64>,

    /// Minimum number of Lua VM instances kept warm in the pool. Defaults to 1.
    /// Lua 虚拟机池最小实例数，默认 1。
    pub lua_vm_pool_min_size: Option<usize>,

    /// Maximum number of Lua VM instances allowed in the pool. Defaults to 4.
    /// Lua 虚拟机池最大实例数，默认 4。
    pub lua_vm_pool_max_size: Option<usize>,

    /// Idle lifetime in seconds before an excess Lua VM can be destroyed. Defaults to 300 seconds.
    /// Lua 虚拟机空闲多久后允许销毁（秒），默认 300 秒。
    pub lua_vm_pool_idle_ttl_secs: Option<u64>,

    /// Optional dedicated isolated runlua VM pool settings mapped to `LuaRuntimeHostOptions.runlua_pool_config`.
    /// 映射到 `LuaRuntimeHostOptions.runlua_pool_config` 的隔离 runlua 专用虚拟机池可选配置。
    #[serde(default)]
    pub runlua_pool_config: RunLuaPoolConfigSection,

    /// Skill identifiers that the host must skip before dependency and database setup.
    /// 宿主需要在依赖与数据库初始化前跳过的技能标识符列表。
    pub ignored_skill_ids: Option<Vec<String>>,

    /// Dependency directory name, fixed as a sibling of the skills root under the same parent. Defaults to `dependencies`.
    /// 依赖目录名称，固定作为技能根父目录下的同级兄弟目录，默认 `dependencies`。
    pub dependency_dir_name: Option<String>,

    /// State directory name, fixed as a sibling of the skills root under the same parent. Defaults to `state`.
    /// 状态目录名称，固定作为技能根父目录下的同级兄弟目录，默认 `state`。
    pub state_dir_name: Option<String>,

    /// Database directory name, fixed as a sibling of the skills root under the same parent. Defaults to `databases`.
    /// 数据库目录名称，固定作为技能根父目录下的同级兄弟目录，默认 `databases`。
    pub database_dir_name: Option<String>,
    /// Shared controller configuration used by the MCP host runtime.
    /// MCP 宿主运行时使用的共享控制器配置。
    #[serde(default)]
    pub space_controller: SpaceControllerConfig,

    /// Loaded config file path captured after deserialization for stable relative-path resolution.
    /// 反序列化后记录的配置文件路径，用于稳定解析相对路径。
    #[serde(skip)]
    pub loaded_config_path: Option<String>,
}

fn default_http_addr() -> Option<String> {
    Some("127.0.0.1:19201".to_string())
}

fn default_grpc_addr() -> Option<String> {
    Some("127.0.0.1:19202".to_string())
}

/// Return the default boolean `true` used by controller auto-spawn options.
/// 返回控制器自动拉起选项使用的默认布尔值 `true`。
fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from the given YAML file path.
    /// 从指定 YAML 文件路径加载配置。
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let normalized_path = normalize_cli_config_path(path).unwrap_or_else(|| path.into());
        let content = fs::read_to_string(&normalized_path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;
        config.loaded_config_path = Some(normalized_path.to_string_lossy().to_string());
        Ok(config)
    }

    /// Load configuration strictly from the runtime-root layout or the built-in executable-side runtime layout.
    /// 严格从 runtime_root 目录布局或内置的可执行文件同级运行目录布局加载配置。
    /// The repository template lives at `runtime/configs/config.yaml` and is synced during build.
    /// 仓库内默认模板文件位于 `runtime/configs/config.yaml`，构建时会同步到输出目录。
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();
        reject_legacy_config_flag(&args)?;
        let runtime_root_arg =
            parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])?;
        let config_path = if let Some(runtime_root) = runtime_root_arg.as_deref() {
            find_runtime_root_config(runtime_root)
        } else {
            find_exe_parent_config()
        };

        match config_path {
            Some(path) => {
                let mut config = Self::from_file(&path)?;
                if let Some(runtime_root) = runtime_root_arg
                    .as_deref()
                    .and_then(normalize_cli_runtime_root_arg)
                {
                    config.runtime_root = Some(runtime_root.to_string_lossy().to_string());
                }
                eprintln!("[Config] Loaded from: {}", path);
                Ok(config)
            }
            None => {
                eprintln!("[Config] Error: No config file found.");
                eprintln!("[Config] Searched:");
                if runtime_root_arg.is_some() {
                    eprintln!(
                        "[Config]   - -runtime-root/--runtime-root + <runtime_root>/configs/config.yaml"
                    );
                } else {
                    eprintln!("[Config]   - <exe_parent>/configs/config.yaml");
                }
                eprintln!("[Config] Template source in repository: runtime/configs/config.yaml");
                eprintln!(
                    "[Config] Provide config via --runtime-root and place config at <runtime_root>/configs/config.yaml, or place the built config file at <exe_parent>/configs/config.yaml."
                );
                std::process::exit(1);
            }
        }
    }
}

/// Reject the removed legacy `--config` entry so runtime configuration stays anchored to one runtime root.
/// 拒绝已移除的历史 `--config` 入口，从而让运行时配置始终锚定到唯一运行根。
fn reject_legacy_config_flag(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.iter().any(|arg| is_removed_config_flag_arg(arg)) {
        return Err(
            "Unsupported CLI flag: -config/--config. Use --runtime-root and place config at <runtime_root>/configs/config.yaml.".into(),
        );
    }
    Ok(())
}

/// Return whether one raw argv token still uses the removed `--config` / `-config` CLI entry, including `--config=...` inline forms.
/// 返回某个原始 argv 片段是否仍在使用已移除的 `--config` / `-config` CLI 入口，包含 `--config=...` 内联写法。
fn is_removed_config_flag_arg(arg: &str) -> bool {
    arg == "-config"
        || arg == "--config"
        || arg.starts_with("-config=")
        || arg.starts_with("--config=")
}

/// Parse one CLI path flag from argv and fail early when the flag is missing a concrete value.
/// 从 argv 解析单个 CLI 路径标志，并在缺少实际取值时尽早失败。
fn parse_cli_path_flag_from_args(
    args: &[String],
    flags: &[&str],
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    for i in 0..args.len() {
        if let Some((flag, value)) = parse_inline_cli_path_flag_value(args[i].as_str(), flags) {
            if value.is_empty() {
                return Err(format!("{flag} requires a value").into());
            }
            return Ok(Some(value.to_string()));
        }
        if flags.iter().any(|flag| args[i] == *flag) {
            let flag = args[i].as_str();
            let Some(value) = args.get(i + 1) else {
                return Err(format!("{flag} requires a value").into());
            };
            if value.starts_with("--") || value.starts_with('-') {
                return Err(format!("{flag} requires a value").into());
            }
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

/// Parse one inline `--flag=value` style CLI path token and return the matched canonical flag with its value.
/// 解析一条 `--flag=value` 风格的内联 CLI 路径参数，并返回匹配到的规范标志及其取值。
fn parse_inline_cli_path_flag_value<'a>(
    arg: &'a str,
    flags: &[&'a str],
) -> Option<(&'a str, &'a str)> {
    flags.iter().find_map(|flag| {
        arg.strip_prefix(flag)
            .and_then(|remainder| remainder.strip_prefix('='))
            .map(|value| (*flag, value))
    })
}

/// Resolve the config path under one explicit runtime root.
/// 从显式给定的运行根目录下解析配置文件路径。
fn find_runtime_root_config(runtime_root: &str) -> Option<String> {
    let config_path = normalize_cli_runtime_root_arg(runtime_root)?
        .join("configs")
        .join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Normalize one CLI runtime-root argument so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI runtime-root 参数，使相对路径立即锚定到当前工作目录。
fn normalize_cli_runtime_root_arg(runtime_root: &str) -> Option<std::path::PathBuf> {
    let candidate_root = std::path::PathBuf::from(runtime_root);
    if candidate_root.is_absolute() {
        Some(candidate_root)
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(candidate_root))
    }
}

/// Normalize one CLI config path so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI 配置文件路径，使相对路径立即锚定到当前工作目录。
fn normalize_cli_config_path(config_path: &str) -> Option<std::path::PathBuf> {
    let candidate_path = std::path::PathBuf::from(config_path);
    if candidate_path.is_absolute() {
        Some(candidate_path)
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(candidate_path))
    }
}

/// Find configs/config.yaml in the parent output directory of the running executable.
/// 在运行中可执行文件的上级输出目录中查找 configs/config.yaml。
/// The repository template lives in runtime/configs/config.yaml and is copied here during build.
/// 仓库模板文件位于 runtime/configs/config.yaml，构建后会复制到这里。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative runtime-root CLI arguments should be normalized against the current working directory immediately.
    /// 相对 runtime-root CLI 参数应当立即相对当前工作目录完成规范化。
    #[test]
    fn normalize_cli_runtime_root_arg_anchors_relative_paths_to_cwd() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let normalized =
            normalize_cli_runtime_root_arg("runtime").expect("runtime root should normalize");
        assert_eq!(normalized, cwd.join("runtime"));
    }

    /// Relative config CLI arguments should be normalized against the current working directory immediately.
    /// 相对配置文件 CLI 参数应当立即相对当前工作目录完成规范化。
    #[test]
    fn normalize_cli_config_path_anchors_relative_paths_to_cwd() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let normalized = normalize_cli_config_path("runtime/configs/config.yaml")
            .expect("config path should normalize");
        assert_eq!(
            normalized,
            cwd.join("runtime").join("configs").join("config.yaml")
        );
    }

    /// Legacy config flags should be rejected so runtime config discovery stays anchored to one runtime root.
    /// 历史 config 标志应被拒绝，从而让运行时配置发现始终锚定到唯一运行根。
    #[test]
    fn reject_legacy_config_flag_reports_runtime_root_only_model() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--config".to_string(),
            "runtime/configs/config.yaml".to_string(),
        ];
        let error =
            reject_legacy_config_flag(&args).expect_err("legacy config flag should be rejected");
        assert!(
            error.to_string().contains("Unsupported CLI flag"),
            "unexpected error: {error}"
        );
    }

    /// Inline `--config=...` forms should be rejected too so removed config entrypoints cannot slip through argv parsing.
    /// 内联 `--config=...` 形式也应被拒绝，避免已移除的配置入口从 argv 解析中漏过去。
    #[test]
    fn reject_legacy_config_flag_rejects_inline_equals_form() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--config=runtime/configs/config.yaml".to_string(),
        ];
        let error =
            reject_legacy_config_flag(&args).expect_err("inline legacy config flag should fail");
        assert!(
            error.to_string().contains("Unsupported CLI flag"),
            "unexpected error: {error}"
        );
    }

    /// CLI runtime-root flags should fail early when the next argv token is another flag instead of a path.
    /// 当 CLI runtime-root 标志后面直接跟着另一个标志时，应尽早失败。
    #[test]
    fn parse_cli_path_flag_rejects_missing_runtime_root_value() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--runtime-root".to_string(),
            "--stdio".to_string(),
        ];
        let error = parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])
            .expect_err("missing runtime-root value should fail");
        assert!(
            error
                .to_string()
                .contains("--runtime-root requires a value"),
            "unexpected error: {error}"
        );
    }

    /// Inline `--runtime-root=...` forms should be accepted so runtime-root parsing stays consistent with common CLI conventions.
    /// 内联 `--runtime-root=...` 形式应被接受，从而让运行根解析与常见 CLI 约定保持一致。
    #[test]
    fn parse_cli_path_flag_accepts_inline_runtime_root_value() {
        let args = vec![
            "vulcan-mcp.exe".to_string(),
            "--runtime-root=output".to_string(),
        ];
        let runtime_root =
            parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])
                .expect("inline runtime-root should parse");
        assert_eq!(runtime_root, Some("output".to_string()));
    }

    /// Inline `--runtime-root=` forms should still fail early when the value is empty.
    /// 内联 `--runtime-root=` 在取值为空时也应尽早失败。
    #[test]
    fn parse_cli_path_flag_rejects_empty_inline_runtime_root_value() {
        let args = vec!["vulcan-mcp.exe".to_string(), "--runtime-root=".to_string()];
        let error = parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])
            .expect_err("empty inline runtime-root should fail");
        assert!(
            error
                .to_string()
                .contains("--runtime-root requires a value"),
            "unexpected error: {error}"
        );
    }

    /// Config YAML should deserialize the dedicated runlua pool block so hosts can override isolated luaexec pool behavior.
    /// 配置 YAML 应能反序列化专用 runlua 池配置段，以便宿主覆盖隔离 luaexec 池行为。
    #[test]
    fn config_deserializes_runlua_pool_config_block() {
        let config: Config = serde_yaml::from_str(
            r#"
runlua_pool_config:
  min_size: 2
  max_size: 6
  idle_ttl_secs: 90
"#,
        )
        .expect("runlua pool config should deserialize");

        assert_eq!(
            config.runlua_pool_config,
            RunLuaPoolConfigSection {
                min_size: Some(2),
                max_size: Some(6),
                idle_ttl_secs: Some(90),
            }
        );
    }
}
