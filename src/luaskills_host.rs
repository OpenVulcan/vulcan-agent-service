use crate::client_budget::{ClientBudgetSnapshot, resolve_client_budget_snapshot};
use crate::config::{Config, SkillRootConfigEntry, SpaceControllerProcessModeConfig};
use crate::protocol::{RequestContext, Tool, ToolAnnotations};
use crate::runtime_logging::{error as log_error, info as log_info, warn as log_warn};
use crate::temp_maintenance::ensure_runtime_temp_dir;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use vulcan_luaskills::{
    DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS, DEFAULT_TOOL_CACHE_MAX_ENTRIES,
    DEFAULT_TOOL_CACHE_MAX_TTL_SECS, LuaEngineOptions, LuaInvocationContext,
    LuaRuntimeCapabilityOptions, LuaRuntimeDatabaseCallbackMode, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeHostOptions, LuaRuntimeSpaceControllerOptions, LuaRuntimeSpaceControllerProcessMode,
    LuaVmPoolConfig, RuntimeClientInfo, RuntimeEntryDescriptor, RuntimeLogCallback,
    RuntimeLogEvent, RuntimeLogLevel, RuntimeRequestContext, RuntimeSkillRoot,
    SkillProtectionConfig, ToolCacheConfig, set_log_callback,
};

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

/// Convert one MCP request context into the generic runtime request context expected by the LuaSkills library.
/// 把一份 MCP 请求上下文转换为 LuaSkills 库期望的通用运行时请求上下文。
pub fn build_runtime_request_context(request_context: &RequestContext) -> RuntimeRequestContext {
    RuntimeRequestContext {
        transport_name: request_context.transport.clone(),
        session_id: request_context.session_id.clone(),
        client_info: request_context
            .client_info
            .as_ref()
            .map(|client_info| RuntimeClientInfo {
                kind: Some("mcp".to_string()),
                name: Some(client_info.name.clone()),
                version: Some(client_info.version.clone()),
            }),
        client_capabilities: request_context.client_capabilities.clone(),
    }
}

/// Build one host-injected runtime invocation context from MCP request context, client budgets, and tool config.
/// 基于 MCP 请求上下文、客户端预算与工具配置构造一份宿主注入式运行时调用上下文。
pub fn build_runtime_invocation_context(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> LuaInvocationContext {
    let client_budget = resolve_client_budget_snapshot(request_context, tool_name, skill_name);
    let runtime_request_context = request_context.map(build_runtime_request_context);
    LuaInvocationContext::new(
        runtime_request_context,
        serde_json::to_value(&client_budget).unwrap_or_else(|_| json!({})),
        client_budget.tool_config.clone(),
    )
}

/// Build one LuaSkills engine options object from the current MCP host runtime layout.
/// 基于当前 MCP 宿主运行目录布局构造一份 LuaSkills 引擎选项对象。
pub fn build_luaskills_engine_options(
    config: &Config,
    pool_config: LuaVmPoolConfig,
    cache_config: ToolCacheConfig,
) -> Result<LuaEngineOptions, Box<dyn std::error::Error>> {
    let runtime_root =
        resolve_runtime_root_from_config(config).ok_or("Failed to resolve runtime root")?;
    let runtime_temp_root = ensure_runtime_temp_dir()?;
    let temp_root = runtime_temp_root.join("mcp");
    let download_cache_root = Some(runtime_temp_root.join("downloads"));
    let lua_packages_dir = resolve_lua_packages_dir(&runtime_root)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(temp_root.clone()),
        resources_dir: resolve_runtime_resources_dir(&runtime_root)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
        lua_packages_dir: lua_packages_dir.clone(),
        luaexec_program: std::env::current_exe().ok(),
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
        protection: resolve_skill_protection_config(config),
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
        reserved_entry_names: host_reserved_tool_names(),
        capabilities: LuaRuntimeCapabilityOptions {
            enable_skill_management_bridge: false,
        },
    };
    Ok(LuaEngineOptions::new(pool_config, host_options))
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
fn resolve_space_controller_options(
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
fn space_controller_executable_file_name() -> &'static str {
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

/// Resolve the stable base directory used for relative host configuration paths.
/// 解析宿主配置中相对路径应当依附的稳定基准目录。
fn resolve_config_base_dir(config: &Config) -> Option<PathBuf> {
    config
        .loaded_config_path
        .as_ref()
        .map(PathBuf::from)
        .and_then(|path| {
            let config_dir = path.parent()?.to_path_buf();
            let use_parent_of_configs = config_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("configs"))
                .unwrap_or(false);
            if use_parent_of_configs {
                config_dir.parent().map(std::path::Path::to_path_buf)
            } else {
                Some(config_dir)
            }
        })
}

/// Resolve the runtime root directory according to host configuration first and fallback layouts second.
/// 优先按宿主配置、其次按回退布局解析运行根目录。
pub fn resolve_runtime_root_from_config(config: &Config) -> Option<PathBuf> {
    if let Some(configured_root) = config
        .runtime_root
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let candidate_root = PathBuf::from(configured_root);
        let normalized_root = if candidate_root.is_absolute() {
            candidate_root
        } else if let Some(config_base_dir) = resolve_config_base_dir(config) {
            config_base_dir.join(candidate_root)
        } else {
            std::env::current_dir().ok()?.join(candidate_root)
        };
        if !normalized_root.exists() || !normalized_root.is_dir() {
            return None;
        }
        return Some(normalized_root);
    }

    let exe_path = std::env::current_exe().ok()?;
    let current_dir = std::env::current_dir().ok()?;
    resolve_implicit_runtime_root_from_paths(&current_dir, &exe_path)
}

