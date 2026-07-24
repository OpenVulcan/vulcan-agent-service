use crate::config::{Config, SpaceControllerProcessModeConfig};
use crate::support::runtime_logging::{error as log_error, info as log_info, warn as log_warn};
use crate::support::temp_maintenance::ensure_runtime_temp_dir_for_root;
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::{
    DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS, DEFAULT_TOOL_CACHE_MAX_ENTRIES,
    DEFAULT_TOOL_CACHE_MAX_TTL_SECS, LuaEngineOptions, LuaRuntimeCapabilityOptions,
    LuaRuntimeDatabaseCallbackMode, LuaRuntimeDatabaseProviderMode, LuaRuntimeHostOptions,
    LuaRuntimeManagedRuntimeConfig, LuaRuntimeSpaceControllerOptions,
    LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig, RuntimeLogCallback, RuntimeLogEvent,
    RuntimeLogLevel, ToolCacheConfig, set_log_callback,
};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::runtime_paths::{
    resolve_application_root_from_config, resolve_luaskills_runtime_root_from_config,
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
    // RuntimeRoot is the isolated child package and is the sole authority for every fixed LuaSkills path.
    // RuntimeRoot 是隔离的子运行时包，也是全部固定 LuaSkills 路径的唯一权威来源。
    let runtime_root = resolve_luaskills_runtime_root_from_config(config)?
        .ok_or("Failed to resolve LuaSkills runtime root")?;
    validate_luaskills_runtime_layout(&runtime_root)?;
    resolve_skill_config_file_path(&runtime_root)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    // Temp initialization happens before engine construction so path failures surface during startup.
    // 临时目录在引擎构造前初始化，使路径错误在启动阶段直接暴露。
    ensure_runtime_temp_dir_for_root(&runtime_root)?;
    // HostOptions starts from the upstream fixed-layout constructor; manually derived legacy paths are intentionally absent.
    // HostOptions 从上游固定布局构造器开始；不再手工派生历史路径。
    let mut host_options = LuaRuntimeHostOptions::with_runtime_root(runtime_root.clone());
    host_options.managed_runtime_distribution_root = resolve_managed_runtime_root_override(
        config,
        config.managed_runtime_distribution_root.as_deref(),
        "managed_runtime_distribution_root",
        true,
    )?;
    host_options.managed_runtime_environment_root = resolve_managed_runtime_root_override(
        config,
        config.managed_runtime_environment_root.as_deref(),
        "managed_runtime_environment_root",
        false,
    )?;
    host_options.managed_runtime_config = resolve_managed_runtime_config(config)?;
    host_options.allow_network_download = true;
    host_options.github_base_url = optional_env_text("VULCAN_GITHUB_BASE_URL")?;
    host_options.github_api_base_url = optional_env_text("VULCAN_GITHUB_API_BASE_URL")?;
    host_options.sqlite_library_path = None;
    host_options.sqlite_provider_mode = LuaRuntimeDatabaseProviderMode::SpaceController;
    host_options.sqlite_callback_mode = LuaRuntimeDatabaseCallbackMode::Standard;
    host_options.lancedb_library_path = None;
    host_options.lancedb_provider_mode = LuaRuntimeDatabaseProviderMode::SpaceController;
    host_options.lancedb_callback_mode = LuaRuntimeDatabaseCallbackMode::Standard;
    host_options.space_controller = resolve_space_controller_options(config, &runtime_root)?;
    host_options.cache_config = Some(cache_config);
    host_options.runlua_pool_config = resolve_runlua_pool_config(config);
    host_options.reserved_entry_names = host_reserved_tool_names();
    host_options.ignored_skill_ids = resolve_ignored_skill_ids(config);
    host_options.capabilities = LuaRuntimeCapabilityOptions {
        enable_skill_management_bridge: false,
        enable_managed_io_compat: true,
    };
    Ok(LuaEngineOptions::new(pool_config, host_options))
}

