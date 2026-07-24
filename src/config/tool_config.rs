use super::runtime_root::{clone_runtime_root_override, find_optional_runtime_config_file};
use serde::Serialize;
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Runtime tool-config cache that stores the explicitly committed state or a not-preloaded error.
/// 运行时工具配置缓存，保存显式提交的状态或未预载错误。
static TOOL_CONFIG_RUNTIME: OnceLock<RwLock<ToolConfigRuntimeState>> = OnceLock::new();
/// Optional explicit runtime-root override used to keep tool-config discovery aligned with one selected runtime.
/// 可选的显式 runtime_root 覆盖，用于让工具配置发现链与当前选中的运行根保持一致。
static TOOL_CONFIG_RUNTIME_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// Internal runtime state for the cached tool configuration store.
/// 工具配置缓存的内部运行时状态。
#[derive(Debug, Clone, Default)]
pub(super) struct ToolConfigRuntime {
    configs: BTreeMap<String, Value>,
    source_path: Option<PathBuf>,
}

/// Cached tool-config runtime state used to preserve load failures.
/// 工具配置运行时缓存状态，用于保留加载失败。
pub(super) type ToolConfigRuntimeState = Result<ToolConfigRuntime, String>;

/// Tool-config load report used by startup logging and reload results.
/// 工具配置加载结果，便于启动日志与热重载结果输出。
#[derive(Debug, Clone, Serialize)]
pub struct ToolConfigLoadReport {
    pub source_path: Option<String>,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    pub config_counts: BTreeMap<String, usize>,
}

/// Return the explicitly preloaded tool-config cache without performing request-time disk I/O.
/// 返回显式预载的工具配置缓存，且不在请求期执行磁盘 I/O。
/// Returns the shared cache lock, initialized to an explicit not-preloaded error when necessary.
/// 返回共享缓存锁；必要时以显式的未预载错误初始化。
pub(super) fn tool_config_runtime() -> &'static RwLock<ToolConfigRuntimeState> {
    TOOL_CONFIG_RUNTIME
        .get_or_init(|| RwLock::new(Err("tool config runtime has not been preloaded".to_string())))
}

/// Return the shared runtime-root override store used by tool-config discovery.
/// 返回工具配置发现链使用的共享运行根覆盖存储。
fn tool_config_runtime_root() -> &'static RwLock<Option<PathBuf>> {
    TOOL_CONFIG_RUNTIME_ROOT.get_or_init(|| RwLock::new(None))
}

/// Initialize the runtime-root override used by tool-config preload and reload.
/// 初始化供工具配置预载与热重载使用的运行根覆盖值。
pub fn initialize_tool_config_runtime_root(runtime_root: Option<&Path>) -> Result<(), String> {
    let mut guard = tool_config_runtime_root()
        .write()
        .map_err(|_| "tool config runtime-root lock poisoned".to_string())?;
    *guard = runtime_root.map(std::path::Path::to_path_buf);
    Ok(())
}

/// Read the current runtime-root override used by tool-config discovery.
/// 读取当前工具配置发现链使用的运行根覆盖值。
fn current_tool_config_runtime_root() -> Result<Option<PathBuf>, String> {
    clone_runtime_root_override(
        tool_config_runtime_root().read(),
        "tool config runtime-root lock poisoned",
    )
}

/// Preload tool configs during startup so configuration issues are discovered early.
/// 启动时预载工具配置，便于尽早发现配置格式问题。
#[cfg(test)]
fn preload_tool_configs() -> Result<ToolConfigLoadReport, String> {
    let (runtime, report) = stage_tool_config_runtime()?;
    // Serialize standalone preload commits with aggregate reloads and request-time readers.
    // 将独立预载提交与聚合重载及请求期读取串行化。
    let _transaction_guard = super::runtime_config_write_guard()?;
    let mut guard = tool_config_runtime()
        .write()
        .map_err(|_| "tool config runtime lock poisoned".to_string())?;
    *guard = Ok(runtime);
    Ok(report)
}

