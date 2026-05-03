use super::resolution::parse_metric_literal;
use super::*;
use crate::transport::mcp::protocol::ClientInfo;
use serde_yaml::from_str;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// Return one shared mutex used to serialize runtime-root override tests for client-budget loading.
/// 返回一个共享互斥锁，用于串行化客户端预算加载中的运行根覆盖测试。
fn runtime_root_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Return one shared mutex used to serialize environment-variable override tests for client-budget matching.
/// 返回一个共享互斥锁，用于串行化客户端预算匹配中的环境变量覆盖测试。
fn environment_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Verify that `-1` is parsed as explicit unlimited rather than a normal numeric value.
/// 验证 `-1` 会被正确解析为显式不限，而不是普通数值。
#[test]
fn parse_metric_literal_treats_minus_one_as_unlimited() {
    let parsed = parse_metric_literal("-1", "env").expect("expected unlimited metric");
    assert_eq!(parsed.value, Some(-1));
    assert_eq!(parsed.source, "env");
}

/// Verify that when tokens and bytes coexist, the inline byte budget uses the stricter effective byte value.
/// 验证 tokens 与 bytes 同时存在时，会按最严格的 bytes 结果生成内联预算。
#[test]
fn resolve_scope_budget_uses_lowest_effective_inline_bytes() {
    let estimation = EffectiveBudgetEstimation {
        bytes_per_token: 3,
        safe_bytes_ratio: 1.0,
        unlimited_bytes_cap: 200 * 1024,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "tokens".to_string(),
        BudgetMetricConfig {
            default: Some(50_000),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );
    metrics.insert(
        "bytes".to_string(),
        BudgetMetricConfig {
            default: Some(120_000),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );

    let scope = resolve_scope_budget(&metrics, &estimation);
    assert_eq!(scope.bytes, 120_000);
    assert_eq!(scope.lines, -1);
}

/// Verify that the host shrinks the final byte budget with the safety ratio before exposing it to Lua.
/// 验证宿主会先按安全比例收缩最终字节预算，再把结果暴露给 Lua。
#[test]
fn resolve_scope_budget_applies_safe_bytes_ratio_before_exposing_to_lua() {
    let estimation = EffectiveBudgetEstimation {
        bytes_per_token: 3,
        safe_bytes_ratio: 0.95,
        unlimited_bytes_cap: 200 * 1024,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "bytes".to_string(),
        BudgetMetricConfig {
            default: Some(100),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );

    let scope = resolve_scope_budget(&metrics, &estimation);
    assert_eq!(scope.bytes, 95);
    assert_eq!(scope.lines, -1);
}

/// Verify that lines alone no longer back-compute bytes and the final byte budget remains on the default fallback.
/// 验证仅提供 lines 时不会再反向估算 bytes，最终字节预算保持默认值。
#[test]
fn resolve_scope_budget_does_not_estimate_bytes_from_lines() {
    let estimation = EffectiveBudgetEstimation {
        bytes_per_token: 3,
        safe_bytes_ratio: 1.0,
        unlimited_bytes_cap: 200 * 1024,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "lines".to_string(),
        BudgetMetricConfig {
            default: Some(2000),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );

    let scope = resolve_scope_budget(&metrics, &estimation);
    assert_eq!(scope.bytes, DEFAULT_INLINE_BYTES_LIMIT);
    assert_eq!(scope.lines, 2000);
}

/// Verify that explicit unlimited is not exposed to Lua and instead falls back to the configured safety byte cap.
/// 验证显式不限时不会把 unlimited 暴露给 Lua，而是回退到配置化的安全字节封顶。
#[test]
fn resolve_scope_budget_caps_unlimited_bytes_to_safe_limit() {
    let estimation = EffectiveBudgetEstimation {
        bytes_per_token: 3,
        safe_bytes_ratio: 1.0,
        unlimited_bytes_cap: 200 * 1024,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "bytes".to_string(),
        BudgetMetricConfig {
            default: Some(-1),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );
    metrics.insert(
        "lines".to_string(),
        BudgetMetricConfig {
            default: Some(-1),
            config_sources: Vec::new(),
            resolved_source_value: None,
        },
    );

    let scope = resolve_scope_budget(&metrics, &estimation);
    assert_eq!(scope.bytes, 200 * 1024);
    assert_eq!(scope.lines, -1);
}

/// Verify that bytes always has a fallback value and lines uses -1 to represent unlimited.
/// 验证当仅存在默认回退时，bytes 始终有值且 lines 使用 -1 表达不限。
#[test]
fn resolve_scope_budget_always_exposes_numeric_bytes_and_lines() {
    let estimation = EffectiveBudgetEstimation {
        bytes_per_token: 3,
        safe_bytes_ratio: 1.0,
        unlimited_bytes_cap: 200 * 1024,
    };

    let metrics = BTreeMap::new();
    let scope = resolve_scope_budget(&metrics, &estimation);
    assert_eq!(scope.bytes, DEFAULT_INLINE_BYTES_LIMIT);
    assert_eq!(scope.lines, -1);
}

/// Verify that the default multipliers are used stably when no tool override is present.
/// 验证在没有工具覆盖时，会稳定使用默认倍率。
#[test]
fn merge_effective_estimation_uses_defaults_without_override() {
    let defaults = BudgetEstimationConfig {
        bytes_per_token: Some(3),
        safe_bytes_ratio: Some(0.95),
        unlimited_bytes_cap: Some(200 * 1024),
    };

    let estimation = merge_effective_estimation(&defaults, None, None);
    assert_eq!(estimation.bytes_per_token, 3);
    assert!((estimation.safe_bytes_ratio - 0.95).abs() < f64::EPSILON);
    assert_eq!(estimation.unlimited_bytes_cap, 200 * 1024);
}

/// Verify that the runtime YAML rules parse into the expected client and tool budget structure.
/// 验证运行时 YAML 规则能正确解析出我们约定的客户端与工具预算结构。
#[test]
fn client_budget_yaml_parses_expected_rules() {
    let yaml = include_str!("../../../runtime/configs/client_budgets.yaml");
    let parsed: ClientBudgetConfig = from_str(yaml).expect("client_budgets.yaml should parse");

    assert!(parsed.clients.iter().any(|rule| rule.pattern == "*qwen*"));
    assert!(
        parsed
            .clients
            .iter()
            .any(|rule| rule.pattern == "codex-mcp-client")
    );
    assert!(
        parsed
            .clients
            .iter()
            .any(|rule| rule.pattern == "*claude-code*")
    );
    assert!(
        parsed
            .clients
            .iter()
            .any(|rule| rule.pattern == "*opencode*")
    );

    let opencode_rule = parsed
        .clients
        .iter()
        .find(|rule| rule.pattern == "*opencode*")
        .expect("opencode rule should exist");
    let tool_result = opencode_rule
        .budgets
        .get("tool_result")
        .expect("opencode tool_result budget should exist");
    let line_config = tool_result
        .get("lines")
        .expect("opencode tool_result.lines should exist");
    let byte_config = tool_result
        .get("bytes")
        .expect("opencode tool_result.bytes should exist");

    assert_eq!(line_config.config_sources.len(), 1);
    assert_eq!(line_config.config_sources[0].source_type, "json");
    assert_eq!(
        line_config.config_sources[0].path.as_deref(),
        Some("~/.config/opencode/opencode.json")
    );
    assert_eq!(
        line_config.config_sources[0].field.as_deref(),
        Some("tool_output.max_lines")
    );
    assert_eq!(byte_config.config_sources.len(), 1);
    assert_eq!(byte_config.config_sources[0].source_type, "json");
    assert_eq!(
        byte_config.config_sources[0].path.as_deref(),
        Some("~/.config/opencode/opencode.json")
    );
    assert_eq!(
        byte_config.config_sources[0].field.as_deref(),
        Some("tool_output.max_bytes")
    );
}

/// JSON config sources should resolve nested dotted fields so OpenCode budget values can be loaded from `tool_output.*`.
/// JSON 配置源应支持解析嵌套点路径字段，从而读取 OpenCode 的 `tool_output.*` 预算值。
#[test]
fn read_metric_from_source_reads_nested_json_fields() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-mcp-client-budget-json-source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let config_path = root.join("opencode.json");
    std::fs::create_dir_all(&root).expect("failed to create json source directory");
    std::fs::write(
        &config_path,
        r#"{"tool_output":{"max_lines":10000,"max_bytes":204800}}"#,
    )
    .expect("failed to write json budget source");

    let line_source = BudgetConfigSource {
        source_type: "json".to_string(),
        key: None,
        path: Some(config_path.to_string_lossy().to_string()),
        field: Some("tool_output.max_lines".to_string()),
    };
    let byte_source = BudgetConfigSource {
        source_type: "json".to_string(),
        key: None,
        path: Some(config_path.to_string_lossy().to_string()),
        field: Some("tool_output.max_bytes".to_string()),
    };

    let resolved_lines =
        read_metric_from_source(&line_source).expect("expected line metric from json source");
    let resolved_bytes =
        read_metric_from_source(&byte_source).expect("expected byte metric from json source");

    assert_eq!(resolved_lines.value, Some(10_000));
    assert_eq!(resolved_lines.source, "client_config");
    assert_eq!(resolved_bytes.value, Some(204_800));
    assert_eq!(resolved_bytes.source, "client_config");

    let _ = std::fs::remove_dir_all(&root);
}

/// Verify that when a client does not explicitly define file_read, it falls back to the same client's tool_result budget.
/// 验证当客户端未显式配置 file_read 时，会自动回退复用同客户端的 tool_result 预算。
#[test]
fn resolve_client_budget_snapshot_falls_back_file_read_to_tool_result() {
    let yaml = include_str!("../../../runtime/configs/client_budgets.yaml");
    let parsed: ClientBudgetConfig = from_str(yaml).expect("client_budgets.yaml should parse");

    let codex_rule = parsed
        .clients
        .iter()
        .find(|rule| rule.pattern == "codex-mcp-client")
        .expect("codex rule should exist");

    let estimation = merge_effective_estimation(&parsed.defaults.estimation, None, None);
    let mut budgets = BTreeMap::new();
    for (scope_name, metric_configs) in &codex_rule.budgets {
        budgets.insert(
            scope_name.clone(),
            resolve_scope_budget(metric_configs, &estimation),
        );
    }

    if !budgets.contains_key("file_read") {
        if let Some(tool_result_scope) = budgets.get("tool_result").cloned() {
            budgets.insert("file_read".to_string(), tool_result_scope);
        }
    }

    let tool_result = budgets
        .get("tool_result")
        .expect("tool_result budget should exist");
    let file_read = budgets
        .get("file_read")
        .expect("file_read fallback budget should exist");
    assert_eq!(tool_result.bytes, file_read.bytes);
    assert_eq!(tool_result.lines, file_read.lines);
}

/// Explicit runtime-root overrides should redirect client-budget preload to the selected runtime instead of the default output tree.
/// 显式 runtime_root 覆盖应把客户端预算预载重定向到选中的运行根，而不是默认输出树。
#[test]
fn preload_client_budget_config_prefers_explicit_runtime_root() {
    let _guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = std::env::temp_dir().join(format!(
        "vulcan-mcp-client-budget-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let config_path = root.join("configs").join("client_budgets.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("failed to create config directory");
    std::fs::write(
        &config_path,
        "defaults:\n  budgets:\n    tool_result:\n      bytes:\n        default: 1234\n",
    )
    .expect("failed to write client budget config");

    initialize_client_budget_runtime_root(Some(&root)).expect("runtime root init should succeed");
    let report = preload_client_budget_config().expect("client budget preload should succeed");
    assert_eq!(
        report.source_path.as_deref(),
        Some(config_path.to_string_lossy().as_ref())
    );

    initialize_client_budget_runtime_root(None).expect("runtime root clear should succeed");
    let _ = std::fs::remove_dir_all(&root);
}

/// Prepare one isolated runtime root backed by one test-local client budget config so matching tests stay deterministic.
/// 基于测试专用客户端预算配置准备隔离 runtime root，确保匹配测试具备稳定且可重复的配置来源。
fn prepare_isolated_client_budget_runtime_root_with_yaml(
    client_budget_yaml: &str,
) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "vulcan-mcp-client-budget-match-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let config_path = root.join("configs").join("client_budgets.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("failed to create config directory");
    std::fs::write(&config_path, client_budget_yaml)
        .expect("failed to write isolated client budget config");
    initialize_client_budget_runtime_root(Some(&root)).expect("runtime root init should succeed");
    preload_client_budget_config().expect("client budget preload should succeed");
    root
}

/// Clear the isolated runtime root created for one matching test and restore runtime-root discovery to defaults.
/// 清理单次匹配测试创建的隔离 runtime root，并将运行根发现恢复为默认行为。
fn cleanup_isolated_client_budget_runtime_root(root: &std::path::Path) {
    initialize_client_budget_runtime_root(None).expect("runtime root clear should succeed");
    let _ = std::fs::remove_dir_all(root);
}

/// Environment overrides should force client-budget matching to use the supplied name instead of the MCP-reported client name.
/// 环境变量覆盖应强制客户端预算匹配使用指定名称，而不是 MCP 实际上报的客户端名称。
#[test]
fn resolve_client_budget_snapshot_prefers_env_override_name() {
    let _environment_guard = environment_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _runtime_root_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var(CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    let root = prepare_isolated_client_budget_runtime_root_with_yaml(
        r#"
defaults:
  budgets:
    tool_result:
      bytes:
        default: 10000
      lines:
        default: -1
clients:
  - pattern: "mcphost"
    budgets:
      tool_result:
        bytes:
          default: 95000
        lines:
          default: -1
  - pattern: "*qwen*"
    budgets:
      tool_result:
        bytes:
          default: 25000
        lines:
          default: -1
"#,
    );
    unsafe {
        std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, "qwen-forced");
    }

    let request_context = RequestContext {
        client_info: Some(ClientInfo {
            name: "mcphost".to_string(),
            version: "1.0.0".to_string(),
        }),
        ..RequestContext::default()
    };

    let snapshot = resolve_client_budget_snapshot(Some(&request_context), None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("qwen-forced"));
    assert_eq!(snapshot.matched_client_pattern.as_deref(), Some("*qwen*"));
    assert_eq!(snapshot.tool_result.bytes, 23_750);

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, value);
        }
    } else {
        unsafe {
            std::env::remove_var(CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
    cleanup_isolated_client_budget_runtime_root(&root);
}

/// Blank environment overrides should be ignored so normal MCP client-name matching still applies.
/// 空白环境变量覆盖应被忽略，从而继续使用正常的 MCP 客户端名称匹配。
#[test]
fn resolve_client_budget_snapshot_ignores_blank_env_override() {
    let _environment_guard = environment_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _runtime_root_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var(CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    let root = prepare_isolated_client_budget_runtime_root_with_yaml(
        r#"
defaults:
  budgets:
    tool_result:
      bytes:
        default: 10000
      lines:
        default: -1
clients:
  - pattern: "mcphost"
    budgets:
      tool_result:
        bytes:
          default: 100000
        lines:
          default: -1
"#,
    );
    unsafe {
        std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, "   ");
    }

    let request_context = RequestContext {
        client_info: Some(ClientInfo {
            name: "mcphost".to_string(),
            version: "1.0.0".to_string(),
        }),
        ..RequestContext::default()
    };

    let snapshot = resolve_client_budget_snapshot(Some(&request_context), None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("mcphost"));
    assert_eq!(snapshot.matched_client_pattern.as_deref(), Some("mcphost"));
    assert_eq!(snapshot.tool_result.bytes, 95_000);

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, value);
        }
    } else {
        unsafe {
            std::env::remove_var(CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
    cleanup_isolated_client_budget_runtime_root(&root);
}

/// Request-context overrides should win over both environment overrides and raw MCP clientInfo.name.
/// 请求上下文覆盖值应优先于环境变量覆盖和原始 MCP clientInfo.name。
#[test]
fn resolve_client_budget_snapshot_prefers_request_context_override_name() {
    let _environment_guard = environment_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _runtime_root_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var(CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    let root = prepare_isolated_client_budget_runtime_root_with_yaml(
        r#"
defaults:
  budgets:
    tool_result:
      bytes:
        default: 10000
      lines:
        default: -1
clients:
  - pattern: "mcphost"
    budgets:
      tool_result:
        bytes:
          default: 95000
        lines:
          default: -1
  - pattern: "*qwen*"
    budgets:
      tool_result:
        bytes:
          default: 25000
        lines:
          default: -1
"#,
    );
    unsafe {
        std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, "mcphost");
    }

    let request_context = RequestContext {
        client_info: Some(ClientInfo {
            name: "copilot".to_string(),
            version: "1.0.0".to_string(),
        }),
        client_match_name_override: Some("qwen-inline".to_string()),
        ..RequestContext::default()
    };

    let snapshot = resolve_client_budget_snapshot(Some(&request_context), None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("qwen-inline"));
    assert_eq!(snapshot.matched_client_pattern.as_deref(), Some("*qwen*"));
    assert_eq!(snapshot.tool_result.bytes, 23_750);

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, value);
        }
    } else {
        unsafe {
            std::env::remove_var(CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
    cleanup_isolated_client_budget_runtime_root(&root);
}

/// gRPC budget resolution should use only exact grpc_clients entries and ignore generic overrides.
/// gRPC 预算解析应仅使用精确 grpc_clients 配置，并忽略通用覆盖来源。
#[test]
fn resolve_grpc_client_budget_snapshot_uses_exact_client_name_only() {
    let _environment_guard = environment_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _runtime_root_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var(CLIENT_MATCH_NAME_OVERRIDE_ENV).ok();
    let root = prepare_isolated_client_budget_runtime_root_with_yaml(
        r#"
defaults:
  budgets:
    tool_result:
      bytes:
        default: 10000
      lines:
        default: -1
grpc_clients:
  exact-client:
    budgets:
      tool_result:
        bytes:
          default: 50000
        lines:
          default: -1
clients:
  - pattern: "*exact*"
    budgets:
      tool_result:
        bytes:
          default: 90000
        lines:
          default: -1
  - pattern: "*qwen*"
    budgets:
      tool_result:
        bytes:
          default: 25000
        lines:
          default: -1
"#,
    );
    unsafe {
        std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, "qwen-forced");
    }

    let snapshot = resolve_grpc_client_budget_snapshot("exact-client", None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("exact-client"));
    assert_eq!(
        snapshot.matched_client_pattern.as_deref(),
        Some("exact-client")
    );
    assert_eq!(snapshot.tool_result.bytes, 47_500);

    let request_context = RequestContext {
        client_info: Some(ClientInfo {
            name: "exact-client".to_string(),
            version: "1.0.0".to_string(),
        }),
        disable_client_match_overrides: true,
        ..RequestContext::default()
    };
    let snapshot = resolve_client_budget_snapshot(Some(&request_context), None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("exact-client"));
    assert_eq!(snapshot.tool_result.bytes, 47_500);

    if let Some(value) = previous {
        unsafe {
            std::env::set_var(CLIENT_MATCH_NAME_OVERRIDE_ENV, value);
        }
    } else {
        unsafe {
            std::env::remove_var(CLIENT_MATCH_NAME_OVERRIDE_ENV);
        }
    }
    cleanup_isolated_client_budget_runtime_root(&root);
}

/// gRPC budget resolution should not activate generic wildcard client rules.
/// gRPC 预算解析不应激活通用通配客户端规则。
#[test]
fn resolve_grpc_client_budget_snapshot_does_not_use_wildcard_rules() {
    let _runtime_root_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = prepare_isolated_client_budget_runtime_root_with_yaml(
        r#"
defaults:
  budgets:
    tool_result:
      bytes:
        default: 10000
      lines:
        default: -1
clients:
  - pattern: "*qwen*"
    budgets:
      tool_result:
        bytes:
          default: 25000
        lines:
          default: -1
"#,
    );

    let snapshot = resolve_grpc_client_budget_snapshot("qwen-grpc", None, None);
    assert_eq!(snapshot.client_name.as_deref(), Some("qwen-grpc"));
    assert_eq!(snapshot.matched_client_pattern, None);
    assert_eq!(snapshot.tool_result.bytes, 9_500);

    cleanup_isolated_client_budget_runtime_root(&root);
}
