use crate::config;
use crate::config::Config;
use crate::host_core::McpServer;
use crate::luaskills_adapter::{
    build_luaskills_cache_config, build_luaskills_engine_options, default_user_skill_root,
    normalize_skill_root_key, resolve_runtime_root_from_config, resolve_skill_config_file_path,
    resolve_skill_roots_from_config, validate_unique_skill_root_spaces,
};
use crate::support::temp_maintenance::initialize_runtime_temp_root;
use crate::support::tool_result_format::initialize_tool_result_template_roots;
use crate::transport::mcp::protocol::{ClientInfo, PROTOCOL_VERSION_LATEST, RequestContext};
use luaskills::{
    LuaEngine, LuaRuntimeHostOptions, LuaVmPoolConfig, RuntimeSkillRoot, SkillManager,
    SkillManagerConfig,
};
use serde_json::json;
/// Resolve the effective runtime root for one host execution path and surface explicit runtime-root misconfiguration as an immediate error.
/// 为单条宿主执行链解析生效运行根，并把显式 runtime_root 配置错误立即上抛。
pub(super) fn resolve_runtime_root_for_host(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    resolve_runtime_root_from_config(config)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
}

/// Resolve the explicit unified runtime skill-config file path used by host-owned luaskill-config operations.
/// 解析宿主自有 luaskill-config 操作使用的显式统一运行时 Skill 配置文件路径。
fn resolve_runtime_skill_config_file_path_for_host(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let Some(runtime_root) = resolve_runtime_root_for_host(config)? else {
        return Ok(None);
    };
    let file_path = resolve_skill_config_file_path(&runtime_root)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    Ok(Some(file_path))
}

/// Build one host-only MCP server surface and inject luaskill-config when the runtime root is available.
/// 构建一份仅含宿主工具面的 MCP 服务，并在运行根可用时注入 luaskill-config。
pub(super) fn build_host_tool_surface_server(
    config: &Config,
) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();
    if let Some(skill_config_file_path) = resolve_runtime_skill_config_file_path_for_host(config)? {
        server = server.with_runtime_skill_config_file_path(skill_config_file_path);
    }
    Ok(server)
}

/// Initialize the shared runtime temp root from config after runtime-root validation has completed.
/// 在完成运行根校验后，基于配置初始化共享运行时临时目录根。
pub(super) fn initialize_runtime_temp_root_from_config(
    config: &Config,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(config)?;
    initialize_runtime_temp_root(runtime_root.as_deref());
    Ok(runtime_root)
}

/// Runtime state required by local ROOT skill-management commands.
/// 本地 ROOT 技能管理命令所需的运行时状态。
pub(super) struct RootSkillCliContext {
    /// Single-VM LuaSkills engine used to execute lifecycle operations in-process.
    /// 用于在当前进程内执行生命周期操作的单 VM LuaSkills 引擎。
    pub(super) engine: LuaEngine,
    /// Fully resolved formal skill-root chain used for lifecycle preflight checks.
    /// 用于生命周期预检查的完整正式技能根链。
    pub(super) skill_roots: Vec<RuntimeSkillRoot>,
    /// Concrete ROOT target selected from the formal skill-root chain.
    /// 从正式技能根链中选出的具体 ROOT 目标。
    pub(super) target_root: RuntimeSkillRoot,
    /// Host options cloned from the engine configuration for record inspection.
    /// 从引擎配置中克隆出的宿主选项，用于检查安装记录。
    pub(super) host_options: LuaRuntimeHostOptions,
}

/// Build and initialize the MCP server, including external clients, Lua skills, and shared cache.
/// 构建并初始化 MCP Server，包括外部客户端、Lua Skills 与共享缓存。
pub(super) async fn build_server(cfg: &Config) -> Result<McpServer, Box<dyn std::error::Error>> {
    let mut server = build_host_tool_surface_server(cfg)?;

    // Connect the VMM gRPC client only when explicitly enabled.
    // 仅在显式启用时连接 VMM gRPC 客户端。
    if cfg.vmm_enable {
        let endpoint = cfg
            .vmm
            .as_deref()
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .ok_or("vmm_enable=true requires a non-empty vmm endpoint")?;
        server = server.with_vmm(endpoint).await?;
    }

    // Load Lua skills from system directory, with optional user override
    let runtime_root = resolve_runtime_root_for_host(cfg)?;
    let mut skill_roots = find_skill_roots(&cfg)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    let resources_root = runtime_root.as_ref().map(|root| root.join("resources"));
    initialize_tool_result_template_roots(
        &skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect::<Vec<_>>(),
        resources_root.as_deref(),
    )
    .map_err(|error| format!("Failed to initialize tool result template roots: {}", error))?;
    if !skill_roots.is_empty() {
        server = server.with_lua_skills(
            cfg,
            &skill_roots,
            LuaVmPoolConfig {
                min_size: cfg.lua_vm_pool_min_size.unwrap_or(1),
                max_size: cfg.lua_vm_pool_max_size.unwrap_or(4),
                idle_ttl_secs: cfg.lua_vm_pool_idle_ttl_secs.unwrap_or(300),
            },
            build_luaskills_cache_config(
                cfg.tool_cache_max_entries,
                cfg.tool_cache_default_ttl_secs,
                cfg.tool_cache_max_ttl_secs,
            ),
        )?;
    }

    Ok(server)
}

