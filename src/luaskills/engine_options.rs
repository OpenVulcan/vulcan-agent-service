use crate::config::{Config, SpaceControllerProcessModeConfig};
use crate::support::runtime_logging::{error as log_error, info as log_info, warn as log_warn};
use crate::support::temp_maintenance::ensure_runtime_temp_dir;
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::{
    DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS, DEFAULT_TOOL_CACHE_MAX_ENTRIES,
    DEFAULT_TOOL_CACHE_MAX_TTL_SECS, LuaEngineOptions, LuaRuntimeCapabilityOptions,
    LuaRuntimeDatabaseCallbackMode, LuaRuntimeDatabaseProviderMode, LuaRuntimeHostOptions,
    LuaRuntimeSpaceControllerOptions, LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig,
    RuntimeLogCallback, RuntimeLogEvent, RuntimeLogLevel, ToolCacheConfig, set_log_callback,
};
use std::path::PathBuf;
use std::sync::Arc;

use super::runtime_paths::{
    resolve_host_ffi_root, resolve_host_provided_tool_root, resolve_lua_packages_dir,
    resolve_runtime_resources_dir, resolve_runtime_root_from_config,
    resolve_skill_config_file_path,
};

/// Built-in AI memory skill that is superseded when a VMM gRPC endpoint is configured.
/// 配置 VMM gRPC 端点时会被替代的内置 AI 记忆技能。
const VMM_REPLACED_AI_MEMORY_SKILL_ID: &str = "vulcan-ai-memory";

/// Install the host-side LuaSkills log callback so runtime events flow into the MCP host logger.
/// 宿主侧安装 LuaSkills 日志回调，让运行时事件统一流入 MCP 宿主日志器。
pub fn install_luaskills_log_callback() {
    let callback: RuntimeLogCallback = Arc::new(|event: &RuntimeLogEvent| match event.level {
        RuntimeLogLevel::Info => log_info(event.message.as_str()),
        RuntimeLogLevel::Warn => log_warn(event.message.as_str()),
        RuntimeLogLevel::Error => log_error(event.message.as_str()),
    });
    set_log_callback(Some(callback));
}

/// Build one LuaSkills engine options object from the current MCP host runtime layout.
/// 基于当前 MCP 宿主运行目录布局构造一份 LuaSkills 引擎选项对象。
pub fn build_luaskills_engine_options(
    config: &Config,
    pool_config: LuaVmPoolConfig,
    cache_config: ToolCacheConfig,
) -> Result<LuaEngineOptions, Box<dyn std::error::Error>> {
    let runtime_root =
        resolve_runtime_root_from_config(config)?.ok_or("Failed to resolve runtime root")?;
    let runtime_temp_root = ensure_runtime_temp_dir()?;
    let temp_root = runtime_temp_root.join("mcp");
    let download_cache_root = Some(runtime_temp_root.join("downloads"));
    let lua_packages_dir = resolve_lua_packages_dir(&runtime_root)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    // The upstream runtime has removed the legacy `luaexec_program`; isolated runlua now only uses the in-process dedicated VM pool.
    // 上游运行时已移除历史 `luaexec_program`；隔离 runlua 现仅使用进程内独立 VM 池。
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(temp_root.clone()),
        resources_dir: resolve_runtime_resources_dir(&runtime_root)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
        lua_packages_dir: lua_packages_dir.clone(),
        host_provided_tool_root: resolve_host_provided_tool_root(&runtime_root)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
        host_provided_lua_root: lua_packages_dir,
        host_provided_ffi_root: resolve_host_ffi_root(&runtime_root)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
        download_cache_root,
        dependency_dir_name: config
            .dependency_dir_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "dependencies".to_string()),
        state_dir_name: config
            .state_dir_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "state".to_string()),
        database_dir_name: config
            .database_dir_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "databases".to_string()),
        allow_network_download: true,
        github_base_url: std::env::var("VULCAN_GITHUB_BASE_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        github_api_base_url: std::env::var("VULCAN_GITHUB_API_BASE_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        sqlite_library_path: None,
        sqlite_provider_mode: LuaRuntimeDatabaseProviderMode::SpaceController,
        sqlite_callback_mode: LuaRuntimeDatabaseCallbackMode::Standard,
        lancedb_library_path: None,
        lancedb_provider_mode: LuaRuntimeDatabaseProviderMode::SpaceController,
        lancedb_callback_mode: LuaRuntimeDatabaseCallbackMode::Standard,
        space_controller: resolve_space_controller_options(config, &runtime_root)?,
        cache_config: Some(cache_config),
        runlua_pool_config: resolve_runlua_pool_config(config),
        skill_config_file_path: Some(
            resolve_skill_config_file_path(&runtime_root)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
        ),
        reserved_entry_names: host_reserved_tool_names(),
        ignored_skill_ids: resolve_ignored_skill_ids(config),
        capabilities: LuaRuntimeCapabilityOptions {
            enable_skill_management_bridge: false,
        },
        ..LuaRuntimeHostOptions::default()
    };
    Ok(LuaEngineOptions::new(pool_config, host_options))
}