/// Resolve one implicit runtime root from the current directory and executable path fallback chain.
/// 基于当前工作目录与可执行文件路径的回退链解析一份隐式运行根。
fn resolve_implicit_runtime_root_from_paths(
    current_dir: &std::path::Path,
    exe_path: &std::path::Path,
) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = exe_parent.to_path_buf();
    let hosted_skills_dir = hosted_root.join("skills");
    let hosted_configs_dir = hosted_root.join("configs");
    if (hosted_skills_dir.exists() && hosted_skills_dir.is_dir())
        || (hosted_configs_dir.exists() && hosted_configs_dir.is_dir())
    {
        return Some(hosted_root);
    }

    let repository_root = current_dir.join("runtime");
    if repository_root.exists() && repository_root.is_dir() {
        return Some(repository_root);
    }

    None
}

/// Resolve the ordered default skill roots from host configuration and runtime layout.
/// 从宿主配置与运行时布局解析默认环境使用的有序技能根目录列表。
pub fn resolve_skill_roots_from_config(config: &Config) -> Result<Vec<RuntimeSkillRoot>, String> {
    let mut ordered_roots = Vec::new();
    let mut seen_roots = HashSet::new();
    let mut seen_root_names = HashSet::new();
    let mut synthesized_index = 1usize;
    let config_base_dir = resolve_config_base_dir(config);

    let resolve_configured_path = |raw_path: &str| -> PathBuf {
        let candidate_path = PathBuf::from(raw_path);
        if candidate_path.is_absolute() {
            candidate_path
        } else if let Some(base_dir) = &config_base_dir {
            base_dir.join(candidate_path)
        } else {
            candidate_path
        }
    };

    let mut push_unique_root = |name: String, path: PathBuf| -> Result<(), String> {
        let normalized_name = name.trim().to_string();
        if !seen_root_names.insert(normalized_name.clone()) {
            return Err(format!(
                "duplicate skill root name '{}' is not allowed",
                normalized_name
            ));
        }
        let normalized_storage_path = normalize_skill_root_path(&path)?;
        let normalized_path = normalize_skill_root_key(&normalized_storage_path);
        if !seen_roots.insert(normalized_path) {
            return Err(format!(
                "duplicate skill root '{}' at {} is not allowed",
                name,
                normalized_storage_path.display()
            ));
        }
        ordered_roots.push(RuntimeSkillRoot {
            name: normalized_name,
            skills_dir: normalized_storage_path,
        });
        Ok(())
    };

    if let Some(configured_roots) = &config.skill_roots {
        for (index, value) in configured_roots.iter().enumerate() {
            match value {
                SkillRootConfigEntry::Named(named) => {
                    let name = named.name.trim();
                    let path = named.path.trim();
                    if name.is_empty() || path.is_empty() {
                        return Err(format!(
                            "skill_roots[{}] must declare non-empty name and path",
                            index
                        ));
                    }
                    push_unique_root(name.to_string(), resolve_configured_path(path))?;
                }
                SkillRootConfigEntry::Path(path) => {
                    let trimmed = path.trim();
                    if trimmed.is_empty() {
                        return Err(format!("skill_roots[{}] path must not be empty", index));
                    }
                    let generated = if synthesized_index == 1 {
                        "ROOT".to_string()
                    } else {
                        format!("ROOT-{}", synthesized_index)
                    };
                    synthesized_index += 1;
                    push_unique_root(generated, resolve_configured_path(trimmed))?;
                }
            }
        }
    } else if let Some(override_root) = config
        .skills_override
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(resolve_configured_path)
    {
        push_unique_root("USER".to_string(), override_root)?;
    } else if let Some(home) = home_dir() {
        push_unique_root(
            "USER".to_string(),
            home.join(".vulcan").join("vulcan-mcp").join("skills"),
        )?;
    }

    if let Some(runtime_root) = resolve_runtime_root_from_config(config) {
        if config.skill_roots.is_none() {
            push_unique_root("ROOT".to_string(), runtime_root.join("skills"))?;
        }
    }

    validate_unique_skill_root_spaces(&ordered_roots)?;
    if config.skill_roots.is_some() {
        for root in &ordered_roots {
            validate_skill_root_directory(root, true)?;
        }
        return Ok(ordered_roots);
    }
    let mut implicit_roots = Vec::new();
    for root in ordered_roots {
        if !root.skills_dir.exists() {
            continue;
        }
        validate_skill_root_directory(&root, false)?;
        implicit_roots.push(root);
    }
    Ok(implicit_roots)
}