/// Ensure the formal runtime root chain contains the MCP-managed ROOT and USER layers when possible.
/// 在可行时确保正式运行根链包含 MCP 托管的 ROOT 与 USER 层。
pub(super) fn ensure_skill_manager_runtime_roots(
    runtime_root: Option<&std::path::Path>,
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_root_skill_manager_root(runtime_root, skill_roots)?;
    if !skill_roots.is_empty()
        && !skill_roots
            .iter()
            .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        return Err("ROOT skill root is required before loading LuaSkills".into());
    }
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        ensure_user_skill_manager_root(skill_roots)?;
        sort_skill_manager_formal_roots(skill_roots)
            .map_err(|error| format!("Failed to order skill-manager runtime roots: {}", error))?;
        validate_unique_skill_root_spaces(skill_roots).map_err(|error| {
            format!("Failed to validate skill-manager runtime roots: {}", error)
        })?;
    }
    Ok(())
}

/// Ensure the default USER skills directory is present as the ordinary mutable layer.
/// 确保默认 USER skills 目录作为普通可变层存在。
fn ensure_user_skill_manager_root(
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "USER")
    {
        return Ok(());
    }
    let Some(user_skills_dir) = default_user_skill_root() else {
        return Ok(());
    };
    std::fs::create_dir_all(&user_skills_dir).map_err(|error| {
        format!(
            "Failed to create USER skills directory {}: {}",
            user_skills_dir.display(),
            error
        )
    })?;
    let user_root_key = normalize_skill_root_key(&user_skills_dir);
    let mut candidate_roots = skill_roots.clone();
    candidate_roots.retain(|root| normalize_skill_root_key(&root.skills_dir) != user_root_key);
    candidate_roots.push(RuntimeSkillRoot {
        name: "USER".to_string(),
        skills_dir: user_skills_dir,
    });
    sort_skill_manager_formal_roots(&mut candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager USER skills directory: {}",
            error
        )
    })?;
    validate_unique_skill_root_spaces(&candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager USER skills directory: {}",
            error
        )
    })?;
    *skill_roots = candidate_roots;
    Ok(())
}

/// Ensure the host-managed ROOT skills directory is present when no explicit ROOT layer exists.
/// 当不存在显式 ROOT 层时，确保宿主管理的 ROOT skills 目录存在。
pub(super) fn ensure_root_skill_manager_root(
    runtime_root: Option<&std::path::Path>,
    skill_roots: &mut Vec<RuntimeSkillRoot>,
) -> Result<(), Box<dyn std::error::Error>> {
    if skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        sort_skill_manager_formal_roots(skill_roots).map_err(|error| {
            format!("Failed to order configured skill-manager roots: {}", error)
        })?;
        return Ok(());
    }
    let Some(runtime_root) = runtime_root else {
        return Ok(());
    };
    let root_skills_dir = runtime_root.join("skills");
    std::fs::create_dir_all(&root_skills_dir).map_err(|error| {
        format!(
            "Failed to create ROOT skills directory {}: {}",
            root_skills_dir.display(),
            error
        )
    })?;
    let managed_root_key = normalize_skill_root_key(&root_skills_dir);
    let mut candidate_roots = skill_roots.clone();
    candidate_roots.retain(|root| normalize_skill_root_key(&root.skills_dir) != managed_root_key);
    candidate_roots.push(RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: root_skills_dir,
    });
    sort_skill_manager_formal_roots(&mut candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager ROOT skills directory: {}",
            error
        )
    })?;
    validate_unique_skill_root_spaces(&candidate_roots).map_err(|error| {
        format!(
            "Failed to append skill-manager ROOT skills directory: {}",
            error
        )
    })?;
    *skill_roots = candidate_roots;
    Ok(())
}

/// Normalize one skill-manager layer label for local chain construction.
/// 为本地根链构造规范化单个 skill-manager 层级标签。
pub(super) fn normalize_skill_manager_layer_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

