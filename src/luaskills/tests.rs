use super::tool_mapping::{
    HOST_MANAGED_LUASKILL_SID_FIELD, HOST_MANAGED_LUASKILL_SID_PREFIX,
    LuaSkillToolProjectionOptions, inject_managed_luaskill_sid_argument,
    project_runtime_tool_descriptor, runtime_tool_uses_managed_luaskill_sid,
};
use super::*;
use crate::config::{
    Config, NamedSkillRootConfig, SpaceControllerConfig, SpaceControllerProcessModeConfig,
};
use crate::support::{RuntimeClientInfo, RuntimeRequestContext};
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::{
    LuaRuntimeDatabaseCallbackMode, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig, RuntimeEntryDescriptor,
    RuntimeSkillRoot, ToolCacheConfig,
};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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

/// Restore one environment variable after a test mutates process-wide state.
/// 在测试修改进程级状态后恢复一个环境变量。
struct EnvironmentVariableGuard {
    /// Environment variable name owned by this guard.
    /// 当前守卫负责的环境变量名称。
    name: &'static str,
    /// Original environment variable value, or absence.
    /// 环境变量原始值，或原始缺失状态。
    original: Option<OsString>,
}

impl EnvironmentVariableGuard {
    /// Capture one environment variable before a test changes it.
    /// 在测试修改环境变量前捕获其原始状态。
    /// Parameter `name` is the static environment variable name to restore.
    /// 参数：`name` 是需要恢复的静态环境变量名称。
    /// Returns a guard that restores the captured state on drop.
    /// 返回一个在销毁时恢复捕获状态的守卫。
    fn capture(name: &'static str) -> Self {
        Self {
            name,
            original: std::env::var_os(name),
        }
    }
}