/// Validate one skill root path according to strict or implicit runtime-root rules.
/// 按严格模式或隐式根规则校验单个技能根路径是否合法。
fn validate_skill_root_directory(
    root: &RuntimeSkillRoot,
    strict_missing: bool,
) -> Result<(), String> {
    if !root.skills_dir.exists() {
        if strict_missing {
            return Err(format!(
                "configured skill root '{}' does not exist: {}",
                root.name,
                root.skills_dir.display()
            ));
        }
        return Err(format!(
            "implicit skill root '{}' does not exist: {}",
            root.name,
            root.skills_dir.display()
        ));
    }

    if !root.skills_dir.is_dir() {
        return Err(format!(
            "skill root '{}' is not a directory: {}",
            root.name,
            root.skills_dir.display()
        ));
    }

    Ok(())
}

/// Normalize one skill-root path into a stable absolute path for runtime storage and validation.
/// 将单个技能根路径归一化为用于运行时存储与校验的稳定绝对路径。
pub fn normalize_skill_root_path(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                format!(
                    "failed to resolve current directory while normalizing skill root '{}': {}",
                    path.display(),
                    error
                )
            })?
            .join(path)
    };
    Ok(std::fs::canonicalize(&absolute_path).unwrap_or(absolute_path))
}

pub fn normalize_skill_root_key(path: &std::path::Path) -> String {
    let normalized_path = normalize_skill_root_path(path).unwrap_or_else(|_| path.to_path_buf());
    let rendered = normalized_path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        rendered.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        rendered
    }
}

/// Validate that every skill root maps to one unique sibling runtime space.
/// 校验每个技能根都映射到唯一的同级运行时空间。
pub fn validate_unique_skill_root_spaces(skill_roots: &[RuntimeSkillRoot]) -> Result<(), String> {
    let mut seen_space_parents = HashSet::new();
    let mut seen_root_names = HashSet::new();
    for root in skill_roots {
        let normalized_name = root.name.trim().to_string();
        if !seen_root_names.insert(normalized_name.clone()) {
            return Err(format!(
                "skill root name '{}' is duplicated in one runtime chain",
                normalized_name
            ));
        }
        let parent = root
            .skills_dir
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| root.skills_dir.clone());
        let normalized_parent = normalize_skill_root_key(&parent);
        if !seen_space_parents.insert(normalized_parent) {
            return Err(format!(
                "skill root '{}' at {} shares the same sibling runtime space with another root; each root must use a unique parent directory",
                root.name,
                root.skills_dir.display()
            ));
        }
    }
    Ok(())
}