/// Return the fixed runtime priority rank for one skill-manager formal layer.
/// 返回单个 skill-manager 正式层级的固定运行时优先级。
fn skill_manager_layer_rank(name: &str) -> Result<usize, String> {
    match normalize_skill_manager_layer_name(name).as_str() {
        "ROOT" => Ok(0),
        "PROJECT" => Ok(1),
        "USER" => Ok(2),
        _ => Err(format!(
            "unsupported skill root label '{}'; expected ROOT, PROJECT, or USER",
            name.trim()
        )),
    }
}

/// Sort one root chain into ROOT -> PROJECT -> USER and normalize labels to uppercase.
/// 将根链排序为 ROOT -> PROJECT -> USER，并把标签规范化为大写。
pub(super) fn sort_skill_manager_formal_roots(
    skill_roots: &mut [RuntimeSkillRoot],
) -> Result<(), String> {
    for root in skill_roots.iter_mut() {
        root.name = normalize_skill_manager_layer_name(&root.name);
        skill_manager_layer_rank(&root.name)?;
    }
    skill_roots.sort_by_key(|root| skill_manager_layer_rank(&root.name).unwrap_or(usize::MAX));
    Ok(())
}

/// Build the single-VM LuaSkills context used by local ROOT lifecycle commands.
/// 构建本地 ROOT 生命周期命令使用的单 VM LuaSkills 上下文。
pub(super) fn build_root_skill_cli_context(
    config: &Config,
) -> Result<RootSkillCliContext, Box<dyn std::error::Error>> {
    // Resolve runtime root first so implicit ROOT/USER layers match normal service startup.
    // 先解析运行根，确保隐式 ROOT/USER 层与正常服务启动保持一致。
    let runtime_root = resolve_runtime_root_for_host(config)?;
    // Resolve and normalize the complete formal skill-root chain before selecting ROOT.
    // 在选择 ROOT 前解析并规范化完整正式技能根链。
    let mut skill_roots = find_skill_roots(config)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    // Select a concrete ROOT target and fail explicitly when none exists.
    // 选择具体 ROOT 目标，并在不存在时给出明确失败。
    let target_root = select_root_skill_manager_root(&skill_roots)?;
    // Build engine options once so the manager and engine share identical host paths.
    // 只构建一次引擎选项，确保管理器与引擎共享完全一致的宿主路径。
    let engine_options = build_luaskills_engine_options(
        config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )?;
    // Clone host options before moving the full options into LuaEngine.
    // 在完整选项移入 LuaEngine 前克隆宿主选项。
    let host_options = engine_options.host_options.clone();
    // Load existing skills so lifecycle preflight sees the same declared runtime state as service mode.
    // 加载现有技能，使生命周期预检查看到与服务模式一致的声明运行状态。
    let mut engine = LuaEngine::new(engine_options)?;
    engine.load_from_roots(&skill_roots)?;

    Ok(RootSkillCliContext {
        engine,
        skill_roots,
        target_root,
        host_options,
    })
}

/// Select the ROOT layer from an already normalized formal skill-root chain.
/// 从已经规范化的正式技能根链中选择 ROOT 层。
pub(super) fn select_root_skill_manager_root(
    roots: &[RuntimeSkillRoot],
) -> Result<RuntimeSkillRoot, Box<dyn std::error::Error>> {
    roots
        .iter()
        .find(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
        .cloned()
        .ok_or_else(|| "ROOT skill root is not configured for local skill management.".into())
}

/// Build one SkillManager for ROOT install-record inspection using the engine host options.
/// 使用引擎宿主选项构建一个用于检查 ROOT 安装记录的 SkillManager。
pub(super) fn build_root_skill_manager_for_cli(
    root: &RuntimeSkillRoot,
    host_options: &LuaRuntimeHostOptions,
) -> Result<SkillManager, Box<dyn std::error::Error>> {
    // Derive the lifecycle root exactly like the MCP skill-manager tool does.
    // 按照 MCP skill-manager 工具的方式推导生命周期根目录。
    let runtime_root = root
        .skills_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.skills_dir.clone());
    // Reuse the host download-cache fallback so update checks share cached archives.
    // 复用宿主下载缓存回退逻辑，让更新检查共享归档缓存。
    let download_cache_root = host_options.download_cache_root.clone().unwrap_or_else(|| {
        host_options
            .temp_dir
            .clone()
            .unwrap_or_else(|| runtime_root.join("temp"))
            .join("downloads")
    });
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root: root.clone(),
        lifecycle_root: runtime_root.join(host_options.state_dir_name.as_str()),
        download_cache_root,
        allow_network_download: host_options.allow_network_download,
        github_base_url: host_options.github_base_url.clone(),
        github_api_base_url: host_options.github_api_base_url.clone(),
    }))
}