/// Resolve one optional dedicated isolated runlua pool override from host config while preserving upstream defaults when the block is absent.
/// 从宿主配置解析可选的隔离 runlua 专用池覆盖配置，并在整个配置段缺失时保留上游默认值。
fn resolve_runlua_pool_config(config: &Config) -> Option<LuaRuntimeRunLuaPoolConfig> {
    let configured_pool = &config.runlua_pool_config;
    if configured_pool.min_size.is_none()
        && configured_pool.max_size.is_none()
        && configured_pool.idle_ttl_secs.is_none()
    {
        return None;
    }

    Some(LuaRuntimeRunLuaPoolConfig {
        min_size: configured_pool.min_size.unwrap_or(1),
        max_size: configured_pool.max_size.unwrap_or(4),
        idle_ttl_secs: configured_pool.idle_ttl_secs.unwrap_or(60),
    })
}

/// Map one host config controller process mode into the LuaSkills controller process mode enum.
/// 将宿主配置中的控制器进程模式映射为 LuaSkills 控制器进程模式枚举。
fn map_space_controller_process_mode(
    mode: SpaceControllerProcessModeConfig,
) -> LuaRuntimeSpaceControllerProcessMode {
    match mode {
        SpaceControllerProcessModeConfig::Service => LuaRuntimeSpaceControllerProcessMode::Service,
        SpaceControllerProcessModeConfig::Managed => LuaRuntimeSpaceControllerProcessMode::Managed,
    }
}

/// Resolve the effective controller executable path from explicit config first and conventional runtime `bin/` path second.
/// 优先使用显式配置、其次使用约定的运行时 `bin/` 路径来解析控制器可执行文件路径。
fn resolve_space_controller_executable_path(
    config: &Config,
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    if let Some(configured_path) = config
        .space_controller
        .executable_path
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let candidate_path = PathBuf::from(configured_path);
        let normalized_path = if candidate_path.is_absolute() {
            candidate_path
        } else {
            runtime_root.join(candidate_path)
        };
        if !normalized_path.exists() {
            return Err(format!(
                "space_controller.executable_path does not exist: {}",
                normalized_path.display()
            ));
        }
        if !normalized_path.is_file() {
            return Err(format!(
                "space_controller.executable_path is not a file: {}",
                normalized_path.display()
            ));
        }
        return Ok(Some(normalized_path));
    }
    let copied_path = runtime_root
        .join("bin")
        .join(space_controller_executable_file_name());
    if !copied_path.exists() {
        return Ok(None);
    }
    if !copied_path.is_file() {
        return Err(format!(
            "space_controller fallback executable path is not a file: {}",
            copied_path.display()
        ));
    }
    Ok(Some(copied_path))
}

/// Resolve the host-facing shared controller options from the MCP config and runtime layout.
/// 基于 MCP 配置与运行时布局解析宿主侧共享控制器选项。
pub(super) fn resolve_space_controller_options(
    config: &Config,
    runtime_root: &std::path::Path,
) -> Result<LuaRuntimeSpaceControllerOptions, String> {
    validate_space_controller_endpoint(config)?;
    let defaults = LuaRuntimeSpaceControllerOptions::default();
    Ok(LuaRuntimeSpaceControllerOptions {
        endpoint: config
            .space_controller
            .endpoint
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        auto_spawn: config.space_controller.auto_spawn,
        executable_path: resolve_space_controller_executable_path(config, runtime_root)?,
        process_mode: map_space_controller_process_mode(config.space_controller.process_mode),
        minimum_uptime_secs: config
            .space_controller
            .minimum_uptime_secs
            .unwrap_or(defaults.minimum_uptime_secs),
        idle_timeout_secs: config
            .space_controller
            .idle_timeout_secs
            .unwrap_or(defaults.idle_timeout_secs),
        default_lease_ttl_secs: config
            .space_controller
            .default_lease_ttl_secs
            .unwrap_or(defaults.default_lease_ttl_secs),
        connect_timeout_secs: config
            .space_controller
            .connect_timeout_secs
            .unwrap_or(defaults.connect_timeout_secs),
        startup_timeout_secs: config
            .space_controller
            .startup_timeout_secs
            .unwrap_or(defaults.startup_timeout_secs),
        startup_retry_interval_ms: config
            .space_controller
            .startup_retry_interval_ms
            .unwrap_or(defaults.startup_retry_interval_ms),
        lease_renew_interval_secs: config
            .space_controller
            .lease_renew_interval_secs
            .unwrap_or(defaults.lease_renew_interval_secs),
    })
}