/// Resolve the host-provided protected skill policy from environment and built-in defaults.
/// 从环境变量与内建默认值解析宿主提供的受保护技能策略。
fn resolve_skill_protection_config(config: &Config) -> SkillProtectionConfig {
    let mut protected_skill_ids = vec!["vulcan-lua".to_string()];
    if let Some(configured) = &config.protected_skills {
        for item in configured {
            let normalized = item.trim();
            if normalized.is_empty() {
                continue;
            }
            if !protected_skill_ids
                .iter()
                .any(|existing| existing == normalized)
            {
                protected_skill_ids.push(normalized.to_string());
            }
        }
    }
    if let Ok(extra) = std::env::var("VULCAN_PROTECTED_SKILLS") {
        for item in extra.split(',') {
            let normalized = item.trim();
            if normalized.is_empty() {
                continue;
            }
            if !protected_skill_ids
                .iter()
                .any(|existing| existing == normalized)
            {
                protected_skill_ids.push(normalized.to_string());
            }
        }
    }
    SkillProtectionConfig {
        protected_skill_ids,
    }
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

/// Map one generic runtime entry descriptor into the MCP `Tool` object exposed to clients.
/// 把一份通用运行时入口描述映射为对外暴露给 MCP 客户端的 `Tool` 对象。
pub fn map_runtime_entry_to_mcp_tool(entry: &RuntimeEntryDescriptor) -> Tool {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for parameter in &entry.parameters {
        props.insert(
            parameter.name.clone(),
            json!({
                "type": parameter.param_type,
                "description": parameter.description
            }),
        );
        if parameter.required {
            required.push(parameter.name.clone());
        }
    }

    Tool::with_annotations(
        &entry.canonical_name,
        &entry.description,
        Value::Object(props),
        required,
        ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        },
    )
}

/// Convert the host-side client budget snapshot into the exact spill-render input still used by the MCP host.
/// 把宿主侧客户端预算快照转换为 MCP 宿主当前仍在使用的溢出渲染输入。
pub fn client_budget_snapshot_for_render(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    resolve_client_budget_snapshot(request_context, tool_name, skill_name)
}

/// Resolve the Lua resources directory according to the current MCP host layout.
/// 按当前 MCP 宿主布局解析 Lua 资源目录。
fn resolve_runtime_resources_dir(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let runtime_resources_dir = runtime_root.join("resources");
    if runtime_resources_dir.exists() {
        if !runtime_resources_dir.is_dir() {
            return Err(format!(
                "runtime resources path is not a directory: {}",
                runtime_resources_dir.display()
            ));
        }
        return Ok(Some(runtime_resources_dir));
    }

    Ok(None)
}

/// Resolve the host-provided tool root and reject file-shaped runtime bin/tools paths early.
/// 解析宿主提供工具根目录，并在 runtime bin/tools 为文件形态时尽早拒绝。
fn resolve_host_provided_tool_root(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let tool_root = runtime_root.join("bin").join("tools");
    if tool_root.exists() && !tool_root.is_dir() {
        return Err(format!(
            "host-provided tool root is not a directory: {}",
            tool_root.display()
        ));
    }
    Ok(Some(tool_root))
}

/// Resolve the generic host-provided native library root used by Lua C modules and other runtime FFI payloads.
/// 解析 Lua C 模块及其他运行时 FFI 载荷使用的通用宿主原生库根目录。
fn resolve_host_ffi_root(runtime_root: &std::path::Path) -> Result<Option<PathBuf>, String> {
    let ffi_root = runtime_root.join("libs");
    if ffi_root.exists() {
        if !ffi_root.is_dir() {
            return Err(format!(
                "runtime ffi root is not a directory: {}",
                ffi_root.display()
            ));
        }
        return Ok(Some(ffi_root));
    }
    Ok(None)
}

/// Resolve the host-managed lua_packages directory according to runtime output first and repository output second.
/// 先按运行时输出目录、再按仓库输出目录解析宿主管理的 lua_packages 目录。
fn resolve_lua_packages_dir(runtime_root: &std::path::Path) -> Result<Option<PathBuf>, String> {
    let runtime_path = runtime_root.join("lua_packages");
    if runtime_path.exists() {
        if !runtime_path.is_dir() {
            return Err(format!(
                "runtime lua_packages path is not a directory: {}",
                runtime_path.display()
            ));
        }
        return Ok(Some(runtime_path));
    }
    Ok(None)
}

/// Resolve the current user's home directory when a default skill override root needs to be derived.
/// 在需要推导默认技能覆盖根目录时解析当前用户主目录。
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