/// Validate the shape of every fixed LuaSkills 0.5.4 directory before engine construction.
/// 在引擎构造前校验 LuaSkills 0.5.4 全部固定目录的形态。
/// Parameters: `runtime_root` is the existing isolated LuaSkills package root.
/// 参数：`runtime_root` 是已存在的隔离 LuaSkills 包根目录。
/// Returns unit when every present path is a directory, otherwise an explicit shape or metadata error.
/// 所有已存在路径均为目录时返回空值，否则返回显式形态或元数据错误。
fn validate_luaskills_runtime_layout(
    runtime_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // FixedDirectories matches LuaRuntimeLayout plus package-owned persistent and license directories.
    // FixedDirectories 对齐 LuaRuntimeLayout，并补充包拥有的持久化与许可证目录。
    let fixed_directories = [
        "bin",
        "libs",
        "lua_packages",
        "resources",
        "skills",
        "temp",
        "dependencies",
        "state",
        "databases",
        "config",
        "system_lua_lib",
        "licenses",
    ];
    for directory_name in fixed_directories {
        // DirectoryPath is derived from the authoritative runtime root and one fixed name only.
        // DirectoryPath 仅由权威运行时根与单个固定名称推导。
        let directory_path = runtime_root.join(directory_name);
        match std::fs::metadata(&directory_path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "LuaSkills runtime {} path is not a directory: {}",
                    directory_name,
                    directory_path.display()
                )
                .into());
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect LuaSkills runtime {} path {}: {}",
                    directory_name,
                    directory_path.display(),
                    error
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Resolve one optional managed-runtime root override into an absolute host path.
/// 将一个可选受管运行时根覆盖值解析为绝对宿主路径。
/// Parameters: `config` supplies the selected application root for relative values.
/// 参数：`config` 为相对值提供已选应用根。
/// Parameters: `configured_value` is the exact optional YAML field value.
/// 参数：`configured_value` 是 YAML 字段的精确可选值。
/// Parameters: `field_name` identifies the field in diagnostics.
/// 参数：`field_name` 用于在诊断中标识字段。
/// Parameters: `must_exist` requires a read-only distribution root while allowing a not-yet-created environment root.
/// 参数：`must_exist` 用于要求只读发行根已存在，同时允许环境根尚未创建。
/// Returns an absolute override, `None` when omitted, or an explicit validation error.
/// 返回绝对覆盖路径、省略时的 `None`，或显式校验错误。
fn resolve_managed_runtime_root_override(
    config: &Config,
    configured_value: Option<&str>,
    field_name: &str,
    must_exist: bool,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    // ConfiguredValue distinguishes omission from an invalid blank override.
    // ConfiguredValue 区分字段省略与无效空白覆盖值。
    let Some(configured_value) = configured_value else {
        return Ok(None);
    };
    // TrimmedValue is the exact filesystem spelling accepted from configuration.
    // TrimmedValue 是从配置接受的精确文件系统写法。
    let trimmed_value = configured_value.trim();
    if trimmed_value.is_empty() {
        return Err(format!("{field_name} must not be blank when configured").into());
    }
    // CandidatePath preserves absolute overrides and anchors relative overrides to the application root.
    // CandidatePath 保留绝对覆盖值，并将相对覆盖值锚定到应用根。
    let candidate_path = PathBuf::from(trimmed_value);
    // ResolvedPath is absolute regardless of whether configuration used an absolute or relative spelling.
    // ResolvedPath 无论配置使用绝对还是相对写法都保持绝对路径。
    let resolved_path = if candidate_path.is_absolute() {
        candidate_path
    } else {
        // ApplicationRoot is required because relative managed roots must never depend on process cwd.
        // ApplicationRoot 是必需项，因为相对受管根绝不能依赖进程工作目录。
        let application_root = resolve_application_root_from_config(config)?.ok_or_else(|| {
            format!("{field_name} is relative but no application runtime_root exists")
        })?;
        application_root.join(candidate_path)
    };
    match std::fs::metadata(&resolved_path) {
        Ok(metadata) if metadata.is_dir() => {
            // CanonicalPath pins existing roots before LuaSkills captures their directory identity.
            // CanonicalPath 在 LuaSkills 捕获目录身份前固定已存在根目录。
            let canonical_path = std::fs::canonicalize(&resolved_path)?;
            Ok(Some(canonical_path))
        }
        Ok(_) => Err(format!(
            "{field_name} is not a directory: {}",
            resolved_path.display()
        )
        .into()),
        Err(error) if error.kind() == ErrorKind::NotFound && !must_exist => Ok(Some(resolved_path)),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(format!("{field_name} does not exist: {}", resolved_path.display()).into())
        }
        Err(error) => Err(format!(
            "failed to inspect {field_name} {}: {}",
            resolved_path.display(),
            error
        )
        .into()),
    }
}

