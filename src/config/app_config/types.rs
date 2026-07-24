//! Application configuration data structures and serde defaults.
//! 应用配置数据结构与 serde 默认值。

use serde::Deserialize;

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

/// Optional host overrides for managed Python and Node worker/session resource policy.
/// 受管 Python 与 Node Worker/会话资源策略的可选宿主覆盖配置。
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedRuntimeConfigSection {
    /// Maximum live workers for one exact environment and package-owner pool key.
    /// 单个精确环境与包所有者池键允许的最大活动 Worker 数量。
    pub worker_pool_max_size_per_environment: Option<usize>,
    /// Idle seconds after which an unused managed worker may be retired.
    /// 未使用受管 Worker 可被回收前的空闲秒数。
    pub worker_idle_ttl_secs: Option<u64>,
    /// Maximum launching or live persistent sessions retained by one engine.
    /// 单个引擎允许保留的启动中或活动持久会话最大数量。
    pub persistent_session_limit_per_engine: Option<usize>,
    /// Retained byte limit for each persistent-session stdout or stderr stream.
    /// 每个持久会话 stdout 或 stderr 流的保留字节上限。
    pub persistent_session_default_buffer_limit_bytes_per_stream: Option<usize>,
    /// Positive default invoke timeout in milliseconds; omission keeps the upstream unlimited default.
    /// 正数默认 invoke 超时毫秒数；省略时保留上游无限制默认值。
    pub invoke_default_timeout_ms: Option<u64>,
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

    /// Optional application root that owns host binaries, configs, logs, and the isolated `lua_runtime` package.
    /// 可选应用根目录，承载宿主二进制、配置、日志与隔离的 `lua_runtime` 包。
    pub runtime_root: Option<String>,

    /// Optional absolute or application-root-relative managed Python/Node distribution root.
    /// 可选的绝对路径或相对应用根的受管 Python/Node 发行根目录。
    pub managed_runtime_distribution_root: Option<String>,

    /// Optional absolute or application-root-relative writable managed environment root.
    /// 可选的绝对路径或相对应用根的可写受管环境根目录。
    pub managed_runtime_environment_root: Option<String>,

    /// Optional managed Python/Node worker and persistent-session policy overrides.
    /// 可选的受管 Python/Node Worker 与持久会话策略覆盖配置。
    #[serde(default)]
    pub managed_runtime_config: ManagedRuntimeConfigSection,

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