/// Resolve one cached tool config while the caller holds the shared runtime-config read transaction.
/// 在调用方持有共享运行时配置读取事务时解析一份缓存工具配置。
/// Parameters: `skill_name` is the optional skill name used to look up flat tool config.
/// 参数：`skill_name` 是用于查找扁平工具配置的可选 skill 名称。
/// Returns the matching config, an empty object for absent config, or the cached load error.
/// 返回匹配配置、缺失配置对应的空对象，或缓存的加载错误。
pub(super) fn resolve_tool_config_value_within_transaction(
    skill_name: Option<&str>,
) -> Result<Value, String> {
    let normalized_skill_name = skill_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let guard = match tool_config_runtime().read() {
        Ok(guard) => guard,
        Err(_) => return Err("tool config runtime lock poisoned".to_string()),
    };

    resolve_tool_config_value_from_runtime_state(&guard, normalized_skill_name.as_deref())
}

/// Resolve one skill config from a cached runtime state.
/// 从单个缓存运行时状态解析一个 skill 配置。
/// Parameters: `runtime_state` is the cached tool-config state to inspect.
/// 参数：`runtime_state` 是待检查的工具配置缓存状态。
/// Parameters: `skill_name` is the normalized skill name to look up.
/// 参数：`skill_name` 是需要查找的规范化 skill 名称。
/// Returns the matching tool config, an empty object for absent config, or the cached load error.
/// 返回匹配的工具配置、缺失配置对应的空对象，或缓存的加载错误。
fn resolve_tool_config_value_from_runtime_state(
    runtime_state: &ToolConfigRuntimeState,
    skill_name: Option<&str>,
) -> Result<Value, String> {
    // Propagate cached load errors before interpreting an absent skill as an empty config.
    // 在把缺失 skill 解释为空配置前先传播缓存加载错误。
    let runtime = runtime_state.as_ref().map_err(|error| error.clone())?;
    let Some(skill_name) = skill_name else {
        return Ok(Value::Object(Map::new()));
    };

    Ok(runtime
        .configs
        .get(skill_name)
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new())))
}

/// Extract budget-estimation overrides from one already-resolved tool config value.
/// 从一份已经解析出的工具配置值中提取预算估算覆盖信息。
/// Parameters: `tool_config` is the flat tool config value for one skill.
/// 参数：`tool_config` 是单个 skill 的扁平工具配置值。
/// Returns parsed estimation overrides or an explicit object/field type error.
/// 返回解析出的估算覆盖，或显式的对象/字段类型错误。
pub(super) fn tool_estimation_override_from_config(
    tool_config: &Value,
) -> Result<ToolEstimationOverride, String> {
    // Resolved tool configs must retain the object shape established by the load boundary.
    // 已解析工具配置必须保持加载边界建立的对象形态。
    let object = tool_config
        .as_object()
        .ok_or_else(|| "resolved tool config must be an object".to_string())?;

    Ok(ToolEstimationOverride {
        bytes_per_token: optional_tool_estimation_u64(object, "bytes_per_token")?,
        unlimited_bytes_cap: optional_tool_estimation_u64(object, "unlimited_bytes_cap")?,
    })
}

/// Budget-estimation override fields that may be supplied through tool config.
/// 工具配置中可影响预算折算的覆盖项。
#[derive(Debug, Clone, Default)]
pub(super) struct ToolEstimationOverride {
    /// Optional tool-level tokens-to-bytes multiplier.
    /// 可选的工具级 token 到字节换算倍率。
    pub(super) bytes_per_token: Option<u64>,
    /// Optional tool-level cap used for unlimited byte budgets.
    /// 可选的工具级不限字节预算封顶值。
    pub(super) unlimited_bytes_cap: Option<u64>,
}

