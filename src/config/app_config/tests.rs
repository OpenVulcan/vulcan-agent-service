use super::paths::{
    find_exe_parent_config_from_exe_path, find_runtime_root_config, normalize_cli_config_path,
    normalize_cli_runtime_root_arg, parse_cli_path_flag_from_args,
};
use super::{Config, ManagedRuntimeConfigSection, RunLuaPoolConfigSection};

/// Repository application-config template should satisfy the current strict contract.
/// 仓库应用配置模板应满足当前严格契约。
#[test]
fn application_config_template_parses() {
    let yaml = include_str!("../../../configs/config.yaml");
    let parsed: Config = serde_yaml::from_str(yaml).expect("config.yaml should parse");

    assert_eq!(
        parsed.format_version,
        super::super::HOST_CONFIG_FORMAT_VERSION
    );
}

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
    let normalized =
        normalize_cli_config_path("configs/config.yaml").expect("config path should normalize");
    assert_eq!(normalized, cwd.join("configs").join("config.yaml"));
}

/// Runtime-root config discovery should return the concrete config path when the file exists.
/// 当配置文件存在时，运行根配置发现应返回具体配置路径。
#[test]
fn find_runtime_root_config_returns_existing_config_path() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-runtime-root-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let config_path = root.join("configs").join("config.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("failed to create config directory");
    std::fs::write(&config_path, "format_version: 1\nstdio: true\n")
        .expect("failed to write config file");

    let discovered = find_runtime_root_config(&root.to_string_lossy())
        .expect("runtime-root config discovery should succeed")
        .expect("config path should exist");

    assert_eq!(discovered, config_path.to_string_lossy().as_ref());
    let _ = std::fs::remove_dir_all(&root);
}

