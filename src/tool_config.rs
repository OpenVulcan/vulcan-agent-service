use serde::Serialize;
use serde_json::{Map, Value};
#[cfg(test)]
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// 中文：运行时工具配置缓存，保存已解析的工具配置映射与来源路径，支持启动预载与显式热重载。
/// English: Runtime tool-config cache that stores parsed tool configurations and their source path, supporting startup preload and explicit hot reload.
static TOOL_CONFIG_RUNTIME: OnceLock<RwLock<ToolConfigRuntime>> = OnceLock::new();

/// 中文：工具配置缓存的内部运行时状态。
/// English: Internal runtime state for the cached tool configuration store.
#[derive(Debug, Clone, Default)]
struct ToolConfigRuntime {
    configs: BTreeMap<String, Value>,
    source_path: Option<PathBuf>,
}

/// 中文：工具配置加载结果，便于启动日志与热重载结果输出。
/// English: Tool-config load report used by startup logging and reload results.
#[derive(Debug, Clone, Serialize)]
pub struct ToolConfigLoadReport {
    pub source_path: Option<String>,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    pub config_counts: BTreeMap<String, usize>,
}

/// 中文：确保工具配置缓存已经初始化；若尚未初始化则立即从磁盘加载一次。
/// English: Ensure the tool-config cache has been initialized, loading it from disk immediately on the first access.
fn tool_config_runtime() -> &'static RwLock<ToolConfigRuntime> {
    TOOL_CONFIG_RUNTIME.get_or_init(|| {
        let runtime = load_tool_config_runtime().unwrap_or_default();
        RwLock::new(runtime)
    })
}

/// 中文：启动时预载工具配置，便于尽早发现配置格式问题。
/// English: Preload tool configs during startup so configuration issues are discovered early.
pub fn preload_tool_configs() -> Result<ToolConfigLoadReport, String> {
    let runtime = load_tool_config_runtime()?;
    let report = build_tool_config_load_report(&runtime);
    let mut guard = tool_config_runtime()
        .write()
        .map_err(|_| "tool config runtime lock poisoned / 工具配置缓存写锁已损坏".to_string())?;
    *guard = runtime;
    Ok(report)
}

/// 中文：显式热重载工具配置，不会重新加载 `config.yaml` 等基础运行配置。
/// English: Explicitly hot-reload tool configs without reloading foundational runtime configs such as `config.yaml`.
pub fn reload_tool_configs() -> Result<ToolConfigLoadReport, String> {
    preload_tool_configs()
}

/// 中文：读取当前 skill 的工具配置值；若不存在则返回空对象，便于 Lua 侧稳定消费。
/// English: Resolve the tool config for the current skill; return an empty object when absent so Lua can consume it safely.
pub fn resolve_tool_config_value(skill_name: Option<&str>) -> Value {
    let normalized_skill_name = skill_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let Some(skill_name) = normalized_skill_name else {
        return Value::Object(Map::new());
    };

    let guard = match tool_config_runtime().read() {
        Ok(guard) => guard,
        Err(_) => return Value::Object(Map::new()),
    };

    guard
        .configs
        .get(&skill_name)
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// 中文：根据工具配置提取预算估算覆盖信息；只有扁平的一层标量键会生效。
/// English: Extract budget-estimation overrides from tool config; only flat first-level scalar keys are honored.
pub fn resolve_tool_estimation_override(skill_name: Option<&str>) -> ToolEstimationOverride {
    let tool_config = resolve_tool_config_value(skill_name);
    let Some(object) = tool_config.as_object() else {
        return ToolEstimationOverride::default();
    };

    ToolEstimationOverride {
        bytes_per_token: object.get("bytes_per_token").and_then(value_as_u64),
        bytes_per_line: object.get("bytes_per_line").and_then(value_as_u64),
        unlimited_bytes_cap: object.get("unlimited_bytes_cap").and_then(value_as_u64),
    }
}

/// 中文：工具配置中可影响预算折算的覆盖项。
/// English: Budget-estimation override fields that may be supplied through tool config.
#[derive(Debug, Clone, Default)]
pub struct ToolEstimationOverride {
    pub bytes_per_token: Option<u64>,
    pub bytes_per_line: Option<u64>,
    pub unlimited_bytes_cap: Option<u64>,
}

/// 中文：尝试把 JSON 值解析成无符号整数，支持 number 和 string 两种输入。
/// English: Try to parse a JSON value into an unsigned integer, supporting both number and string inputs.
fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

/// 中文：从磁盘加载工具配置运行时状态。
/// English: Load the tool-config runtime state from disk.
fn load_tool_config_runtime() -> Result<ToolConfigRuntime, String> {
    let source_path = find_tool_config_path();
    let Some(path) = source_path else {
        return Ok(ToolConfigRuntime::default());
    };

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read tool config file {} / 读取工具配置文件失败: {}",
            path.display(),
            error
        )
    })?;
    let parsed_yaml: Value = serde_yaml::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse tool config YAML {} / 解析工具配置 YAML 失败: {}",
            path.display(),
            error
        )
    })?;
    let configs = normalize_tool_config_root(&parsed_yaml).map_err(|error| {
        format!(
            "Invalid tool config file {} / 工具配置文件格式无效: {}",
            path.display(),
            error
        )
    })?;

    Ok(ToolConfigRuntime {
        configs,
        source_path: Some(path),
    })
}