/// Read one optional estimation field from a validated flat tool-config object.
/// 从已校验的扁平工具配置对象读取一个可选估算字段。
/// Parameters: `object` is the resolved flat tool-config object.
/// 参数：`object` 是已经解析出的扁平工具配置对象。
/// Parameters: `field_name` is the reserved estimation field to read.
/// 参数：`field_name` 是需要读取的保留估算字段。
/// Returns the absent or parsed unsigned integer field, or an explicit type error.
/// 返回缺失或已解析的无符号整数字段，或显式类型错误。
fn optional_tool_estimation_u64(
    object: &Map<String, Value>,
    field_name: &str,
) -> Result<Option<u64>, String> {
    object
        .get(field_name)
        .map(|value| {
            parse_tool_estimation_u64(value)
                .map_err(|error| format!("tool config field `{field_name}` is invalid: {error}"))
        })
        .transpose()
}

/// Parse one reserved estimation value as a JSON/YAML unsigned integer number.
/// 把一个保留估算值解析为 JSON/YAML 无符号整数数字。
/// Parameters: `value` is the raw field value from the flat tool config.
/// 参数：`value` 是扁平工具配置中的原始字段值。
/// Returns the parsed integer or a type-specific validation error.
/// 返回解析后的整数或包含类型信息的校验错误。
fn parse_tool_estimation_u64(value: &Value) -> Result<u64, String> {
    value.as_u64().ok_or_else(|| {
        format!(
            "expected an unsigned integer number, got {}",
            tool_config_value_kind(value)
        )
    })
}

/// Describe one JSON value kind for tool-config validation diagnostics.
/// 描述工具配置校验诊断使用的 JSON 值类型。
/// Parameters: `value` is the JSON value whose kind is reported.
/// 参数：`value` 是需要报告类型的 JSON 值。
/// Returns a stable human-readable kind label.
/// 返回稳定且适合人类阅读的类型标签。
fn tool_config_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.as_u64().is_some() => "unsigned integer number",
        Value::Number(number) if number.as_i64().is_some() => "negative integer number",
        Value::Number(_) => "floating-point number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Return whether one tool-config field has reserved budget-estimation semantics.
/// 判断一个工具配置字段是否具有保留的预算估算语义。
/// Parameters: `field_name` is the normalized flat tool-config field name.
/// 参数：`field_name` 是规范化后的扁平工具配置字段名。
/// Returns `true` only for fields consumed by budget estimation.
/// 仅对预算估算实际消费的字段返回 `true`。
fn is_tool_estimation_field(field_name: &str) -> bool {
    matches!(field_name, "bytes_per_token" | "unlimited_bytes_cap")
}

/// Load the tool-config runtime state from disk.
/// 从磁盘加载工具配置运行时状态。
fn load_tool_config_runtime() -> Result<ToolConfigRuntime, String> {
    let source_path = find_tool_config_path()?;
    let Some(path) = source_path else {
        return Ok(ToolConfigRuntime::default());
    };

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read tool config file {}: {}",
            path.display(),
            error
        )
    })?;
    let parsed_yaml: Value = serde_yaml::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse tool config YAML {}: {}",
            path.display(),
            error
        )
    })?;
    let configs = normalize_tool_config_root(&parsed_yaml)
        .map_err(|error| format!("Invalid tool config file {}: {}", path.display(), error))?;

    Ok(ToolConfigRuntime {
        configs,
        source_path: Some(path),
    })
}

/// Stage one validated tool-config runtime and its report without mutating the shared cache.
/// 分阶段加载一份已校验工具配置运行时及其报告，不修改共享缓存。
/// Returns the staged runtime/report pair or the first discovery, read, parse, or validation error.
/// 返回分阶段运行时与报告，或首个发现、读取、解析或校验错误。
pub(super) fn stage_tool_config_runtime()
-> Result<(ToolConfigRuntime, ToolConfigLoadReport), String> {
    let runtime = load_tool_config_runtime()?;
    let report = build_tool_config_load_report(&runtime);
    Ok((runtime, report))
}