/// Return the platform-specific controller executable filename used by the MCP host runtime.
/// 返回 MCP 宿主运行时使用的平台相关控制器可执行文件名。
pub(super) fn space_controller_executable_file_name() -> &'static str {
    if cfg!(windows) {
        "vldb-controller.exe"
    } else {
        "vldb-controller"
    }
}

/// Validate the controller endpoint combination that the MCP host is willing to auto-spawn locally.
/// 校验 MCP 宿主允许本地自动拉起控制器时使用的端点组合是否合法。
fn validate_space_controller_endpoint(config: &Config) -> Result<(), String> {
    if !config.space_controller.auto_spawn {
        return Ok(());
    }
    let Some(endpoint) = config
        .space_controller
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if endpoint_supports_local_auto_spawn(endpoint) {
        return Ok(());
    }
    Err(format!(
        "space_controller.auto_spawn=true requires one local bindable endpoint, got: {}",
        endpoint
    ))
}

/// Decide whether one controller endpoint string can be auto-spawned safely on the local machine.
/// 判断一条控制器端点字符串是否能在本机安全地自动拉起。
fn endpoint_supports_local_auto_spawn(endpoint: &str) -> bool {
    let trimmed = endpoint.trim();
    if let Some(port) = trimmed.strip_prefix(':') {
        return !port.is_empty() && port.chars().all(|value| value.is_ascii_digit());
    }
    if !trimmed.is_empty() && trimmed.chars().all(|value| value.is_ascii_digit()) {
        return true;
    }

    let authority = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(trimmed)
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .trim()
    } else {
        trimmed
    };

    if let Some(port) = authority.strip_prefix("localhost:") {
        return !port.is_empty() && port.chars().all(|value| value.is_ascii_digit());
    }
    if authority.starts_with("127.0.0.1:") || authority.starts_with("0.0.0.0:") {
        return authority
            .split(':')
            .nth(1)
            .map(|port| !port.is_empty() && port.chars().all(|value| value.is_ascii_digit()))
            .unwrap_or(false);
    }
    if authority.starts_with("[::1]:") {
        return authority
            .split("]:")
            .nth(1)
            .map(|port| !port.is_empty() && port.chars().all(|value| value.is_ascii_digit()))
            .unwrap_or(false);
    }

    false
}

/// Resolve the host-level skill ignore list and add the AI memory skill when VMM is configured.
/// 解析宿主级技能忽略列表，并在配置 VMM 时自动加入 AI 记忆技能。
fn resolve_ignored_skill_ids(config: &Config) -> Vec<String> {
    let mut ignored_skill_ids = Vec::new();
    if let Some(configured) = &config.ignored_skill_ids {
        for item in configured {
            push_unique_skill_id(&mut ignored_skill_ids, item);
        }
    }

    if config.vmm_enable
        && config
            .vmm
            .as_ref()
            .map(|endpoint| !endpoint.trim().is_empty())
            .unwrap_or(false)
    {
        push_unique_skill_id(&mut ignored_skill_ids, VMM_REPLACED_AI_MEMORY_SKILL_ID);
    }

    ignored_skill_ids
}

/// Push one non-empty skill identifier while preserving order and avoiding case-insensitive duplicates.
/// 加入一个非空技能标识符，同时保持顺序并避免大小写不敏感的重复项。
fn push_unique_skill_id(skill_ids: &mut Vec<String>, skill_id: &str) {
    let normalized = skill_id.trim();
    if normalized.is_empty() {
        return;
    }
    if skill_ids
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(normalized))
    {
        return;
    }
    skill_ids.push(normalized.to_string());
}

/// Resolve the host-side cache policy that should be injected into the LuaSkills library.
/// 解析应由宿主注入到 LuaSkills 库中的缓存策略。
pub fn build_luaskills_cache_config(
    max_entries: Option<usize>,
    default_ttl_secs: Option<u64>,
    max_ttl_secs: Option<u64>,
) -> ToolCacheConfig {
    ToolCacheConfig {
        max_entries: max_entries.unwrap_or(DEFAULT_TOOL_CACHE_MAX_ENTRIES),
        default_ttl_secs: default_ttl_secs.unwrap_or(DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS),
        max_ttl_secs: max_ttl_secs.unwrap_or(DEFAULT_TOOL_CACHE_MAX_TTL_SECS),
    }
}

/// Return the host-owned MCP tool names that must stay reserved from LuaSkills canonical entry generation.
/// 返回必须从 LuaSkills canonical 入口生成中保留的宿主 MCP 工具名称集合。
pub fn host_reserved_tool_names() -> Vec<String> {
    vec![
        "vulcan-help-list".to_string(),
        "vulcan-help-detail".to_string(),
        "reload_vulcan_mcp_configs".to_string(),
        "luaskill-config".to_string(),
        "skill-manager".to_string(),
    ]
}