/// Return the host-owned MCP tool names that must stay reserved from LuaSkills canonical entry generation.
/// 返回必须从 LuaSkills canonical 入口生成中保留的宿主 MCP 工具名称集合。
pub fn host_reserved_tool_names() -> Vec<String> {
    vec![
        "vulcan-help-list".to_string(),
        "vulcan-help-detail".to_string(),
        "reload_vulcan_mcp_configs".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, SpaceControllerConfig};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Return one shared mutex used to serialize environment-variable dependent tests.
    /// 返回一个共享互斥锁，用于串行化依赖环境变量的测试。
    fn environment_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Acquire the shared environment lock and recover from earlier poisoned test states.
    /// 获取共享环境锁，并从之前被污染的测试状态中恢复。
    fn acquire_environment_lock() -> MutexGuard<'static, ()> {
        environment_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Build one unique temporary directory path for one test case.
    /// 为单个测试用例构建唯一的临时目录路径。
    fn unique_test_dir(name: &str) -> PathBuf {
        let unique = format!(
            "vulcan-mcp-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    /// Build one minimal runtime entry descriptor used by MCP tool-mapping tests.
    /// 构造一份供 MCP 工具映射测试使用的最小运行时入口描述对象。
    fn sample_runtime_entry_descriptor() -> RuntimeEntryDescriptor {
        RuntimeEntryDescriptor {
            canonical_name: "demo-skill-search".to_string(),
            skill_id: "demo-skill".to_string(),
            local_name: "search".to_string(),
            root_name: "ROOT".to_string(),
            skill_dir: "D:/runtime/skills/demo-skill".to_string(),
            description: "Search demo content.".to_string(),
            parameters: vec![],
        }
    }

    /// Create one minimal runtime root layout used by host option resolution tests.
    /// 创建一份供宿主选项解析测试使用的最小运行时根目录布局。
    fn create_runtime_root_for_test(root: &PathBuf) {
        std::fs::create_dir_all(root.join("skills")).expect("failed to create skills directory");
        std::fs::create_dir_all(root.join("bin").join("tools"))
            .expect("failed to create tools directory");
    }

    /// Default config should keep both database backends on the controller-only path.
    /// 默认配置应保持两个数据库后端都走 controller-only 路径。
    #[test]
    fn default_config_keeps_controller_only_modes() {
        let root = unique_test_dir("dynamic-library-default");
        create_runtime_root_for_test(&root);
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let options =
            resolve_space_controller_options(&config, &root).expect("controller options failed");
        assert!(options.endpoint.is_none());
        assert!(options.executable_path.is_none());
        assert!(options.auto_spawn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Controller config should map endpoint, spawn policy, process mode, and copied executable path correctly.
    /// 控制器配置应正确映射端点、拉起策略、进程模式与复制后的可执行文件路径。
    #[test]
    fn controller_config_maps_to_space_controller_options() {
        let root = unique_test_dir("space-controller");
        create_runtime_root_for_test(&root);
        let copied_executable = root
            .join("bin")
            .join(space_controller_executable_file_name());
        std::fs::write(&copied_executable, b"test-controller")
            .expect("failed to create copied controller executable");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            space_controller: SpaceControllerConfig {
                endpoint: Some("http://127.0.0.1:29801".to_string()),
                auto_spawn: true,
                executable_path: None,
                process_mode: SpaceControllerProcessModeConfig::Service,
                minimum_uptime_secs: Some(600),
                idle_timeout_secs: Some(1800),
                default_lease_ttl_secs: Some(90),
                connect_timeout_secs: Some(3),
                startup_timeout_secs: Some(20),
                startup_retry_interval_ms: Some(100),
                lease_renew_interval_secs: Some(15),
            },
            ..Config::default()
        };
        let options =
            resolve_space_controller_options(&config, &root).expect("controller options failed");
        assert_eq!(options.endpoint.as_deref(), Some("http://127.0.0.1:29801"));
        assert_eq!(options.executable_path.as_ref(), Some(&copied_executable));
        assert_eq!(
            options.process_mode,
            LuaRuntimeSpaceControllerProcessMode::Service
        );
        assert_eq!(options.minimum_uptime_secs, 600);
        assert_eq!(options.idle_timeout_secs, 1800);
        assert_eq!(options.default_lease_ttl_secs, 90);
        assert_eq!(options.connect_timeout_secs, 3);
        assert_eq!(options.startup_timeout_secs, 20);
        assert_eq!(options.startup_retry_interval_ms, 100);
        assert_eq!(options.lease_renew_interval_secs, 15);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Building engine options with controller mode should preserve the copied executable path and provider selections.
    /// 使用控制器模式构建引擎选项时，应保留复制后的可执行文件路径和 provider 选择结果。
    #[test]
    fn build_engine_options_maps_space_controller_configuration() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("engine-options-controller");
        create_runtime_root_for_test(&root);
        let copied_executable = root
            .join("bin")
            .join(space_controller_executable_file_name());
        std::fs::write(&copied_executable, b"test-controller")
            .expect("failed to create copied controller executable");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            space_controller: SpaceControllerConfig {
                endpoint: Some("http://127.0.0.1:29801".to_string()),
                auto_spawn: true,
                executable_path: None,
                process_mode: SpaceControllerProcessModeConfig::Managed,
                ..SpaceControllerConfig::default()
            },
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let options = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect("failed to build luaskills engine options");
        assert_eq!(
            options.host_options.sqlite_provider_mode,
            LuaRuntimeDatabaseProviderMode::SpaceController
        );
        assert_eq!(
            options.host_options.lancedb_provider_mode,
            LuaRuntimeDatabaseProviderMode::SpaceController
        );
        assert_eq!(
            options.host_options.sqlite_callback_mode,
            LuaRuntimeDatabaseCallbackMode::Standard
        );
        assert_eq!(
            options.host_options.lancedb_callback_mode,
            LuaRuntimeDatabaseCallbackMode::Standard
        );
        assert_eq!(
            options.host_options.space_controller.endpoint.as_deref(),
            Some("http://127.0.0.1:29801")
        );
        assert_eq!(
            options
                .host_options
                .space_controller
                .executable_path
                .as_ref(),
            Some(&copied_executable)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Relative controller executable paths should be resolved against the runtime root instead of the current working directory.
    /// 相对控制器可执行文件路径应基于 runtime_root 解析，而不是依赖当前工作目录。
    #[test]
    fn controller_config_resolves_relative_executable_path_under_runtime_root() {
        let root = unique_test_dir("controller-relative-executable");
        create_runtime_root_for_test(&root);
        let relative_executable =
            PathBuf::from("bin").join(space_controller_executable_file_name());
        let copied_executable = root.join(&relative_executable);
        std::fs::write(&copied_executable, b"test-controller")
            .expect("failed to create relative controller executable");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            space_controller: SpaceControllerConfig {
                executable_path: Some(relative_executable.to_string_lossy().to_string()),
                ..SpaceControllerConfig::default()
            },
            ..Config::default()
        };
        let options =
            resolve_space_controller_options(&config, &root).expect("controller options failed");
        assert_eq!(options.executable_path.as_ref(), Some(&copied_executable));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Missing explicit controller executable paths should fail during host option construction instead of being deferred to runtime.
    /// 缺失的显式控制器可执行文件路径应在宿主选项构建阶段直接失败，而不是延迟到运行时。
    #[test]
    fn build_engine_options_rejects_missing_controller_executable_path() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("controller-missing-executable");
        create_runtime_root_for_test(&root);
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            space_controller: SpaceControllerConfig {
                executable_path: Some("bin/missing-vldb-controller.exe".to_string()),
                ..SpaceControllerConfig::default()
            },
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("missing executable path should fail");
        let rendered = error.to_string();
        assert!(
            rendered.contains("space_controller.executable_path does not exist"),
            "unexpected error: {rendered}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Fallback copied controller paths should also be validated as files, not merely as existing paths.
    /// 回退复制得到的控制器路径也应校验为文件，不能仅按存在性接受。
    #[test]
    fn build_engine_options_rejects_directory_shaped_fallback_controller_path() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("controller-directory-fallback");
        create_runtime_root_for_test(&root);
        let copied_executable_dir = root
            .join("bin")
            .join(space_controller_executable_file_name());
        std::fs::create_dir_all(&copied_executable_dir)
            .expect("failed to create directory-shaped fallback path");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("directory-shaped fallback path should fail");
        let rendered = error.to_string();
        assert!(
            rendered.contains("space_controller fallback executable path is not a file"),
            "unexpected error: {rendered}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Controller auto-spawn should reject remote endpoints during host option construction instead of deferring to runtime.
    /// 控制器自动拉起应在宿主选项构建阶段拒绝远端端点，而不是延迟到运行时。
    #[test]
    fn build_engine_options_rejects_remote_controller_endpoint_for_auto_spawn() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("controller-remote-endpoint");
        create_runtime_root_for_test(&root);
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            space_controller: SpaceControllerConfig {
                endpoint: Some("http://controller.internal:19801".to_string()),
                auto_spawn: true,
                ..SpaceControllerConfig::default()
            },
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("remote controller endpoint should fail during host validation");
        assert!(
            error
                .to_string()
                .contains("space_controller.auto_spawn=true requires one local bindable endpoint"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Relative runtime_root should be resolved against the loaded config file directory instead of the current working directory.
    /// 相对 runtime_root 应基于已加载配置文件所在目录解析，而不是依赖当前工作目录。
    #[test]
    fn resolve_runtime_root_uses_config_file_directory_for_relative_paths() {
        let base_dir = unique_test_dir("runtime-root-relative");
        let config_dir = base_dir.join("configs");
        let runtime_root = base_dir.join("runtime");
        std::fs::create_dir_all(&config_dir).expect("failed to create config directory");
        std::fs::create_dir_all(runtime_root.join("skills"))
            .expect("failed to create runtime skills directory");
        let config = Config {
            runtime_root: Some("runtime".to_string()),
            loaded_config_path: Some(config_dir.join("config.yaml").to_string_lossy().to_string()),
            ..Config::default()
        };
        let resolved =
            resolve_runtime_root_from_config(&config).expect("runtime root should resolve");
        assert_eq!(resolved, runtime_root);
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    /// Relative configured skill roots should be resolved from the stable config base directory instead of the current working directory.
    /// 相对技能根配置应基于稳定的配置基准目录解析，而不是依赖当前工作目录。
    #[test]
    fn resolve_skill_roots_uses_config_base_dir_for_relative_paths() {
        let base_dir = unique_test_dir("skill-root-relative");
        let config_dir = base_dir.join("configs");
        let skills_dir = base_dir.join("project-skills");
        std::fs::create_dir_all(&config_dir).expect("failed to create config directory");
        std::fs::create_dir_all(&skills_dir).expect("failed to create relative skills directory");
        let config = Config {
            skill_roots: Some(vec![SkillRootConfigEntry::Named(
                crate::config::NamedSkillRootConfig {
                    name: "PROJECT_A".to_string(),
                    path: "project-skills".to_string(),
                },
            )]),
            loaded_config_path: Some(config_dir.join("config.yaml").to_string_lossy().to_string()),
            ..Config::default()
        };
        let skill_roots =
            resolve_skill_roots_from_config(&config).expect("skill roots should resolve");
        assert_eq!(skill_roots.len(), 1);
        assert_eq!(
            normalize_skill_root_key(&skill_roots[0].skills_dir),
            normalize_skill_root_key(&skills_dir)
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    /// Invalid explicit runtime_root values should be rejected during resolution instead of flowing deeper into runtime assembly.
    /// 无效的显式 runtime_root 应在解析阶段被拒绝，而不是继续流入更深的运行时装配链路。
    #[test]
    fn resolve_runtime_root_rejects_missing_or_non_directory_paths() {
        let base_dir = unique_test_dir("runtime-root-invalid");
        let file_path = base_dir.join("runtime-file");
        std::fs::create_dir_all(&base_dir).expect("failed to create base directory");
        std::fs::write(&file_path, b"not-a-directory").expect("failed to create runtime file");

        let missing_config = Config {
            runtime_root: Some(
                base_dir
                    .join("missing-runtime")
                    .to_string_lossy()
                    .to_string(),
            ),
            ..Config::default()
        };
        assert!(
            resolve_runtime_root_from_config(&missing_config).is_none(),
            "missing runtime root should be rejected"
        );

        let file_config = Config {
            runtime_root: Some(file_path.to_string_lossy().to_string()),
            ..Config::default()
        };
        assert!(
            resolve_runtime_root_from_config(&file_config).is_none(),
            "file-shaped runtime root should be rejected"
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    /// File-shaped implicit repository runtime paths should not be accepted as valid fallback runtime roots.
    /// 文件形态的隐式仓库 runtime 路径不应被接受为合法的回退运行根。
    #[test]
    fn resolve_implicit_runtime_root_rejects_file_shaped_repository_runtime_path() {
        let _guard = acquire_environment_lock();
        let base_dir = unique_test_dir("implicit-runtime-file");
        let fake_exe = base_dir.join("bin").join("vulcan-mcp.exe");
        let runtime_file = base_dir.join("runtime");
        std::fs::create_dir_all(fake_exe.parent().expect("fake exe parent should exist"))
            .expect("failed to create fake exe parent");
        std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");
        std::fs::write(&runtime_file, b"not-a-directory").expect("failed to create runtime file");

        let resolved = resolve_implicit_runtime_root_from_paths(&base_dir, &fake_exe);
        assert!(
            resolved.is_none(),
            "file-shaped implicit runtime root should be rejected"
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    /// Runtime entry mapping should not expose IDE-only project-environment routing in the MCP product surface.
    /// 运行时入口映射不应在 MCP 产品面暴露仅供 IDE 使用的项目环境路由参数。
    #[test]
    fn map_runtime_entry_to_mcp_tool_omits_environment_id_parameter() {
        let tool = map_runtime_entry_to_mcp_tool(&sample_runtime_entry_descriptor());
        let schema = tool
            .input_schema
            .properties
            .as_ref()
            .and_then(|value| value.as_object())
            .expect("schema object");

        assert!(!schema.contains_key("environment_id"));
    }

    /// Reserved host tool names should not expose the IDE-only environment management bridge.
    /// 宿主保留工具名称集合不应再暴露仅供 IDE 使用的环境管理桥接工具。
    #[test]
    fn host_reserved_tool_names_omit_environment_management_tools() {
        let names = host_reserved_tool_names();

        assert!(
            !names
                .iter()
                .any(|name| name.starts_with("vulcan-environment-"))
        );
    }

    /// File-shaped runtime ffi roots should be rejected instead of being injected into the controller-only host options.
    /// 文件形态的运行时 ffi 根目录应被拒绝，不能注入 controller-only 宿主选项。
    #[test]
    fn build_engine_options_rejects_file_shaped_runtime_ffi_root() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("runtime-ffi-root-file");
        create_runtime_root_for_test(&root);
        let ffi_root = root.join("libs");
        std::fs::write(&ffi_root, b"not-a-directory")
            .expect("failed to create file-shaped ffi root path");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("file-shaped runtime ffi root should fail");
        assert!(
            error
                .to_string()
                .contains("runtime ffi root is not a directory"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// File-shaped runtime resources directories should be rejected during host option construction.
    /// 文件形态的运行时 resources 目录应在宿主选项构建阶段被拒绝。
    #[test]
    fn build_engine_options_rejects_file_shaped_runtime_resources_dir() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("runtime-resources-file");
        create_runtime_root_for_test(&root);
        let resources_file = root.join("resources");
        std::fs::write(&resources_file, b"not-a-directory")
            .expect("failed to create file-shaped resources path");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("file-shaped runtime resources dir should fail");
        assert!(
            error
                .to_string()
                .contains("runtime resources path is not a directory"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// File-shaped runtime lua_packages directories should be rejected during host option construction.
    /// 文件形态的运行时 lua_packages 目录应在宿主选项构建阶段被拒绝。
    #[test]
    fn build_engine_options_rejects_file_shaped_runtime_lua_packages_dir() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("runtime-lua-packages-file");
        create_runtime_root_for_test(&root);
        let lua_packages_file = root.join("lua_packages");
        std::fs::write(&lua_packages_file, b"not-a-directory")
            .expect("failed to create file-shaped lua_packages path");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("file-shaped runtime lua_packages dir should fail");
        assert!(
            error
                .to_string()
                .contains("runtime lua_packages path is not a directory"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// File-shaped host-provided tool roots should be rejected during host option construction.
    /// 文件形态的宿主工具根目录应在宿主选项构建阶段被拒绝。
    #[test]
    fn build_engine_options_rejects_file_shaped_host_provided_tool_root() {
        let _guard = acquire_environment_lock();
        let root = unique_test_dir("runtime-host-tools-file");
        create_runtime_root_for_test(&root);
        let tool_root = root.join("bin").join("tools");
        std::fs::remove_dir_all(&tool_root).expect("failed to clear tools directory");
        std::fs::write(&tool_root, b"not-a-directory")
            .expect("failed to create file-shaped tools path");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let pool_config = LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        };
        let cache_config = ToolCacheConfig::default();
        let error = build_luaskills_engine_options(&config, pool_config, cache_config)
            .expect_err("file-shaped host tool root should fail");
        assert!(
            error
                .to_string()
                .contains("host-provided tool root is not a directory"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