/// Normalize the raw YAML root object into a `skill_name -> flat config object` mapping.
/// 把原始 YAML 根对象规范化为 “skill_name -> flat config object” 的映射。
fn normalize_tool_config_root(root: &Value) -> Result<BTreeMap<String, Value>, String> {
    let root_object = root
        .as_object()
        .ok_or_else(|| "tool_configs.yaml root must be an object".to_string())?;

    let mut configs = BTreeMap::new();
    for (skill_name, raw_config) in root_object {
        let normalized_name = skill_name.trim();
        if normalized_name.is_empty() {
            return Err("tool config key must not be empty".to_string());
        }
        if normalized_name != skill_name {
            return Err(format!(
                "tool config key `{skill_name}` must not contain surrounding whitespace"
            ));
        }
        let normalized_value = normalize_flat_tool_config(normalized_name, raw_config)?;
        configs.insert(normalized_name.to_string(), normalized_value);
    }
    Ok(configs)
}

/// Validate and normalize one skill's flat tool config, including reserved estimation field types.
/// 校验并规范化单个 skill 的扁平工具配置，包括保留估算字段类型。
/// Parameters: `skill_name` is the normalized skill name used in field-path diagnostics.
/// 参数：`skill_name` 是字段路径诊断使用的规范化 skill 名称。
/// Parameters: `raw_config` is the raw one-level tool-config object.
/// 参数：`raw_config` 是原始的一层工具配置对象。
/// Returns the validated flat object or a field-path-aware validation error.
/// 返回校验后的扁平对象，或包含字段路径的校验错误。
fn normalize_flat_tool_config(skill_name: &str, raw_config: &Value) -> Result<Value, String> {
    let object = raw_config
        .as_object()
        .ok_or_else(|| format!("tool `{skill_name}` config must be an object"))?;

    let mut normalized = Map::new();
    for (key, value) in object {
        // Compare against the normalized spelling so reserved lookalike keys cannot bypass validation.
        // 对比规范化拼写，避免保留字段近似键绕过校验。
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return Err(format!(
                "tool `{skill_name}` config field name must not be empty"
            ));
        }
        if normalized_key != key {
            return Err(format!(
                "tool config field `{skill_name}.{key}` must not contain surrounding whitespace"
            ));
        }
        validate_flat_tool_value(value).map_err(|error| {
            format!("tool config field `{skill_name}.{key}` is invalid: {error}")
        })?;
        if is_tool_estimation_field(normalized_key) {
            parse_tool_estimation_u64(value).map_err(|error| {
                format!("tool config field `{skill_name}.{key}` is invalid: {error}")
            })?;
        }
        normalized.insert(key.clone(), value.clone());
    }

    Ok(Value::Object(normalized))
}

/// Validate that a tool-config value is either a scalar, null, or an array of scalars.
/// 校验工具配置值只能是标量、null 或标量数组。
fn validate_flat_tool_value(value: &Value) -> Result<(), String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
                    _ => {
                        return Err("arrays may only contain scalar values".to_string());
                    }
                }
            }
            Ok(())
        }
        Value::Object(_) => Err("nested objects are not allowed".to_string()),
    }
}

/// Find the tool-config file, preferring the runtime output directory and then falling back to the repository template directory.
/// 查找工具配置文件；优先使用运行时输出目录，其次回退到仓库模板目录。
fn find_tool_config_path() -> Result<Option<PathBuf>, String> {
    find_optional_runtime_config_file(
        current_tool_config_runtime_root()?,
        "tool_configs.yaml",
        "tool config",
    )
}