/// Runtime-root config discovery should treat a missing config file as absence, not as an error.
/// 当配置文件缺失时，运行根配置发现应表示缺失，而不是报错。
#[test]
fn find_runtime_root_config_returns_none_for_missing_config_file() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-missing-runtime-root-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&root).expect("failed to create runtime root directory");

    let discovered = find_runtime_root_config(&root.to_string_lossy())
        .expect("runtime-root config discovery should succeed");

    assert!(discovered.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// Runtime-root config discovery should reject a directory where config.yaml must be a file.
/// 运行根配置发现应拒绝 config.yaml 位置出现目录。
#[test]
fn find_runtime_root_config_rejects_directory_shaped_config_path() {
    // The test runtime root contains a directory at the required config file path.
    // 测试运行根在必需配置文件路径上放置一个目录。
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-directory-runtime-root-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    // The directory-shaped config path exercises discovery, not YAML parsing.
    // 目录形态配置路径用于覆盖发现阶段，而不是 YAML 解析阶段。
    let config_path = root.join("configs").join("config.yaml");
    std::fs::create_dir_all(&config_path).expect("directory-shaped config path should be created");

    // The discovery error should identify the damaged runtime-root config path.
    // 发现错误应标识损坏的运行根配置路径。
    let error = find_runtime_root_config(&root.to_string_lossy())
        .expect_err("directory-shaped runtime-root config should fail");

    assert!(
        error
            .to_string()
            .contains("runtime-root config path is not a file"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Executable-parent config discovery should treat a missing config file as absence.
/// 可执行文件父目录配置发现应把缺失配置文件视为缺席。
#[test]
fn find_exe_parent_config_from_exe_path_returns_none_for_missing_config_file() {
    // The fake executable path models <runtime_root>/bin/<exe> without a configs/config.yaml file.
    // 伪造可执行路径模拟缺少 configs/config.yaml 的 <runtime_root>/bin/<exe> 布局。
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-missing-exe-parent-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    // The bin directory gives the discovery helper a concrete executable parent chain.
    // bin 目录为发现 helper 提供具体的可执行文件父级链路。
    let exe_path = root.join("bin").join("vulcan-agent-service.exe");
    std::fs::create_dir_all(exe_path.parent().expect("exe parent should exist"))
        .expect("fake executable parent should be created");

    // Missing configs/config.yaml remains the only absence case in this discovery helper.
    // 缺失 configs/config.yaml 仍是该发现 helper 中唯一的缺席语义。
    let discovered = find_exe_parent_config_from_exe_path(&exe_path)
        .expect("executable-parent config discovery should succeed");

    assert!(discovered.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// Executable-parent config discovery should reject a directory where config.yaml must be a file.
/// 可执行文件父目录配置发现应拒绝 config.yaml 位置出现目录。
#[test]
fn find_exe_parent_config_from_exe_path_rejects_directory_shaped_config_path() {
    // The fake output layout contains bin/<exe> and a directory-shaped configs/config.yaml.
    // 伪造输出布局包含 bin/<exe> 以及目录形态的 configs/config.yaml。
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-directory-exe-parent-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    // The helper derives the runtime root from the executable parent and then inspects configs/config.yaml.
    // helper 会从可执行文件父级推导运行根，然后检查 configs/config.yaml。
    let exe_path = root.join("bin").join("vulcan-agent-service.exe");
    std::fs::create_dir_all(exe_path.parent().expect("exe parent should exist"))
        .expect("fake executable parent should be created");
    let config_path = root.join("configs").join("config.yaml");
    std::fs::create_dir_all(&config_path).expect("directory-shaped config path should be created");

    // The discovery error should identify the executable-parent source rather than falling through as missing.
    // 发现错误应标识可执行文件父目录来源，而不是降级为缺失。
    let error = find_exe_parent_config_from_exe_path(&exe_path)
        .expect_err("directory-shaped executable-parent config should fail");

    assert!(
        error
            .to_string()
            .contains("executable-parent config path is not a file"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Application config should require the current explicit format version.
/// 应用配置应要求当前显式格式版本。
#[test]
fn config_rejects_missing_format_version() {
    let error = serde_yaml::from_str::<Config>("http: \"127.0.0.1:19201\"\n")
        .expect_err("missing format version should fail");
    assert!(error.to_string().contains("format_version"));
}

/// Application config should reject fields outside the current strict schema.
/// 应用配置应拒绝当前严格结构以外的字段。
#[test]
fn config_rejects_unknown_fields() {
    let error = serde_yaml::from_str::<Config>("format_version: 1\nunsupported_setting: true\n")
        .expect_err("unknown fields should fail");
    assert!(error.to_string().contains("unsupported_setting"));
}

/// Application config should reject path-only skill roots instead of assigning implicit layer names.
/// 应用配置应拒绝纯路径技能根，而不是为其分配隐式层级名称。
#[test]
fn config_rejects_path_only_skill_roots() {
    let error =
        serde_yaml::from_str::<Config>("format_version: 1\nskill_roots:\n  - \"D:/skills/root\"\n")
            .expect_err("path-only skill roots should fail");
    assert!(error.to_string().contains("skill_roots"));
}

/// Application config loading should reject every format version other than the current contract.
/// 应用配置加载应拒绝当前契约版本以外的所有格式版本。
#[test]
fn config_rejects_unsupported_format_version() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-agent-service-app-config-version-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let config_path = root.join("config.yaml");
    std::fs::create_dir_all(&root).expect("config test directory should be created");
    std::fs::write(&config_path, "format_version: 2\n")
        .expect("versioned config fixture should be written");

    let error = Config::from_file(&config_path.to_string_lossy())
        .expect_err("unsupported version should fail");

    assert!(
        error
            .to_string()
            .contains("Unsupported config format_version 2")
    );
    let _ = std::fs::remove_dir_all(root);
}

/// CLI runtime-root flags should fail early when the next argv token is another flag instead of a path.
/// 当 CLI runtime-root 标志后面直接跟着另一个标志时，应尽早失败。
#[test]
fn parse_cli_path_flag_rejects_missing_runtime_root_value() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
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
        "vulcan-agent-service.exe".to_string(),
        "--runtime-root=output".to_string(),
    ];
    let runtime_root = parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])
        .expect("inline runtime-root should parse");
    assert_eq!(runtime_root, Some("output".to_string()));
}

/// Inline `--runtime-root=` forms should still fail early when the value is empty.
/// 内联 `--runtime-root=` 在取值为空时也应尽早失败。
#[test]
fn parse_cli_path_flag_rejects_empty_inline_runtime_root_value() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--runtime-root=".to_string(),
    ];
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
format_version: 1
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

/// Config YAML should deserialize the explicit user-level LuaSkills package configuration root.
/// 配置 YAML 应能反序列化显式的用户级 LuaSkills 技能包配置根。
#[test]
fn config_deserializes_skill_config_root() {
    let config: Config = serde_yaml::from_str(
        r#"
format_version: 1
skill_config_root: "D:/service-data/vulcan/config"
"#,
    )
    .expect("skill config root should deserialize");

    assert_eq!(
        config.skill_config_root.as_deref(),
        Some("D:/service-data/vulcan/config")
    );
}

/// Managed runtime roots and policy fields should deserialize through their canonical fields.
/// 受管运行时根与策略字段应通过规范字段正常反序列化。
#[test]
fn config_deserializes_managed_runtime_block() {
    // ConfigText exercises both root placement and every positive resource-policy field.
    // ConfigText 同时覆盖根目录位置与全部正数资源策略字段。
    let config: Config = serde_yaml::from_str(
        r#"
format_version: 1
managed_runtime_distribution_root: "lua_runtime/dependencies/runtimes"
managed_runtime_environment_root: "lua_runtime/dependencies/envs"
managed_runtime_config:
  worker_pool_max_size_per_environment: 8
  worker_idle_ttl_secs: 120
  persistent_session_limit_per_engine: 128
  persistent_session_default_buffer_limit_bytes_per_stream: 2097152
  invoke_default_timeout_ms: 30000
"#,
    )
    .expect("managed runtime config should deserialize");

    assert_eq!(
        config.managed_runtime_distribution_root.as_deref(),
        Some("lua_runtime/dependencies/runtimes")
    );
    assert_eq!(
        config.managed_runtime_environment_root.as_deref(),
        Some("lua_runtime/dependencies/envs")
    );
    assert_eq!(
        config.managed_runtime_config,
        ManagedRuntimeConfigSection {
            worker_pool_max_size_per_environment: Some(8),
            worker_idle_ttl_secs: Some(120),
            persistent_session_limit_per_engine: Some(128),
            persistent_session_default_buffer_limit_bytes_per_stream: Some(2_097_152),
            invoke_default_timeout_ms: Some(30_000),
        }
    );
}