impl Drop for EnvironmentVariableGuard {
    /// Restore the captured environment variable state.
    /// 恢复已捕获的环境变量状态。
    fn drop(&mut self) {
        unsafe {
            match self.original.as_ref() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }
}

/// Build one unique temporary directory path for one test case.
/// 为单个测试用例构建唯一的临时目录路径。
fn unique_test_dir(name: &str) -> PathBuf {
    let unique = format!(
        "vulcan-agent-service-{}-{}-{}",
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
        skill_dir: "D:/runtime/lua_runtime/skills/demo-skill".to_string(),
        description: "Search demo content.".to_string(),
        parameters: vec![],
        input_schema: serde_json::Value::Null,
    }
}

/// Create one minimal runtime root layout used by host option resolution tests.
/// 创建一份供宿主选项解析测试使用的最小运行时根目录布局。
/// Parameters: `root` is the test runtime root directory to initialize.
/// 参数：`root` 是需要初始化的测试运行时根目录。
/// Returns nothing after creating the required skills and tools directories.
/// 返回值：创建必需的 skills 与 tools 目录后不返回数据。
fn create_runtime_root_for_test(root: &Path) {
    // LuaRuntimeRoot mirrors the production `<application_root>/lua_runtime` package boundary.
    // LuaRuntimeRoot 镜像生产环境的 `<application_root>/lua_runtime` 包边界。
    let lua_runtime_root = root.join("lua_runtime");
    std::fs::create_dir_all(lua_runtime_root.join("skills"))
        .expect("failed to create skills directory");
    std::fs::create_dir_all(lua_runtime_root.join("bin"))
        .expect("failed to create runtime bin directory");
    std::fs::create_dir_all(lua_runtime_root.join("config"))
        .expect("failed to create runtime config directory");
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
    let options = resolve_space_controller_options(&config, &root.join("lua_runtime"))
        .expect("controller options failed");
    assert!(options.endpoint.is_none());
    assert!(options.executable_path.is_none());
    assert!(options.auto_spawn);
    let _ = std::fs::remove_dir_all(&root);
}

/// Ignored skill config should be forwarded into LuaSkills host options without enabling it by default.
/// 技能忽略配置应转发到 LuaSkills 宿主选项，同时默认不启用任何忽略项。
#[test]
fn build_engine_options_forwards_configured_ignored_skill_ids() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("ignored-skill-config");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ignored_skill_ids: Some(vec![
            "  custom-ai-memory  ".to_string(),
            "CUSTOM-AI-MEMORY".to_string(),
            "".to_string(),
        ]),
        ..Config::default()
    };

    let options = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect("failed to build luaskills engine options");

    assert_eq!(
        options.host_options.ignored_skill_ids,
        vec!["custom-ai-memory".to_string()]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// VMM configuration should ignore only the AI memory skill while keeping work memory on the SQLite skill path.
/// 配置 VMM 时应只忽略 AI 记忆技能，并保留工作记忆继续走 SQLite skill 路径。
#[test]
fn build_engine_options_ignores_ai_memory_only_when_vmm_is_configured() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("ignored-skill-vmm");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        vmm_enable: true,
        vmm: Some("http://127.0.0.1:50053".to_string()),
        ignored_skill_ids: Some(vec!["custom-skill".to_string()]),
        ..Config::default()
    };

    let options = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect("failed to build luaskills engine options");

    assert!(
        options
            .host_options
            .ignored_skill_ids
            .contains(&"custom-skill".to_string())
    );
    assert!(
        options
            .host_options
            .ignored_skill_ids
            .contains(&"vulcan-ai-memory".to_string())
    );
    assert!(
        !options
            .host_options
            .ignored_skill_ids
            .contains(&"vulcan-work-memory".to_string())
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A configured VMM endpoint should not skip AI memory unless VMM is explicitly enabled.
/// 仅配置 VMM 端点但未显式启用时，不应跳过 AI 记忆技能。
#[test]
fn build_engine_options_keeps_ai_memory_when_vmm_is_disabled() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("ignored-skill-vmm-disabled");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        vmm_enable: false,
        vmm: Some("http://127.0.0.1:50053".to_string()),
        ..Config::default()
    };

    let options = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect("failed to build luaskills engine options");

    assert!(
        !options
            .host_options
            .ignored_skill_ids
            .contains(&"vulcan-ai-memory".to_string())
    );
    assert!(
        !options
            .host_options
            .ignored_skill_ids
            .contains(&"vulcan-work-memory".to_string())
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Runtime request context should expose the forced environment override as the effective client name seen by Lua skills.
/// 运行时请求上下文应把强制环境变量覆盖值暴露为 Lua 技能看到的最终客户端名称。
#[test]
fn build_runtime_request_context_prefers_env_override_name() {
    let _guard = acquire_environment_lock();
    let previous = std::env::var(crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    unsafe {
        std::env::set_var(
            crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV,
            "qoder",
        );
    }

    let request_context = RuntimeRequestContext {
        client_info: Some(RuntimeClientInfo {
            name: "mcphost".to_string(),
            version: "1.0.0".to_string(),
        }),
        ..RuntimeRequestContext::default()
    };

    let runtime_context = build_runtime_request_context(&request_context);
    assert_eq!(
        runtime_context
            .client_info
            .as_ref()
            .and_then(|client_info| client_info.name.as_deref()),
        Some("qoder")
    );
    assert_eq!(
        runtime_context
            .client_info
            .as_ref()
            .and_then(|client_info| client_info.version.as_deref()),
        Some("1.0.0")
    );

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(
                crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV,
                value,
            );
        }
    } else {
        unsafe {
            std::env::remove_var(crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
}

/// Runtime request context should let request-scoped overrides win over both the original MCP client name and process-level env overrides.
/// 运行时请求上下文应让请求级覆盖值优先于原始 MCP 客户端名称和进程级环境变量覆盖。
#[test]
fn build_runtime_request_context_prefers_request_override_name() {
    let _guard = acquire_environment_lock();
    let previous = std::env::var(crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    unsafe {
        std::env::set_var(
            crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV,
            "mcphost",
        );
    }

    let request_context = RuntimeRequestContext {
        client_info: Some(RuntimeClientInfo {
            name: "copilot".to_string(),
            version: "2.0.0".to_string(),
        }),
        client_match_name_override: Some("workbuddy".to_string()),
        ..RuntimeRequestContext::default()
    };

    let runtime_context = build_runtime_request_context(&request_context);
    assert_eq!(
        runtime_context
            .client_info
            .as_ref()
            .and_then(|client_info| client_info.name.as_deref()),
        Some("workbuddy")
    );
    assert_eq!(
        runtime_context
            .client_info
            .as_ref()
            .and_then(|client_info| client_info.version.as_deref()),
        Some("2.0.0")
    );

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(
                crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV,
                value,
            );
        }
    } else {
        unsafe {
            std::env::remove_var(crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
}

/// Controller config should map endpoint, spawn policy, process mode, and copied executable path correctly.
/// 控制器配置应正确映射端点、拉起策略、进程模式与复制后的可执行文件路径。
#[test]
fn controller_config_maps_to_space_controller_options() {
    let root = unique_test_dir("space-controller");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
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
    let options = resolve_space_controller_options(&config, &root.join("lua_runtime"))
        .expect("controller options failed");
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
        .join("lua_runtime")
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
            .as_ref()
            .expect("controller executable should be configured")
            .canonicalize()
            .expect("controller option should resolve"),
        copied_executable
            .canonicalize()
            .expect("controller fixture should resolve")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Engine options should pin the fixed `system_lua_lib` directory from the LuaSkills 0.5.7 runtime root.
/// 引擎选项应从 LuaSkills 0.5.7 运行根固定 `system_lua_lib` 目录。
#[test]
fn build_engine_options_sets_fixed_system_lua_lib_dir() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("engine-options-system-lua-lib");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let options = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect("failed to build luaskills engine options");

    let system_lua_lib = options
        .host_options
        .system_lua_lib_dir
        .as_ref()
        .expect("system Lua directory should be configured");
    assert_eq!(
        system_lua_lib.file_name(),
        Some(std::ffi::OsStr::new("system_lua_lib"))
    );
    assert_eq!(
        system_lua_lib
            .parent()
            .expect("system Lua directory should have a runtime parent")
            .canonicalize()
            .expect("runtime parent should resolve"),
        root.join("lua_runtime")
            .canonicalize()
            .expect("runtime fixture should resolve")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// File-shaped `system_lua_lib` paths should fail fast instead of silently becoming one runtime-lease fallback target.
/// 文件形态的 `system_lua_lib` 路径应尽早失败，而不是静默成为运行时租约的回退目标。
#[test]
fn build_engine_options_rejects_file_shaped_system_lua_lib_dir() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("engine-options-system-lua-lib-file");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    std::fs::write(
        root.join("lua_runtime").join("system_lua_lib"),
        b"not-a-directory",
    )
    .expect("failed to create file-shaped system_lua_lib path");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let error = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect_err("file-shaped system_lua_lib should be rejected");

    assert!(
        error
            .to_string()
            .contains("LuaSkills runtime system_lua_lib path is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Missing runlua pool config should leave the dedicated isolated pool unset so LuaSkills can apply its own upstream defaults.
/// 缺失 runlua 池配置时应保持专用隔离池未显式设置，从而让 LuaSkills 采用其上游默认值。
#[test]
fn build_engine_options_leaves_runlua_pool_unset_when_config_is_absent() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("runlua-pool-defaults");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let pool_config = LuaVmPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 60,
    };

    let options = build_luaskills_engine_options(&config, pool_config, ToolCacheConfig::default())
        .expect("failed to build luaskills engine options");

    assert!(options.host_options.runlua_pool_config.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// Partial runlua pool config should be promoted into a full host override by filling missing fields with the upstream default values.
/// 局部 runlua 池配置应补齐为完整的宿主覆盖值，缺失字段使用上游默认值填充。
#[test]
fn build_engine_options_maps_runlua_pool_config_with_default_fill() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("runlua-pool-configured");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        runlua_pool_config: crate::config::RunLuaPoolConfigSection {
            min_size: Some(2),
            max_size: Some(6),
            idle_ttl_secs: None,
        },
        ..Config::default()
    };
    let pool_config = LuaVmPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 60,
    };

    let options = build_luaskills_engine_options(&config, pool_config, ToolCacheConfig::default())
        .expect("failed to build luaskills engine options");

    assert_eq!(
        options.host_options.runlua_pool_config,
        Some(LuaRuntimeRunLuaPoolConfig {
            min_size: 2,
            max_size: 6,
            idle_ttl_secs: 60,
        })
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Missing skill_config_root should use the current account's fixed user-level configuration directory.
/// 缺失 skill_config_root 时应使用当前账户固定的用户级配置目录。
#[test]
fn build_engine_options_defaults_skill_config_root_under_user_home() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("skill-config-default-path");
    create_runtime_root_for_test(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    let options = build_luaskills_engine_options(
        &config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 2,
            idle_ttl_secs: 60,
        },
        ToolCacheConfig::default(),
    )
    .expect("failed to build luaskills engine options");

    assert_eq!(
        options.host_options.skill_config_root.as_ref(),
        default_skill_config_root().as_ref()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// An explicit absolute skill_config_root should be passed through as the sole configuration root.
/// 显式绝对 skill_config_root 应作为唯一配置根传递。
#[test]
fn resolve_skill_config_root_uses_explicit_absolute_directory() {
    let root = unique_test_dir("skill-config-fixed-path");
    let configured_root = root.join("user-config");
    let config = Config {
        skill_config_root: Some(configured_root.to_string_lossy().to_string()),
        ..Config::default()
    };

    let resolved = resolve_skill_config_root_from_config(&config)
        .expect("explicit skill config root should resolve");
    assert_eq!(resolved, configured_root);

    let _ = std::fs::remove_dir_all(&root);
}

/// Relative skill_config_root values should be rejected instead of being anchored to runtime_root or cwd.
/// 相对 skill_config_root 应被拒绝，不能依附到 runtime_root 或当前工作目录。
#[test]
fn resolve_skill_config_root_rejects_relative_path() {
    let config = Config {
        skill_config_root: Some("relative/config".to_string()),
        ..Config::default()
    };

    let error = resolve_skill_config_root_from_config(&config)
        .expect_err("relative skill config root should fail");
    assert!(
        error.contains("must be an absolute directory path"),
        "unexpected error: {error}"
    );
}

/// An explicitly configured blank skill_config_root should fail instead of silently selecting the account default.
/// 显式配置为空白的 skill_config_root 应失败，不能静默选择账户默认目录。
#[test]
fn resolve_skill_config_root_rejects_blank_override() {
    let config = Config {
        skill_config_root: Some("   ".to_string()),
        ..Config::default()
    };

    let error = resolve_skill_config_root_from_config(&config)
        .expect_err("blank skill config root should fail");
    assert!(
        error.contains("must not be empty"),
        "unexpected error: {error}"
    );
}

/// File-shaped skill_config_root values should fail before LuaEngine construction.
/// 文件形态的 skill_config_root 应在 LuaEngine 构造前失败。
#[test]
fn resolve_skill_config_root_rejects_file_shaped_path() {
    let root = unique_test_dir("skill-config-file-shaped");
    std::fs::create_dir_all(&root).expect("fixture root should be created");
    let configured_root = root.join("config-file");
    std::fs::write(&configured_root, b"not-a-directory")
        .expect("file-shaped skill config root should be created");
    let config = Config {
        skill_config_root: Some(configured_root.to_string_lossy().to_string()),
        ..Config::default()
    };

    let error = resolve_skill_config_root_from_config(&config)
        .expect_err("file-shaped skill config root should fail");
    assert!(
        error.contains("skill config root is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Missing account-home state should fail explicitly when no skill_config_root override is provided.
/// 未提供 skill_config_root 覆盖值且账户主目录缺失时应明确失败。
#[test]
fn resolve_skill_config_root_rejects_missing_account_home() {
    let _guard = acquire_environment_lock();
    #[cfg(windows)]
    let home_variable = "USERPROFILE";
    #[cfg(not(windows))]
    let home_variable = "HOME";
    let _home_guard = EnvironmentVariableGuard::capture(home_variable);
    unsafe {
        std::env::remove_var(home_variable);
    }

    let error = resolve_skill_config_root_from_config(&Config::default())
        .expect_err("missing account home should fail without an explicit config root");

    assert!(
        error.contains("current account home directory is unavailable"),
        "unexpected error: {error}"
    );
}

/// Relative controller executable paths should be resolved against the runtime root instead of the current working directory.
/// 相对控制器可执行文件路径应基于 runtime_root 解析，而不是依赖当前工作目录。
#[test]
fn controller_config_resolves_relative_executable_path_under_runtime_root() {
    let root = unique_test_dir("controller-relative-executable");
    create_runtime_root_for_test(&root);
    let relative_executable = PathBuf::from("bin").join(space_controller_executable_file_name());
    let copied_executable = root.join("lua_runtime").join(&relative_executable);
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
    let options = resolve_space_controller_options(&config, &root.join("lua_runtime"))
        .expect("controller options failed");
    assert_eq!(options.executable_path.as_ref(), Some(&copied_executable));
    let _ = std::fs::remove_dir_all(&root);
}

/// Directory-shaped explicit controller executable paths should fail during host option construction.
/// 目录形态的显式控制器可执行文件路径应在宿主选项构建阶段失败。
#[test]
fn controller_config_rejects_directory_shaped_executable_path() {
    // The runtime root contains a directory where the explicit controller executable should be.
    // 运行根在显式控制器可执行文件位置放置一个目录。
    let root = unique_test_dir("controller-directory-executable");
    create_runtime_root_for_test(&root);
    // The relative executable path forces the resolver through runtime_root anchoring before inspection.
    // 相对可执行文件路径会迫使解析器先基于 runtime_root 锚定，再执行检查。
    let relative_executable = PathBuf::from("bin").join(space_controller_executable_file_name());
    let copied_executable = root.join("lua_runtime").join(&relative_executable);
    std::fs::create_dir_all(&copied_executable)
        .expect("failed to create directory-shaped controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        space_controller: SpaceControllerConfig {
            executable_path: Some(relative_executable.to_string_lossy().to_string()),
            ..SpaceControllerConfig::default()
        },
        ..Config::default()
    };

    // The configured executable path should report its own source label, not the fallback path label.
    // 显式配置的可执行文件路径应报告自身来源标签，而不是 fallback 路径标签。
    let error = resolve_space_controller_options(&config, &root.join("lua_runtime"))
        .expect_err("directory-shaped configured executable should fail");

    assert!(
        error.contains("space_controller.executable_path is not a file"),
        "unexpected error: {error}"
    );
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
        .join("lua_runtime")
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

/// Controller auto-spawn should accept local URL endpoints even when path, query, and fragment are present.
/// 控制器自动拉起应接受带路径、查询串和片段的本地 URL 端点。
#[test]
fn build_engine_options_accepts_local_controller_url_with_path_query_and_fragment() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("controller-local-url-parts");
    create_runtime_root_for_test(&root);
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        space_controller: SpaceControllerConfig {
            endpoint: Some("http://localhost:29801/api/leases?scope=test#ready".to_string()),
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

    build_luaskills_engine_options(&config, pool_config, cache_config)
        .expect("local URL controller endpoint should pass host validation");

    let _ = std::fs::remove_dir_all(&root);
}

/// Controller auto-spawn should reject remote URL authorities even when the path mentions localhost.
/// 控制器自动拉起应拒绝远端 URL authority，即使路径中出现 localhost。
#[test]
fn build_engine_options_rejects_remote_authority_with_localhost_path() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("controller-remote-authority-local-path");
    create_runtime_root_for_test(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        space_controller: SpaceControllerConfig {
            endpoint: Some("http://controller.internal/localhost:29801".to_string()),
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
        .expect_err("remote URL authority should fail during host validation");
    assert!(
        error
            .to_string()
            .contains("space_controller.auto_spawn=true requires one local bindable endpoint"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Controller auto-spawn should reject unsupported URL schemes instead of treating their authority as local.
/// 控制器自动拉起应拒绝不支持的 URL scheme，而不是把其中的 authority 当作本地地址。
#[test]
fn build_engine_options_rejects_unknown_scheme_for_auto_spawn() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("controller-unknown-scheme");
    create_runtime_root_for_test(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        space_controller: SpaceControllerConfig {
            endpoint: Some("grpc://localhost:29801".to_string()),
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
        .expect_err("unknown controller endpoint scheme should fail host validation");
    assert!(
        error
            .to_string()
            .contains("space_controller.auto_spawn=true requires one local bindable endpoint"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Controller auto-spawn should reject bare host endpoints that contain URL-only path syntax.
/// 控制器自动拉起应拒绝包含 URL 专属路径语法的裸 host 端点。
#[test]
fn build_engine_options_rejects_bare_localhost_path_for_auto_spawn() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("controller-bare-localhost-path");
    create_runtime_root_for_test(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        space_controller: SpaceControllerConfig {
            endpoint: Some("localhost:29801/api".to_string()),
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
        .expect_err("bare host endpoint with path syntax should fail host validation");
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
fn resolve_application_root_uses_config_file_directory_for_relative_paths() {
    let base_dir = unique_test_dir("runtime-root-relative");
    let config_dir = base_dir.join("configs");
    let runtime_root = base_dir.join("runtime");
    std::fs::create_dir_all(&config_dir).expect("failed to create config directory");
    std::fs::create_dir_all(runtime_root.join("lua_runtime"))
        .expect("failed to create LuaSkills runtime package directory");
    let config = Config {
        runtime_root: Some("runtime".to_string()),
        loaded_config_path: Some(config_dir.join("config.yaml").to_string_lossy().to_string()),
        ..Config::default()
    };
    let resolved = resolve_application_root_from_config(&config)
        .expect("runtime root lookup should succeed")
        .expect("runtime root should resolve");
    assert_eq!(
        resolved
            .canonicalize()
            .expect("resolved runtime root should exist"),
        runtime_root
            .canonicalize()
            .expect("runtime fixture should resolve")
    );
    std::fs::remove_dir_all(&base_dir).expect("test runtime root should be removed");
}

/// Relative configured skill roots should be resolved from the stable config base directory instead of the current working directory.
/// 相对技能根配置应基于稳定的配置基准目录解析，而不是依赖当前工作目录。
#[test]
fn resolve_skill_roots_uses_config_base_dir_for_relative_paths() {
    let base_dir = unique_test_dir("skill-root-relative");
    let config_dir = base_dir.join("configs");
    let skills_dir = base_dir.join("project-skills");
    std::fs::create_dir_all(&config_dir).expect("failed to create config directory");
    // The loaded configuration owns its runtime layout even when only USER is explicitly configured.
    // 即使只显式配置 USER，已加载配置所在应用也拥有自己的运行目录布局。
    std::fs::create_dir_all(base_dir.join("lua_runtime"))
        .expect("failed to create configuration-owned LuaSkills runtime");
    std::fs::create_dir_all(&skills_dir).expect("failed to create relative skills directory");
    let config = Config {
        skill_roots: Some(vec![NamedSkillRootConfig {
            name: "PROJECT".to_string(),
            path: "project-skills".to_string(),
        }]),
        loaded_config_path: Some(config_dir.join("config.yaml").to_string_lossy().to_string()),
        ..Config::default()
    };
    let skill_roots = resolve_skill_roots_from_config(&config).expect("skill roots should resolve");
    assert_eq!(skill_roots.len(), 1);
    assert_eq!(
        normalize_skill_root_key(&skill_roots[0].skills_dir),
        normalize_skill_root_key(&skills_dir)
    );
    let _ = std::fs::remove_dir_all(&base_dir);
}

/// Formal skill roots should sort into ROOT, PROJECT, then USER without fallback ranks.
/// 正式技能根应在不使用降级 rank 的情况下排序为 ROOT、PROJECT、USER。
#[test]
fn sort_formal_skill_roots_orders_known_layers() {
    // The input order is intentionally reversed so the test proves rank-based ordering.
    // 输入顺序有意反转，用于证明排序确实基于层级 rank。
    let mut skill_roots = vec![
        RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: PathBuf::from("D:/user/skills"),
        },
        RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: PathBuf::from("D:/runtime/skills"),
        },
        RuntimeSkillRoot {
            name: "PROJECT".to_string(),
            skills_dir: PathBuf::from("D:/project/skills"),
        },
    ];

    sort_formal_skill_roots(&mut skill_roots).expect("formal skill roots should sort");

    assert_eq!(
        skill_roots
            .iter()
            .map(|root| root.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ROOT", "PROJECT", "USER"]
    );
}

/// Formal skill-root sorting should leave input order untouched when a label is invalid.
/// 当标签无效时，正式技能根排序应保持输入顺序不变。
#[test]
fn sort_formal_skill_roots_preserves_input_on_invalid_layer() {
    // The invalid root is placed before ROOT so the old fallback-rank sort would have moved it.
    // 无效根位于 ROOT 之前，旧的降级 rank 排序会移动它。
    let mut skill_roots = vec![
        RuntimeSkillRoot {
            name: "BROKEN".to_string(),
            skills_dir: PathBuf::from("D:/broken/skills"),
        },
        RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: PathBuf::from("D:/runtime/skills"),
        },
    ];
    let original_roots = skill_roots.clone();

    let error = sort_formal_skill_roots(&mut skill_roots)
        .expect_err("invalid formal skill root should fail");

    assert!(
        error.contains("unsupported skill root name"),
        "unexpected error: {error}"
    );
    assert_eq!(skill_roots, original_roots);
}

/// Fallible skill-root key normalization should still allow missing paths so validation can report domain-specific errors later.
/// 可失败的技能根键规范化仍应允许缺失路径，以便后续校验报告领域专用错误。
#[test]
fn try_normalize_skill_root_key_preserves_missing_path_for_deferred_validation() {
    let missing_path = unique_test_dir("missing-skill-root-key").join("skills");
    let key = try_normalize_skill_root_key(&missing_path)
        .expect("missing skill root key should normalize for later validation");
    assert!(
        key.ends_with("skills"),
        "missing skill root key should preserve path text: {key}"
    );
}

/// Invalid explicit runtime_root values should return explicit resolution errors instead of silently collapsing into implicit fallback discovery.
/// 无效的显式 runtime_root 应返回明确的解析错误，而不是静默塌缩成隐式回退发现。
#[test]
fn resolve_application_root_rejects_missing_or_non_directory_paths() {
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
    let missing_error = resolve_application_root_from_config(&missing_config)
        .expect_err("missing runtime root should fail");
    assert!(
        missing_error.contains("configured application runtime_root does not exist"),
        "unexpected error: {missing_error}"
    );

    let file_config = Config {
        runtime_root: Some(file_path.to_string_lossy().to_string()),
        ..Config::default()
    };
    let file_error = resolve_application_root_from_config(&file_config)
        .expect_err("file runtime root should fail");
    assert!(
        file_error.contains("configured application runtime_root is not a directory"),
        "unexpected error: {file_error}"
    );
    let _ = std::fs::remove_dir_all(&base_dir);
}

/// File-shaped implicit repository runtime paths should not be accepted as valid fallback runtime roots.
/// 文件形态的隐式仓库 runtime 路径不应被接受为合法的回退运行根。
#[test]
fn resolve_implicit_application_root_rejects_file_shaped_repository_runtime_path() {
    let _guard = acquire_environment_lock();
    let base_dir = unique_test_dir("implicit-runtime-file");
    let fake_exe = base_dir.join("bin").join("vulcan-agent-service.exe");
    let runtime_file = base_dir.join("output");
    std::fs::create_dir_all(fake_exe.parent().expect("fake exe parent should exist"))
        .expect("failed to create fake exe parent");
    std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");
    std::fs::write(&runtime_file, b"not-a-directory").expect("failed to create runtime file");

    let error = resolve_implicit_application_root_from_paths(&base_dir, &fake_exe)
        .expect_err("file-shaped implicit runtime root should fail");
    assert!(
        error.contains("implicit repository application root is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&base_dir);
}

/// File-shaped hosted runtime markers should fail instead of silently falling through to another layout.
/// 文件形态的托管运行根标记应失败，而不是静默落入其它布局。
#[test]
fn resolve_implicit_application_root_rejects_file_shaped_hosted_lua_runtime_marker() {
    let _guard = acquire_environment_lock();
    let runtime_root = unique_test_dir("implicit-runtime-hosted-file-marker");
    let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
    std::fs::create_dir_all(fake_exe.parent().expect("fake exe parent should exist"))
        .expect("failed to create fake exe parent");
    std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");
    std::fs::write(runtime_root.join("lua_runtime"), b"not-a-directory")
        .expect("failed to create file-shaped hosted LuaSkills runtime marker");

    let error = resolve_implicit_application_root_from_paths(&std::env::temp_dir(), &fake_exe)
        .expect_err("file-shaped hosted LuaSkills runtime marker should fail");
    assert!(
        error.contains("hosted LuaSkills runtime path is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&runtime_root);
}

/// Hosted executable layouts should resolve the parent runtime root when it contains a skills directory.
/// 托管式可执行文件布局在父级运行根包含 skills 目录时应解析该运行根。
#[test]
fn resolve_implicit_application_root_accepts_hosted_parent_layout() {
    let _guard = acquire_environment_lock();
    let runtime_root = unique_test_dir("implicit-runtime-hosted");
    let fake_exe = runtime_root.join("bin").join("vulcan-agent-service.exe");
    std::fs::create_dir_all(runtime_root.join("lua_runtime").join("skills"))
        .expect("failed to create hosted LuaSkills directory");
    std::fs::create_dir_all(fake_exe.parent().expect("fake exe parent should exist"))
        .expect("failed to create fake exe parent");
    std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");

    let resolved = resolve_implicit_application_root_from_paths(&std::env::temp_dir(), &fake_exe)
        .expect("hosted runtime root inspection should succeed")
        .expect("hosted runtime root should resolve");

    assert_eq!(
        normalize_skill_root_key(&resolved),
        normalize_skill_root_key(&runtime_root)
    );
    let _ = std::fs::remove_dir_all(&runtime_root);
}

/// Relative executable paths without a stable absolute grandparent should not establish a hosted runtime root.
/// 没有稳定绝对祖父目录的相对可执行文件路径不应建立宿主运行根。
#[test]
fn resolve_implicit_application_root_rejects_relative_executable_without_hosted_parent() {
    // Build an isolated repository path without runtime markers; the explicit inputs fully determine discovery.
    // 构建一个不含运行时标记的隔离仓库路径；发现结果完全由显式输入决定。
    let base_dir = unique_test_dir("implicit-runtime-relative-exe");
    let repository_cwd = base_dir.join("repo");
    std::fs::create_dir_all(&repository_cwd).expect("repository cwd should be created");

    let resolved = resolve_implicit_application_root_from_paths(
        &repository_cwd,
        Path::new("vulcan-agent-service.exe"),
    )
    .expect("relative executable inspection should succeed");

    assert!(
        resolved.is_none(),
        "relative executable without hosted parent should not resolve a runtime root"
    );
    std::fs::remove_dir_all(&base_dir).expect("test runtime root should be removed");
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

/// MCP tool mapping should preserve the normalized tool and parameter descriptions exported by LuaSkills 0.5.7.
/// MCP 工具映射应保留 LuaSkills 0.5.7 导出的规范化工具说明与参数说明文本。
#[test]
fn map_runtime_entry_to_mcp_tool_preserves_luaskills_normalized_descriptions() {
    let entry = RuntimeEntryDescriptor {
        description: "Normalized tool summary.".to_string(),
        parameters: vec![luaskills::RuntimeEntryParameterDescriptor {
            name: "query".to_string(),
            description: "Legacy query description.".to_string(),
            param_type: "string".to_string(),
            required: true,
        }],
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Normalized query description."
                }
            },
            "required": ["query"]
        }),
        ..sample_runtime_entry_descriptor()
    };
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    let schema = tool
        .input_schema
        .properties
        .as_ref()
        .and_then(|value| value.as_object())
        .expect("schema object");
    let query_description = schema
        .get("query")
        .and_then(|value| value.get("description"))
        .and_then(|value| value.as_str());

    assert_eq!(
        tool.description.as_deref(),
        Some("Normalized tool summary.")
    );
    assert_eq!(query_description, Some("Normalized query description."));
}

/// Session-capable hosts should receive LuaSkills schemas without the managed LUASKILL_SID parameter.
/// 支持会话托管的宿主应收到移除了托管 LUASKILL_SID 参数的 LuaSkills schema。
#[test]
fn project_runtime_tool_descriptor_hides_managed_luaskill_sid() {
    let entry = RuntimeEntryDescriptor {
        parameters: vec![
            luaskills::RuntimeEntryParameterDescriptor {
                name: HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
                description: "Managed session identity.".to_string(),
                param_type: "string".to_string(),
                required: true,
            },
            luaskills::RuntimeEntryParameterDescriptor {
                name: "task_name".to_string(),
                description: "Target task.".to_string(),
                param_type: "string".to_string(),
                required: true,
            },
        ],
        ..sample_runtime_entry_descriptor()
    };
    let raw = map_runtime_entry_to_mcp_tool(&entry);
    let projected = project_runtime_tool_descriptor(
        &raw,
        &LuaSkillToolProjectionOptions {
            hide_managed_luaskill_sid: true,
        },
    );
    let schema = projected
        .input_schema
        .properties
        .as_ref()
        .and_then(|value| value.as_object())
        .expect("schema object");

    assert!(runtime_tool_uses_managed_luaskill_sid(&raw));
    assert!(!schema.contains_key(HOST_MANAGED_LUASKILL_SID_FIELD));
    assert_eq!(
        projected.input_schema.required,
        Some(vec!["task_name".to_string()])
    );
    assert!(
        projected
            .description
            .as_deref()
            .is_some_and(|value| !value.contains(HOST_MANAGED_LUASKILL_SID_FIELD))
    );
}

/// Managed LuaSkills calls should inject the trusted session identity instead of trusting model-supplied sid values.
/// 托管 LuaSkills 调用应注入受信任会话身份，而不是信任模型提供的 sid 值。
#[test]
fn inject_managed_luaskill_sid_argument_overrides_untrusted_sid() {
    let entry = RuntimeEntryDescriptor {
        parameters: vec![luaskills::RuntimeEntryParameterDescriptor {
            name: HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
            description: "Managed session identity.".to_string(),
            param_type: "string".to_string(),
            required: true,
        }],
        ..sample_runtime_entry_descriptor()
    };
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    let injected = inject_managed_luaskill_sid_argument(
        &tool,
        serde_json::json!({
            HOST_MANAGED_LUASKILL_SID_FIELD: "spoofed",
            "task_name": "demo"
        }),
        "opencode",
        Some("session-123"),
    )
    .expect("managed sid injection should succeed");
    let mut hasher = Sha256::new();
    hasher.update(b"opencode");
    hasher.update(b"\0");
    hasher.update(b"session-123");
    let digest_hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let expected_managed_sid = format!("{HOST_MANAGED_LUASKILL_SID_PREFIX}{}", &digest_hex[..48]);

    assert_eq!(
        injected
            .get(HOST_MANAGED_LUASKILL_SID_FIELD)
            .and_then(|value| value.as_str()),
        Some(expected_managed_sid.as_str())
    );
}

/// Different host namespaces should derive different managed sid values even when the raw session id matches.
/// 即使原始 session id 相同，不同宿主命名空间也应派生出不同的托管 sid。
#[test]
fn inject_managed_luaskill_sid_argument_namespaces_host_identity() {
    let entry = RuntimeEntryDescriptor {
        parameters: vec![luaskills::RuntimeEntryParameterDescriptor {
            name: HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
            description: "Managed session identity.".to_string(),
            param_type: "string".to_string(),
            required: true,
        }],
        ..sample_runtime_entry_descriptor()
    };
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    let opencode_sid = inject_managed_luaskill_sid_argument(
        &tool,
        serde_json::json!({}),
        "opencode",
        Some("shared-session"),
    )
    .expect("opencode managed sid injection should succeed");
    let openclaw_sid = inject_managed_luaskill_sid_argument(
        &tool,
        serde_json::json!({}),
        "openclaw",
        Some("shared-session"),
    )
    .expect("openclaw managed sid injection should succeed");

    assert_ne!(
        opencode_sid
            .get(HOST_MANAGED_LUASKILL_SID_FIELD)
            .and_then(|value| value.as_str()),
        openclaw_sid
            .get(HOST_MANAGED_LUASKILL_SID_FIELD)
            .and_then(|value| value.as_str())
    );
}

/// Pre-prefixed host-managed sid inputs should still be normalized by MCP instead of passing through unchanged.
/// 即使输入已带宿主管理前缀，MCP 也应重新规范化，而不是原样透传。
#[test]
fn inject_managed_luaskill_sid_argument_rewrites_prefixed_sid_inputs() {
    let entry = RuntimeEntryDescriptor {
        parameters: vec![luaskills::RuntimeEntryParameterDescriptor {
            name: HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
            description: "Managed session identity.".to_string(),
            param_type: "string".to_string(),
            required: true,
        }],
        ..sample_runtime_entry_descriptor()
    };
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    let raw_prefixed_sid = format!("{HOST_MANAGED_LUASKILL_SID_PREFIX}caller-provided-value");
    let injected = inject_managed_luaskill_sid_argument(
        &tool,
        serde_json::json!({}),
        "opencode",
        Some(raw_prefixed_sid.as_str()),
    )
    .expect("prefixed sid input should still be normalized");
    let mut hasher = Sha256::new();
    hasher.update(b"opencode");
    hasher.update(b"\0");
    hasher.update(raw_prefixed_sid.as_bytes());
    let digest_hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let expected_managed_sid = format!("{HOST_MANAGED_LUASKILL_SID_PREFIX}{}", &digest_hex[..48]);

    assert_eq!(
        injected
            .get(HOST_MANAGED_LUASKILL_SID_FIELD)
            .and_then(|value| value.as_str()),
        Some(expected_managed_sid.as_str())
    );
    assert_ne!(
        injected
            .get(HOST_MANAGED_LUASKILL_SID_FIELD)
            .and_then(|value| value.as_str()),
        Some(raw_prefixed_sid.as_str())
    );
}

/// Managed LuaSkills calls should fail clearly when the host promised sid injection but omitted the actual session id.
/// 当宿主承诺会注入 sid 却遗漏实际 session id 时，托管 LuaSkills 调用应明确失败。
#[test]
fn inject_managed_luaskill_sid_argument_requires_session_id() {
    let entry = RuntimeEntryDescriptor {
        parameters: vec![luaskills::RuntimeEntryParameterDescriptor {
            name: HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
            description: "Managed session identity.".to_string(),
            param_type: "string".to_string(),
            required: true,
        }],
        ..sample_runtime_entry_descriptor()
    };
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    let error =
        inject_managed_luaskill_sid_argument(&tool, serde_json::json!({}), "opencode", None)
            .expect_err("managed sid injection should require a session id");

    assert_eq!(error.0, -32602);
    assert!(error.1.contains("projection.session_id"));
}

/// Reserved host tool names should not expose the IDE-only environment management bridge.
/// 宿主保留工具名称集合不应再暴露仅供 IDE 使用的环境管理桥接工具。
#[test]
fn host_reserved_tool_names_omit_environment_management_tools() {
    let names = host_reserved_tool_names();

    assert!(names.iter().any(|name| name == "runtime-config"));
    assert!(names.iter().any(|name| name == "skill-manager"));
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
    let ffi_root = root.join("lua_runtime").join("libs");
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
            .contains("LuaSkills runtime libs path is not a directory"),
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
    let resources_file = root.join("lua_runtime").join("resources");
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
            .contains("LuaSkills runtime resources path is not a directory"),
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
    let lua_packages_file = root.join("lua_runtime").join("lua_packages");
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
            .contains("LuaSkills runtime lua_packages path is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Windows verbatim runtime roots should not leak into Lua package path templates.
/// Windows verbatim 运行根不应泄漏到 Lua 包路径模板中。
#[cfg(windows)]
#[test]
fn build_engine_options_strips_windows_verbatim_prefix_from_lua_package_paths() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("runtime-lua-packages-verbatim");
    create_runtime_root_for_test(&root);
    std::fs::create_dir_all(root.join("lua_runtime").join("lua_packages"))
        .expect("failed to create lua_packages directory");
    let copied_executable = root
        .join("lua_runtime")
        .join("bin")
        .join(space_controller_executable_file_name());
    std::fs::write(&copied_executable, b"test-controller")
        .expect("failed to create copied controller executable");
    // Verbatim runtime root mirrors Windows canonicalize output and proves Lua never sees a literal `?` prefix.
    // Verbatim 运行根模拟 Windows canonicalize 输出，并验证 Lua 不会看到包含字面量 `?` 的前缀。
    let verbatim_root = PathBuf::from(format!(r"\\?\{}", root.to_string_lossy()));
    let config = Config {
        runtime_root: Some(verbatim_root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let pool_config = LuaVmPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 60,
    };
    let cache_config = ToolCacheConfig::default();
    let options = build_luaskills_engine_options(&config, pool_config, cache_config)
        .expect("verbatim runtime root should build engine options");
    // Expected Lua package root keeps the normal drive-letter spelling so Lua's `?` placeholder remains unambiguous.
    // 期望的 Lua 包根目录保留普通盘符写法，避免 Lua 的 `?` 占位符产生歧义。
    let expected_lua_packages_dir = root.join("lua_runtime").join("lua_packages");
    let lua_packages_dir = options
        .host_options
        .lua_packages_dir
        .as_ref()
        .expect("Lua package directory should be configured");
    let host_lua_root = options
        .host_options
        .host_provided_lua_root
        .as_ref()
        .expect("host Lua root should be configured");
    for configured in [lua_packages_dir, host_lua_root] {
        assert!(!configured.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(
            configured
                .canonicalize()
                .expect("configured Lua root should resolve"),
            expected_lua_packages_dir
                .canonicalize()
                .expect("Lua package fixture should resolve")
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// File-shaped host-provided tool roots should be rejected during host option construction.
/// 文件形态的宿主工具根目录应在宿主选项构建阶段被拒绝。
#[test]
fn build_engine_options_rejects_file_shaped_host_provided_tool_root() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("runtime-host-tools-file");
    create_runtime_root_for_test(&root);
    let tool_root = root.join("lua_runtime").join("bin");
    std::fs::remove_dir_all(&tool_root).expect("failed to clear runtime bin directory");
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
            .contains("LuaSkills runtime bin path is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