/// Build a normalized load report from the current runtime state.
/// 根据运行时状态构建统一的加载报告。
fn build_tool_config_load_report(runtime: &ToolConfigRuntime) -> ToolConfigLoadReport {
    ToolConfigLoadReport {
        source_path: runtime
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        tool_count: runtime.configs.len(),
        tool_names: runtime.configs.keys().cloned().collect(),
        config_counts: runtime
            .configs
            .iter()
            .map(|(tool_name, config)| {
                let count = config.as_object().map(|object| object.len()).unwrap_or(0);
                (tool_name.clone(), count)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Return one shared mutex used to serialize runtime-root override tests for tool-config loading.
    /// 返回一个共享互斥锁，用于串行化工具配置加载中的运行根覆盖测试。
    fn runtime_root_lock() -> &'static std::sync::Mutex<()> {
        crate::config::runtime_config_test_lock()
    }

    /// Cached tool-config errors should surface instead of becoming an empty tool config object.
    /// 缓存的工具配置错误应显式暴露，而不是变成空工具配置对象。
    #[test]
    fn resolve_tool_config_value_from_runtime_state_reports_cached_error() {
        // Keep one deterministic cached failure to exercise every missing-skill representation.
        // 保留一个确定的缓存失败，用于覆盖每种缺失 skill 表示。
        let runtime_state: ToolConfigRuntimeState = Err("cached tool config failure".to_string());
        // Include a named skill, an absent skill, and a blank skill to prevent early empty-object returns.
        // 同时覆盖具名 skill、缺失 skill 与空白 skill，防止提前返回空对象。
        let skill_names = [Some("vulcan-codekit"), None, Some("   ")];

        for skill_name in skill_names {
            // Resolve each representation against the same failed cache state.
            // 针对同一失败缓存状态解析每种表示。
            let error = resolve_tool_config_value_from_runtime_state(&runtime_state, skill_name)
                .expect_err("cached tool config error should be returned");

            assert_eq!(error, "cached tool config failure");
        }
    }

    /// Verify that tool configs support one-level scalar values and arrays.
    /// 验证工具配置支持一层标量与数组值。
    #[test]
    fn normalize_tool_config_root_accepts_flat_values_and_arrays() {
        let raw = json!({
            "vulcan-codekit": {
                "a": 1,
                "b": 10,
                "c": "100",
                "list": ["x", "y", 3],
                "bytes_per_token": 3,
                "unlimited_bytes_cap": 204800
            }
        });

        let normalized = normalize_tool_config_root(&raw).expect("tool config should be valid");
        assert_eq!(normalized["vulcan-codekit"]["a"], json!(1));
        assert_eq!(normalized["vulcan-codekit"]["list"], json!(["x", "y", 3]));
        // Extract the validated reserved fields through the same typed consumer used by budgets.
        // 通过预算使用的同一个类型化消费入口提取已校验的保留字段。
        let estimation = tool_estimation_override_from_config(&normalized["vulcan-codekit"])
            .expect("validated estimation fields should extract");
        assert_eq!(estimation.bytes_per_token, Some(3));
        assert_eq!(estimation.unlimited_bytes_cap, Some(204800));
    }

    /// Reserved estimation fields should reject every present value that is not an unsigned integer number.
    /// 保留估算字段应拒绝所有不是无符号整数数字的显式值。
    #[test]
    fn normalize_tool_config_root_rejects_invalid_estimation_field_types() {
        // Cover string compatibility, signed numbers, floats, booleans, null, and arrays across both reserved fields.
        // 覆盖两个保留字段上的字符串兼容、负数、浮点数、布尔值、null 与数组。
        let invalid_cases = [
            ("bytes_per_token", json!("3")),
            ("bytes_per_token", json!(-1)),
            ("bytes_per_token", json!(3.5)),
            ("bytes_per_token", json!(true)),
            ("unlimited_bytes_cap", Value::Null),
            ("unlimited_bytes_cap", json!([])),
        ];

        for (field_name, invalid_value) in invalid_cases {
            // Build one isolated config containing exactly the invalid reserved field under test.
            // 构建一份仅包含当前待测非法保留字段的隔离配置。
            let raw = json!({
                "vulcan-codekit": {
                    (field_name): invalid_value
                }
            });
            // Normalize through the same boundary used by startup preload and hot reload.
            // 通过启动预载与热重载使用的同一个边界执行规范化。
            let error = normalize_tool_config_root(&raw)
                .expect_err("invalid reserved estimation field should be rejected");

            assert!(
                error.contains(&format!("vulcan-codekit.{field_name}")),
                "error should contain the complete field path: {error}"
            );
            assert!(
                error.contains("expected an unsigned integer number"),
                "error should describe the required type: {error}"
            );
        }
    }

    /// Typed estimation extraction should fail explicitly if an invalid object bypasses load validation.
    /// 若非法对象绕过加载校验，类型化估算提取应显式失败。
    #[test]
    fn tool_estimation_override_rejects_invalid_bypassed_value() {
        // Construct an invalid resolved value to exercise the consumer-side invariant guard directly.
        // 构造非法解析值，直接覆盖消费侧不变量守卫。
        let invalid_config = json!({"bytes_per_token": "3"});
        let error = tool_estimation_override_from_config(&invalid_config)
            .expect_err("invalid bypassed estimation value should fail extraction");

        assert!(error.contains("bytes_per_token"));
        assert!(error.contains("expected an unsigned integer number"));
    }

    /// Tool-config field names should reject surrounding whitespace instead of creating ignored lookalike keys.
    /// 工具配置字段名应拒绝首尾空白，避免产生会被忽略的近似键。
    #[test]
    fn normalize_tool_config_root_rejects_field_name_whitespace() {
        // Use a reserved-field lookalike that would otherwise bypass typed budget extraction.
        // 使用一个保留字段近似键；若不拒绝空白，它会绕过类型化预算提取。
        let raw = json!({
            "vulcan-codekit": {
                " bytes_per_token": 3
            }
        });
        let error = normalize_tool_config_root(&raw)
            .expect_err("surrounding field-name whitespace should be rejected");

        assert!(error.contains("must not contain surrounding whitespace"));
    }

    /// Top-level skill names should reject surrounding whitespace instead of silently overwriting a normalized peer.
    /// 顶层 skill 名应拒绝首尾空白，而不是静默覆盖同名的规范化键。
    #[test]
    fn normalize_tool_config_root_rejects_skill_name_whitespace() {
        // Include both spellings to prove that normalization cannot collapse them into one cache entry.
        // 同时提供两种拼写，证明规范化不会把它们折叠为同一缓存项。
        let raw = json!({
            "vulcan-codekit": {"bytes_per_token": 3},
            " vulcan-codekit ": {"bytes_per_token": 4}
        });
        // Normalize through the production load boundary and require an explicit key diagnostic.
        // 通过生产加载边界规范化，并要求返回显式键名诊断。
        let error = normalize_tool_config_root(&raw)
            .expect_err("surrounding skill-name whitespace should be rejected");

        assert!(error.contains("must not contain surrounding whitespace"));
    }

    /// Verify that nested objects are rejected to keep tool config intentionally shallow.
    /// 验证嵌套对象会被拒绝，避免工具配置无限膨胀。
    #[test]
    fn normalize_tool_config_root_rejects_nested_objects() {
        let raw = json!({
            "vulcan-codekit": {
                "nested": {
                    "value": 1
                }
            }
        });

        let error = normalize_tool_config_root(&raw).expect_err("nested object must be rejected");
        assert!(error.contains("vulcan-codekit"));
    }

    /// Explicit runtime-root overrides should redirect tool-config preload to the selected runtime instead of the default output tree.
    /// 显式 runtime_root 覆盖应把工具配置预载重定向到选中的运行根，而不是默认输出树。
    #[test]
    fn preload_tool_configs_prefers_explicit_runtime_root() {
        let _guard = runtime_root_lock().lock().expect("lock should succeed");
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-tool-config-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let config_path = root.join("configs").join("tool_configs.yaml");
        std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
            .expect("failed to create config directory");
        std::fs::write(&config_path, "vulcan-codekit:\n  bytes_per_token: 17\n")
            .expect("failed to write tool config");

        initialize_tool_config_runtime_root(Some(&root)).expect("runtime root init should succeed");
        let report = preload_tool_configs().expect("tool config preload should succeed");
        assert_eq!(
            report.source_path.as_deref(),
            Some(config_path.to_string_lossy().as_ref())
        );

        initialize_tool_config_runtime_root(None).expect("runtime root clear should succeed");
        let _ = std::fs::remove_dir_all(&root);
    }
}