/// Collect ROOT skill ids that are managed by install records and therefore updateable.
/// 收集由安装记录管理、因此可更新的 ROOT 技能标识。
pub(super) fn collect_managed_root_skill_ids(
    root: &RuntimeSkillRoot,
    manager: &SkillManager,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !root.skills_dir.exists() {
        return Ok(Vec::new());
    }
    // Keep output deterministic regardless of filesystem iteration order.
    // 无论文件系统迭代顺序如何，都保持输出稳定。
    let mut skill_ids = Vec::new();
    for entry in std::fs::read_dir(&root.skills_dir)? {
        // Read one directory entry from the ROOT skills directory.
        // 从 ROOT skills 目录读取单个目录项。
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        // Convert the directory name into the skill id used by LuaSkills records.
        // 将目录名转换为 LuaSkills 记录使用的技能标识。
        let Some(skill_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !entry.path().join("skill.yaml").exists() {
            continue;
        }
        // Only managed records carry an update source; unmanaged ROOT directories are intentionally skipped.
        // 只有受管记录携带更新来源；非受管 ROOT 目录会被有意跳过。
        let Some(record) = manager.install_record(&skill_id)? else {
            continue;
        };
        if record.managed {
            skill_ids.push(skill_id);
        }
    }
    skill_ids.sort();
    Ok(skill_ids)
}

/// Build a single-VM LuaEngine with fully loaded skills from the unified runtime root for local debug modes.
/// 在本地调试模式下基于统一运行根构建一个完整加载 skills 的单虚拟机 LuaEngine。
pub(super) fn build_single_vm_lua_engine_for_local_mode(
    config: &Config,
) -> Result<LuaEngine, Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(config)?;
    let mut skill_roots = find_skill_roots(config)?;
    ensure_skill_manager_runtime_roots(runtime_root.as_deref(), &mut skill_roots)?;
    if !skill_roots
        .iter()
        .any(|root| normalize_skill_manager_layer_name(&root.name) == "ROOT")
    {
        return Err("Lua skill directory not found for local debug mode".into());
    }
    let resources_root = runtime_root.as_ref().map(|root| root.join("resources"));
    initialize_tool_result_template_roots(
        &skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect::<Vec<_>>(),
        resources_root.as_deref(),
    )
    .map_err(|error| format!("Failed to initialize tool result template roots: {}", error))?;

    let mut engine = LuaEngine::new(build_luaskills_engine_options(
        config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )?)?;
    engine.load_from_roots(&skill_roots)?;
    Ok(engine)
}

/// Find the ordered skill-root chain for the default runtime environment.
/// 查找默认运行环境使用的有序技能根目录覆盖链。
fn find_skill_roots(
    cfg: &config::Config,
) -> Result<Vec<luaskills::RuntimeSkillRoot>, Box<dyn std::error::Error>> {
    resolve_skill_roots_from_config(cfg)
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
}

/// Build a simulated request context for `--call-tools` that stays close to a real MCP request.
/// 为 `--call-tools` 构造尽量贴近真实 MCP 请求的模拟上下文。
pub(super) fn build_call_tool_request_context(client_name: &str) -> RequestContext {
    RequestContext {
        transport: Some("call_tools".to_string()),
        session_id: Some("call-tools-local".to_string()),
        protocol_version: Some(PROTOCOL_VERSION_LATEST.to_string()),
        client_info: Some(ClientInfo {
            name: client_name.trim().to_string(),
            version: "local-debug".to_string(),
        }),
        client_match_name_override: None,
        exact_client_name: None,
        disable_client_match_overrides: false,
        client_capabilities: json!({}),
    }
}

/// Prepend runtime-root libs/ to PATH so C dependency DLLs (zlib1.dll, etc.) are discoverable when Lua C modules load via FFI.
/// 将运行根下的 libs/ 前置到 PATH，保证 Lua C 模块通过 FFI 加载时能找到依赖 DLL。
pub(super) fn add_libs_to_path(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let Some(runtime_root) = resolve_runtime_root_for_host(config)? else {
        return Ok(());
    };
    let libs_dir = runtime_root.join("libs");

    if !libs_dir.exists() {
        return Ok(());
    }
    if !libs_dir.is_dir() {
        return Err(format!(
            "runtime libs path is not a directory: {}",
            libs_dir.display()
        )
        .into());
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
    Ok(())
}
