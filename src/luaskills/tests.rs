use super::tool_mapping::{
    HOST_MANAGED_LUASKILL_SID_FIELD, HOST_MANAGED_LUASKILL_SID_PREFIX,
    LuaSkillToolProjectionOptions, inject_managed_luaskill_sid_argument,
    project_runtime_tool_descriptor, runtime_tool_uses_managed_luaskill_sid,
};
use super::*;
use crate::config::{
    Config, SkillRootConfigEntry, SpaceControllerConfig, SpaceControllerProcessModeConfig,
};
use crate::support::{RuntimeClientInfo, RuntimeRequestContext};
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::{
    LuaRuntimeDatabaseCallbackMode, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig, RuntimeEntryDescriptor, ToolCacheConfig,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
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

/// Ignored skill config should be forwarded into LuaSkills host options without enabling it by default.
/// 技能忽略配置应转发到 LuaSkills 宿主选项，同时默认不启用任何忽略项。
#[test]
fn build_engine_options_forwards_configured_ignored_skill_ids() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("ignored-skill-config");
    create_runtime_root_for_test(&root);
    let copied_executable = root
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

/// Missing runlua pool config should leave the dedicated isolated pool unset so LuaSkills can apply its own upstream defaults.
/// 缺失 runlua 池配置时应保持专用隔离池未显式设置，从而让 LuaSkills 采用其上游默认值。
#[test]
fn build_engine_options_leaves_runlua_pool_unset_when_config_is_absent() {
    let _guard = acquire_environment_lock();
    let root = unique_test_dir("runlua-pool-defaults");
    create_runtime_root_for_test(&root);
    let copied_executable = root
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

/// Missing skill config path should default to the runtime `configs/skill_config.json` file so MCP and LuaSkills share one stable location.
/// 缺失 Skill 配置路径时应默认回退到运行根下的 `configs/skill_config.json`，让 MCP 与 LuaSkills 共享同一稳定位置。
#[test]
fn build_engine_options_defaults_skill_config_path_under_runtime_configs() {
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
        options.host_options.skill_config_file_path.as_ref(),
        Some(&root.join("configs").join("skill_config.json"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Runtime skill config path resolution should always stay under the runtime-root `configs/` directory.
/// 运行时 Skill 配置路径解析应始终固定在运行根的 `configs/` 目录下。
#[test]
fn resolve_skill_config_file_path_uses_runtime_root_configs_directory() {
    let root = unique_test_dir("skill-config-fixed-path");
    create_runtime_root_for_test(&root);

    let resolved =
        resolve_skill_config_file_path(&root).expect("runtime skill config path should resolve");
    assert_eq!(resolved, root.join("configs").join("skill_config.json"));

    let _ = std::fs::remove_dir_all(&root);
}

/// Relative controller executable paths should be resolved against the runtime root instead of the current working directory.
/// 相对控制器可执行文件路径应基于 runtime_root 解析，而不是依赖当前工作目录。
#[test]
fn controller_config_resolves_relative_executable_path_under_runtime_root() {
    let root = unique_test_dir("controller-relative-executable");
    create_runtime_root_for_test(&root);
    let relative_executable = PathBuf::from("bin").join(space_controller_executable_file_name());
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
    let resolved = resolve_runtime_root_from_config(&config)
        .expect("runtime root lookup should succeed")
        .expect("runtime root should resolve");
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
                name: "PROJECT".to_string(),
                path: "project-skills".to_string(),
            },
        )]),
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

/// Invalid explicit runtime_root values should return explicit resolution errors instead of silently collapsing into implicit fallback discovery.
/// 无效的显式 runtime_root 应返回明确的解析错误，而不是静默塌缩成隐式回退发现。
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
    let missing_error = resolve_runtime_root_from_config(&missing_config)
        .expect_err("missing runtime root should fail");
    assert!(
        missing_error.contains("configured runtime_root does not exist"),
        "unexpected error: {missing_error}"
    );

    let file_config = Config {
        runtime_root: Some(file_path.to_string_lossy().to_string()),
        ..Config::default()
    };
    let file_error =
        resolve_runtime_root_from_config(&file_config).expect_err("file runtime root should fail");
    assert!(
        file_error.contains("configured runtime_root is not a directory"),
        "unexpected error: {file_error}"
    );
    let _ = std::fs::remove_dir_all(&base_dir);
}

/// File-shaped implicit repository runtime paths should not be accepted as valid fallback runtime roots.
/// 文件形态的隐式仓库 runtime 路径不应被接受为合法的回退运行根。
#[test]
fn resolve_implicit_runtime_root_rejects_file_shaped_repository_runtime_path() {
    let _guard = acquire_environment_lock();
    let base_dir = unique_test_dir("implicit-runtime-file");
    let fake_exe = base_dir.join("bin").join("vulcan-agent-service.exe");
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

    assert!(names.iter().any(|name| name == "luaskill-config"));
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