/// Merge host-managed runtime policy overrides onto the upstream 0.5.4 defaults.
/// 将宿主受管运行时策略覆盖项合并到上游 0.5.4 默认值。
/// Parameters: `config` contains optional positive policy limits.
/// 参数：`config` 包含可选的正数策略限制。
/// Returns one validated policy or an explicit field-qualified error.
/// 返回一份已校验策略，或带明确字段名的错误。
fn resolve_managed_runtime_config(
    config: &Config,
) -> Result<LuaRuntimeManagedRuntimeConfig, Box<dyn std::error::Error>> {
    // Defaults are sourced from LuaSkills so host behavior follows the exact dependency version.
    // Defaults 来自 LuaSkills，使宿主行为严格跟随依赖版本。
    let defaults = LuaRuntimeManagedRuntimeConfig::default();
    // ManagedConfig applies only explicitly configured values over those upstream defaults.
    // ManagedConfig 仅把显式配置值覆盖到上游默认值之上。
    let managed_config = LuaRuntimeManagedRuntimeConfig {
        worker_pool_max_size_per_environment: config
            .managed_runtime_config
            .worker_pool_max_size_per_environment
            .unwrap_or(defaults.worker_pool_max_size_per_environment),
        worker_idle_ttl_secs: config
            .managed_runtime_config
            .worker_idle_ttl_secs
            .unwrap_or(defaults.worker_idle_ttl_secs),
        persistent_session_limit_per_engine: config
            .managed_runtime_config
            .persistent_session_limit_per_engine
            .unwrap_or(defaults.persistent_session_limit_per_engine),
        persistent_session_default_buffer_limit_bytes_per_stream: config
            .managed_runtime_config
            .persistent_session_default_buffer_limit_bytes_per_stream
            .unwrap_or(defaults.persistent_session_default_buffer_limit_bytes_per_stream),
        invoke_default_timeout_ms: config
            .managed_runtime_config
            .invoke_default_timeout_ms
            .or(defaults.invoke_default_timeout_ms),
    };
    managed_config.validate()?;
    Ok(managed_config)
}

/// Read one optional text environment variable without hiding invalid values.
/// 读取一个可选文本环境变量，同时不隐藏无效值。
///
/// Parameters: `name` is the environment variable name to read.
/// 参数：`name` 是需要读取的环境变量名称。
///
/// Returns: trimmed non-empty text, `None` when absent or blank, or an error for invalid Unicode.
/// 返回：去空白后的非空文本；缺失或空白时返回 `None`；Unicode 无效时返回错误。
fn optional_env_text(name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    optional_env_text_from_result(name, std::env::var(name)).map_err(Into::into)
}

/// Normalize the raw result of one optional text environment lookup.
/// 规范化一次可选文本环境变量查询的原始结果。
///
/// Parameters: `name` is the environment variable name used in diagnostics.
/// 参数：`name` 是用于诊断信息的环境变量名称。
///
/// Parameters: `value` is the raw result returned by `std::env::var`.
/// 参数：`value` 是 `std::env::var` 返回的原始结果。
///
/// Returns: trimmed non-empty text, `None` when absent or blank, or an error for invalid Unicode.
/// 返回：去空白后的非空文本；缺失或空白时返回 `None`；Unicode 无效时返回错误。
fn optional_env_text_from_result(
    name: &str,
    value: Result<String, std::env::VarError>,
) -> Result<Option<String>, String> {
    match value {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(value)) => Err(format!(
            "environment variable {} is not valid Unicode: {:?}",
            name, value
        )),
    }
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
        return require_controller_executable_file_path(
            &normalized_path,
            "space_controller.executable_path",
        )
        .map(Some);
    }
    let copied_path = runtime_root
        .join("bin")
        .join(space_controller_executable_file_name());
    optional_controller_executable_file_path(
        &copied_path,
        "space_controller fallback executable path",
    )
}