/// 中文：把原始 YAML 根对象规范化为 “skill_name -> flat config object” 的映射。
/// English: Normalize the raw YAML root object into a `skill_name -> flat config object` mapping.
fn normalize_tool_config_root(root: &Value) -> Result<BTreeMap<String, Value>, String> {
    let root_object = root
        .as_object()
        .ok_or_else(|| "tool_configs.yaml root must be an object / 根节点必须是对象".to_string())?;

    let mut configs = BTreeMap::new();
    for (skill_name, raw_config) in root_object {
        let normalized_name = skill_name.trim();
        if normalized_name.is_empty() {
            return Err("tool config key must not be empty / 工具配置键不能为空".to_string());
        }
        let normalized_value = normalize_flat_tool_config(raw_config).map_err(|error| {
            format!(
                "tool `{0}` config is invalid / 工具 `{0}` 配置无效: {1}",
                normalized_name, error
            )
        })?;
        configs.insert(normalized_name.to_string(), normalized_value);
    }
    Ok(configs)
}

/// 中文：校验单个工具配置只包含一层标量或标量数组，不允许嵌套对象。
/// English: Validate that one tool config only contains one level of scalar or scalar-array values, without nested objects.
fn normalize_flat_tool_config(raw_config: &Value) -> Result<Value, String> {
    let object = raw_config
        .as_object()
        .ok_or_else(|| "tool config must be an object / 工具配置必须是对象".to_string())?;

    let mut normalized = Map::new();
    for (key, value) in object {
        if key.trim().is_empty() {
            return Err("tool config field name must not be empty / 字段名不能为空".to_string());
        }
        validate_flat_tool_value(value).map_err(|error| {
            format!(
                "field `{}` is invalid / 字段 `{}` 无效: {}",
                key, key, error
            )
        })?;
        normalized.insert(key.clone(), value.clone());
    }

    Ok(Value::Object(normalized))
}

/// 中文：校验工具配置值只能是标量、null 或标量数组。
/// English: Validate that a tool-config value is either a scalar, null, or an array of scalars.
fn validate_flat_tool_value(value: &Value) -> Result<(), String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
                    _ => {
                        return Err(
                            "arrays may only contain scalar values / 数组只能包含标量值"
                                .to_string(),
                        )
                    }
                }
            }
            Ok(())
        }
        Value::Object(_) => Err("nested objects are not allowed / 不允许嵌套对象".to_string()),
    }
}

/// 中文：查找工具配置文件；优先使用运行时输出目录，其次回退到仓库模板目录。
/// English: Find the tool-config file, preferring the runtime output directory and then falling back to the repository template directory.
fn find_tool_config_path() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent_dir = exe_dir.parent()?;
    let runtime_path = parent_dir.join("configs").join("tool_configs.yaml");
    if runtime_path.exists() {
        return Some(runtime_path);
    }

    let repository_path = Path::new("runtime")
        .join("configs")
        .join("tool_configs.yaml");
    if repository_path.exists() {
        return Some(repository_path);
    }
    None
}

/// 中文：根据运行时状态构建统一的加载报告。
/// English: Build a normalized load report from the current runtime state.
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

    /// 中文：验证工具配置支持一层标量与数组值。
    /// English: Verify that tool configs support one-level scalar values and arrays.
    #[test]
    fn normalize_tool_config_root_accepts_flat_values_and_arrays() {
        let raw = json!({
            "vulcan-codekit": {
                "a": 1,
                "b": 10,
                "c": "100",
                "list": ["x", "y", 3]
            }
        });

        let normalized = normalize_tool_config_root(&raw).expect("tool config should be valid");
        assert_eq!(normalized["vulcan-codekit"]["a"], json!(1));
        assert_eq!(normalized["vulcan-codekit"]["list"], json!(["x", "y", 3]));
    }

    /// 中文：验证嵌套对象会被拒绝，避免工具配置无限膨胀。
    /// English: Verify that nested objects are rejected to keep tool config intentionally shallow.
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
}
