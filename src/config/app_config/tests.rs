use super::paths::{
    normalize_cli_config_path, normalize_cli_runtime_root_arg, parse_cli_path_flag_from_args,
    reject_legacy_config_flag,
};
use super::{Config, RunLuaPoolConfigSection};

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
        "vulcan-agent-service.exe".to_string(),
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
        "vulcan-agent-service.exe".to_string(),
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