/// Require one explicitly configured controller executable path to exist and be a file.
/// 要求一个显式配置的控制器可执行文件路径存在且为文件。
/// Parameters: `path` is the normalized controller executable path to inspect.
/// 参数：`path` 是需要检查的规范化控制器可执行文件路径。
/// Parameters: `path_label` names the configuration source in diagnostics.
/// 参数：`path_label` 用于在诊断中标识配置来源。
/// Returns the executable path when valid, or an inspection/shape/missing error.
/// 路径有效时返回可执行文件路径，否则返回检查/形态/缺失错误。
fn require_controller_executable_file_path(
    path: &Path,
    path_label: &str,
) -> Result<PathBuf, String> {
    optional_controller_executable_file_path(path, path_label)?
        .ok_or_else(|| format!("{} does not exist: {}", path_label, path.display()))
}

/// Inspect one optional controller executable path without hiding metadata or shape errors.
/// 检查一个可选控制器可执行文件路径，且不隐藏元数据或形态错误。
/// Parameters: `path` is the controller executable path to inspect.
/// 参数：`path` 是需要检查的控制器可执行文件路径。
/// Parameters: `path_label` names the path source in diagnostics.
/// 参数：`path_label` 用于在诊断中标识路径来源。
/// Returns the path when it is a file, `None` when absent, or an inspection/shape error.
/// 当路径为文件时返回该路径，缺失时返回 `None`，否则返回检查/形态错误。
fn optional_controller_executable_file_path(
    path: &Path,
    path_label: &str,
) -> Result<Option<PathBuf>, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(format!("{} is not a file: {}", path_label, path.display()));
            }
            Ok(Some(path.to_path_buf()))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to inspect {} {}: {}",
            path_label,
            path.display(),
            error
        )),
    }
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
        return is_numeric_port(port);
    }
    if is_numeric_port(trimmed) {
        return true;
    }

    let authority = endpoint_authority(trimmed);

    if let Some(port) = authority.strip_prefix("localhost:") {
        return is_numeric_port(port);
    }
    if let Some(port) = authority
        .strip_prefix("127.0.0.1:")
        .or_else(|| authority.strip_prefix("0.0.0.0:"))
    {
        return is_numeric_port(port);
    }
    if let Some(port) = authority.strip_prefix("[::1]:") {
        return is_numeric_port(port);
    }

    false
}

/// Extract the endpoint authority before any path, query, or fragment.
/// 提取路径、查询串或片段之前的 endpoint authority。
/// Parameters: `endpoint` is the already-trimmed controller endpoint text.
/// 参数：`endpoint` 是已经去除首尾空白的控制器端点文本。
/// Returns the authority portion used by local auto-spawn validation.
/// 返回用于本地自动拉起校验的 authority 部分。
fn endpoint_authority(endpoint: &str) -> &str {
    // Treat only the documented controller HTTP schemes as URL-like endpoints.
    // 仅将文档确认的控制器 HTTP scheme 视为 URL 形态端点。
    let authority = if let Some(rest) = endpoint.strip_prefix("http://") {
        rest
    } else if let Some(rest) = endpoint.strip_prefix("https://") {
        rest
    } else {
        return endpoint;
    };
    // Missing URL separators mean the whole URL tail is the authority.
    // 缺少 URL 分隔符时，URL 剩余部分整体就是 authority。
    let authority_end = match authority.find(['/', '?', '#']) {
        Some(index) => index,
        None => authority.len(),
    };
    authority[..authority_end].trim()
}

