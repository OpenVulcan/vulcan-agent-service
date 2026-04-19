use crate::client_budget::{ClientBudgetSnapshot, resolve_client_budget_snapshot};
use crate::config::Config;
use crate::protocol::{RequestContext, Tool, ToolAnnotations};
use crate::runtime_logging::{error as log_error, info as log_info, warn as log_warn};
use crate::temp_maintenance::ensure_runtime_temp_dir;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_luaskills::{
    LuaEngineOptions, LuaInvocationContext, LuaRuntimeHostOptions, LuaVmPoolConfig,
    RuntimeClientInfo, RuntimeEntryDescriptor, RuntimeLogCallback, RuntimeLogEvent,
    RuntimeLogLevel, RuntimeRequestContext, SkillProtectionConfig, ToolCacheConfig,
    DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS, DEFAULT_TOOL_CACHE_MAX_ENTRIES,
    DEFAULT_TOOL_CACHE_MAX_TTL_SECS, set_log_callback,
};

/// English: Install the host-side LuaSkills log callback so runtime events flow into the MCP host logger.
/// 宿主侧安装 LuaSkills 日志回调，让运行时事件统一流入 MCP 宿主日志器。
pub fn install_luaskills_log_callback() {
    let callback: RuntimeLogCallback = Arc::new(|event: &RuntimeLogEvent| match event.level {
        RuntimeLogLevel::Info => log_info(event.message.as_str()),
        RuntimeLogLevel::Warn => log_warn(event.message.as_str()),
        RuntimeLogLevel::Error => log_error(event.message.as_str()),
    });
    set_log_callback(Some(callback));
}

/// English: Convert one MCP request context into the generic runtime request context expected by the LuaSkills library.
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

/// English: Build one host-injected runtime invocation context from MCP request context, client budgets, and tool config.
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

/// English: Build one LuaSkills engine options object from the current MCP host runtime layout.
/// 基于当前 MCP 宿主运行目录布局构造一份 LuaSkills 引擎选项对象。
pub fn build_luaskills_engine_options(
    config: &Config,
    pool_config: LuaVmPoolConfig,
    cache_config: ToolCacheConfig,
) -> Result<LuaEngineOptions, Box<dyn std::error::Error>> {
    let temp_root = ensure_runtime_temp_dir()?.join("mcp");
    let tool_dependency_root = resolve_tool_dependency_root();
    let download_cache_root = Some(temp_root.join("__download_cache"));
    let lua_packages_dir = resolve_lua_packages_dir();
    let host_library_root = resolve_host_library_root();
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(temp_root.clone()),
        resources_dir: resolve_runtime_resources_dir(),
        lua_packages_dir: lua_packages_dir.clone(),
        luaexec_program: std::env::current_exe().ok(),
        tool_dependency_root,
        host_provided_tool_root: Some(temp_root.join("__host_tools")),
        lua_dependency_root: Some(temp_root.join("__lua_packages")),
        host_provided_lua_root: lua_packages_dir,
        ffi_dependency_root: Some(temp_root.join("__ffi")),
        host_provided_ffi_root: host_library_root,
        download_cache_root,
        skill_state_root: Some(temp_root.join("__skill_state")),
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
        sqlite_library_path: resolve_host_library_path(sqlite_library_file_name()),
        lancedb_library_path: resolve_host_library_path(lancedb_library_file_name()),
        sqlite_database_root: Some(temp_root.join("__database")),
        lancedb_database_root: Some(temp_root.join("__lancedb")),
        cache_config: Some(cache_config),
    };
    Ok(LuaEngineOptions::new(pool_config, host_options))
}

/// English: Resolve the shared tool dependency root used by LuaSkills-managed executable dependencies.
/// 解析供 LuaSkills 管理可执行工具依赖使用的共享根目录。
fn resolve_tool_dependency_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let runtime_path = exe_parent.join("lua_skills").join("__tools");
    if runtime_path.exists() {
        return Some(runtime_path);
    }

    let repository_path = std::env::current_dir()
        .ok()?
        .join(Path::new("runtime").join("lua_skills").join("__tools"));
    Some(repository_path)
}