/// Return whether one port string is non-empty ASCII digits.
/// 判断端口字符串是否为非空 ASCII 数字。
/// Parameters: `port` is the candidate port fragment after endpoint parsing.
/// 参数：`port` 是端点解析后的候选端口片段。
/// Returns true only for a non-empty ASCII digit sequence.
/// 仅当输入为非空 ASCII 数字序列时返回 true。
fn is_numeric_port(port: &str) -> bool {
    !port.is_empty() && port.chars().all(|value| value.is_ascii_digit())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Optional environment text should treat an absent variable as no override.
    /// 可选环境文本应将缺失变量视为没有覆盖值。
    #[test]
    fn optional_env_text_from_result_returns_none_when_absent() {
        let value = optional_env_text_from_result(
            "VULCAN_GITHUB_BASE_URL",
            Err(std::env::VarError::NotPresent),
        )
        .expect("absent env var should not fail");

        assert_eq!(value, None);
    }

    /// Optional environment text should trim configured values and drop blank values.
    /// 可选环境文本应裁剪已配置值并丢弃空白值。
    #[test]
    fn optional_env_text_from_result_trims_and_drops_blank_values() {
        let blank = optional_env_text_from_result("VULCAN_GITHUB_BASE_URL", Ok("   ".to_string()))
            .expect("blank env var should not fail");
        let configured = optional_env_text_from_result(
            "VULCAN_GITHUB_BASE_URL",
            Ok("  https://github.example.test  ".to_string()),
        )
        .expect("configured env var should not fail");

        assert_eq!(blank, None);
        assert_eq!(configured.as_deref(), Some("https://github.example.test"));
    }

    /// Optional environment text should reject invalid Unicode instead of acting as if the variable was absent.
    /// 可选环境文本应拒绝无效 Unicode，而不是把它当作变量缺失。
    #[test]
    fn optional_env_text_from_result_rejects_invalid_unicode() {
        let error = optional_env_text_from_result(
            "VULCAN_GITHUB_BASE_URL",
            Err(std::env::VarError::NotUnicode(std::ffi::OsString::from(
                "bad-value",
            ))),
        )
        .expect_err("invalid Unicode env var should fail");

        assert!(error.contains("VULCAN_GITHUB_BASE_URL"));
        assert!(error.contains("not valid Unicode"));
    }

    /// Managed runtime policy should reject zero-valued host overrides before engine allocation.
    /// 受管运行时策略应在引擎分配前拒绝宿主配置的零值覆盖。
    #[test]
    fn managed_runtime_policy_rejects_zero_worker_capacity() {
        // Config isolates the invalid capacity while every other field uses upstream defaults.
        // Config 隔离无效容量，其余字段使用上游默认值。
        let mut config = Config::default();
        config
            .managed_runtime_config
            .worker_pool_max_size_per_environment = Some(0);

        let error = resolve_managed_runtime_config(&config)
            .expect_err("zero managed worker capacity should fail");

        assert!(
            error
                .to_string()
                .contains("worker_pool_max_size_per_environment"),
            "unexpected error: {error}"
        );
    }

    /// Explicit managed distribution roots must already exist as directories.
    /// 显式受管发行根必须已经以目录形态存在。
    #[test]
    fn managed_distribution_root_requires_existing_directory() {
        // ApplicationRoot provides the absolute anchor for the relative configured path.
        // ApplicationRoot 为相对配置路径提供绝对锚点。
        let application_root = std::env::temp_dir().join(format!(
            "vulcan-managed-distribution-root-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(application_root.join("lua_runtime"))
            .expect("application fixture should be created");
        // Config points at one intentionally missing distribution directory.
        // Config 指向一个故意缺失的发行目录。
        let config = Config {
            runtime_root: Some(application_root.to_string_lossy().to_string()),
            ..Config::default()
        };

        let error = resolve_managed_runtime_root_override(
            &config,
            Some("managed/distributions"),
            "managed_runtime_distribution_root",
            true,
        )
        .expect_err("missing explicit distribution root should fail");

        assert!(
            error
                .to_string()
                .contains("managed_runtime_distribution_root does not exist"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(&application_root).expect("application fixture should be removed");
    }

    /// A missing managed environment root remains valid because LuaSkills creates it lazily.
    /// 缺失的受管环境根仍然有效，因为 LuaSkills 会按需创建它。
    #[test]
    fn managed_environment_root_allows_lazy_creation() {
        // ApplicationRoot provides the stable anchor for one relative writable environment path.
        // ApplicationRoot 为相对可写环境路径提供稳定锚点。
        let application_root = std::env::temp_dir().join(format!(
            "vulcan-managed-environment-root-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(application_root.join("lua_runtime"))
            .expect("application fixture should be created");
        // Config binds the override to the explicit application root.
        // Config 把覆盖路径绑定到显式应用根。
        let config = Config {
            runtime_root: Some(application_root.to_string_lossy().to_string()),
            ..Config::default()
        };

        let resolved = resolve_managed_runtime_root_override(
            &config,
            Some("managed/environments"),
            "managed_runtime_environment_root",
            false,
        )
        .expect("missing writable environment root should remain valid")
        .expect("configured environment root should resolve");

        assert_eq!(resolved, application_root.join("managed/environments"));
        std::fs::remove_dir_all(&application_root).expect("application fixture should be removed");
    }
}