/// English: Resolve the host-provided protected skill policy from environment and built-in defaults.
/// 从环境变量与内建默认值解析宿主提供的受保护技能策略。
fn resolve_skill_protection_config(config: &Config) -> SkillProtectionConfig {
    let mut protected_skill_ids = vec!["vulcan-runtime".to_string()];
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
    SkillProtectionConfig { protected_skill_ids }
}

/// English: Resolve the host-side cache policy that should be injected into the LuaSkills library.
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

/// English: Map one generic runtime entry descriptor into the MCP `Tool` object exposed to clients.
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

/// English: Convert the host-side client budget snapshot into the exact spill-render input still used by the MCP host.
/// 把宿主侧客户端预算快照转换为 MCP 宿主当前仍在使用的溢出渲染输入。
pub fn client_budget_snapshot_for_render(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    resolve_client_budget_snapshot(request_context, tool_name, skill_name)
}

/// English: Resolve the Lua resources directory according to the current MCP host layout.
/// 按当前 MCP 宿主布局解析 Lua 资源目录。
fn resolve_runtime_resources_dir() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let runtime_resources_dir = exe_parent.join("resources");
    if runtime_resources_dir.exists() {
        return Some(runtime_resources_dir);
    }

    let repository_resources_dir = std::env::current_dir()
        .ok()?
        .join(Path::new("output").join("resources"));
    if repository_resources_dir.exists() {
        return Some(repository_resources_dir);
    }

    None
}

/// English: Resolve the root directory that contains host-provided native libraries.
/// 解析宿主提供原生动态库所在的根目录。
fn resolve_host_library_root() -> Option<PathBuf> {
    if let Some(sqlite_path) = resolve_host_library_path(sqlite_library_file_name()) {
        if let Some(parent) = sqlite_path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    if let Some(lancedb_path) = resolve_host_library_path(lancedb_library_file_name()) {
        if let Some(parent) = lancedb_path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

/// English: Resolve the host-managed lua_packages directory according to runtime output first and repository output second.
/// 先按运行时输出目录、再按仓库输出目录解析宿主管理的 lua_packages 目录。
fn resolve_lua_packages_dir() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let runtime_path = exe_parent.join("lua_packages");
    if runtime_path.exists() {
        return Some(runtime_path);
    }

    let repository_path = std::env::current_dir()
        .ok()?
        .join(Path::new("output").join("lua_packages"));
    if repository_path.exists() {
        return Some(repository_path);
    }

    None
}

/// English: Resolve one host-side dynamic-library path with runtime output precedence and repository fallback.
/// 按运行时输出优先、仓库输出兜底的顺序解析一条宿主动态库路径。
fn resolve_host_library_path(file_name: &str) -> Option<PathBuf> {
    let explicit_env_key = if file_name.contains("sqlite") {
        "VLDB_SQLITE_LIBRARY"
    } else {
        "VLDB_LANCEDB_LIBRARY"
    };
    if let Ok(explicit) = std::env::var(explicit_env_key) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let runtime_path = exe_parent.join("libs").join(file_name);
    if runtime_path.exists() {
        return Some(runtime_path);
    }

    let repository_path = std::env::current_dir()
        .ok()?
        .join(Path::new("output").join("libs").join(file_name));
    if repository_path.exists() {
        return Some(repository_path);
    }

    None
}

/// English: Return the current platform-specific SQLite dynamic library filename.
/// 返回当前平台对应的 SQLite 动态库文件名。
fn sqlite_library_file_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "vldb_sqlite.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libvldb_sqlite.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libvldb_sqlite.dylib"
    }
}

/// English: Return the current platform-specific LanceDB dynamic library filename.
/// 返回当前平台对应的 LanceDB 动态库文件名。
fn lancedb_library_file_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "vldb_lancedb.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libvldb_lancedb.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libvldb_lancedb.dylib"
    }
}
