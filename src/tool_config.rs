use serde::Serialize;
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Runtime tool-config cache that stores parsed tool configurations and their source path, supporting startup preload and explicit hot reload.
/// 运行时工具配置缓存，保存已解析的工具配置映射与来源路径，支持启动预载与显式热重载。
static TOOL_CONFIG_RUNTIME: OnceLock<RwLock<ToolConfigRuntime>> = OnceLock::new();
/// Optional explicit runtime-root override used to keep tool-config discovery aligned with one selected runtime.
/// 可选的显式 runtime_root 覆盖，用于让工具配置发现链与当前选中的运行根保持一致。
static TOOL_CONFIG_RUNTIME_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// Internal runtime state for the cached tool configuration store.
/// 工具配置缓存的内部运行时状态。
#[derive(Debug, Clone, Default)]
struct ToolConfigRuntime {
    configs: BTreeMap<String, Value>,
    source_path: Option<PathBuf>,
}

/// Tool-config load report used by startup logging and reload results.
/// 工具配置加载结果，便于启动日志与热重载结果输出。
#[derive(Debug, Clone, Serialize)]
pub struct ToolConfigLoadReport {
    pub source_path: Option<String>,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    pub config_counts: BTreeMap<String, usize>,
}

/// Ensure the tool-config cache has been initialized, loading it from disk immediately on the first access.
/// 确保工具配置缓存已经初始化；若尚未初始化则立即从磁盘加载一次。
fn tool_config_runtime() -> &'static RwLock<ToolConfigRuntime> {
    TOOL_CONFIG_RUNTIME.get_or_init(|| {
        let runtime = load_tool_config_runtime().unwrap_or_default();
        RwLock::new(runtime)
    })
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
fn current_tool_config_runtime_root() -> Option<PathBuf> {
    tool_config_runtime_root()
        .read()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Preload tool configs during startup so configuration issues are discovered early.
/// 启动时预载工具配置，便于尽早发现配置格式问题。
pub fn preload_tool_configs() -> Result<ToolConfigLoadReport, String> {
    let runtime = load_tool_config_runtime()?;
    let report = build_tool_config_load_report(&runtime);
    let mut guard = tool_config_runtime()
        .write()
        .map_err(|_| "tool config runtime lock poisoned".to_string())?;
    *guard = runtime;
    Ok(report)
}

/// Explicitly hot-reload tool configs without reloading foundational runtime configs such as `config.yaml`.
/// 显式热重载工具配置，不会重新加载 `config.yaml` 等基础运行配置。
pub fn reload_tool_configs() -> Result<ToolConfigLoadReport, String> {
    preload_tool_configs()
}

/// Resolve the tool config for the current skill; return an empty object when absent so Lua can consume it safely.
/// 读取当前 skill 的工具配置值；若不存在则返回空对象，便于 Lua 侧稳定消费。
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

/// Extract budget-estimation overrides from tool config; only flat first-level scalar keys are honored.
/// 根据工具配置提取预算估算覆盖信息；只有扁平的一层标量键会生效。
pub fn resolve_tool_estimation_override(skill_name: Option<&str>) -> ToolEstimationOverride {
    let tool_config = resolve_tool_config_value(skill_name);
    let Some(object) = tool_config.as_object() else {
        return ToolEstimationOverride::default();
    };

    ToolEstimationOverride {
        bytes_per_token: object.get("bytes_per_token").and_then(value_as_u64),
        unlimited_bytes_cap: object.get("unlimited_bytes_cap").and_then(value_as_u64),
    }
}

/// Budget-estimation override fields that may be supplied through tool config.
/// 工具配置中可影响预算折算的覆盖项。
#[derive(Debug, Clone, Default)]
pub struct ToolEstimationOverride {
    pub bytes_per_token: Option<u64>,
    pub unlimited_bytes_cap: Option<u64>,
}

/// Try to parse a JSON value into an unsigned integer, supporting both number and string inputs.
/// 尝试把 JSON 值解析成无符号整数，支持 number 和 string 两种输入。
fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

/// Load the tool-config runtime state from disk.
/// 从磁盘加载工具配置运行时状态。
fn load_tool_config_runtime() -> Result<ToolConfigRuntime, String> {
    let source_path = find_tool_config_path();
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
        let normalized_value = normalize_flat_tool_config(raw_config).map_err(|error| {
            format!(
                "tool `{0}` config is invalid: {}, {}",
                normalized_name, error
            )
        })?;
        configs.insert(normalized_name.to_string(), normalized_value);
    }
    Ok(configs)
}

/// Validate that one tool config only contains one level of scalar or scalar-array values, without nested objects.
/// 校验单个工具配置只包含一层标量或标量数组，不允许嵌套对象。
fn normalize_flat_tool_config(raw_config: &Value) -> Result<Value, String> {
    let object = raw_config
        .as_object()
        .ok_or_else(|| "tool config must be an object".to_string())?;

    let mut normalized = Map::new();
    for (key, value) in object {
        if key.trim().is_empty() {
            return Err("tool config field name must not be empty".to_string());
        }
        validate_flat_tool_value(value)
            .map_err(|error| format!("field `{}` is invalid: {}, {}", key, key, error))?;
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
fn find_tool_config_path() -> Option<PathBuf> {
    if let Some(runtime_root) = current_tool_config_runtime_root() {
        let runtime_path = runtime_root.join("configs").join("tool_configs.yaml");
        if runtime_path.exists() && runtime_path.is_file() {
            return Some(runtime_path);
        }
        return None;
    }

    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent_dir = exe_dir.parent()?;
    let runtime_path = parent_dir.join("configs").join("tool_configs.yaml");
    if runtime_path.exists() && runtime_path.is_file() {
        return Some(runtime_path);
    }

    let repository_path = Path::new("runtime")
        .join("configs")
        .join("tool_configs.yaml");
    if repository_path.exists() && repository_path.is_file() {
        return Some(repository_path);
    }
    None
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
    use std::sync::{Mutex, OnceLock};

    /// Return one shared mutex used to serialize runtime-root override tests for tool-config loading.
    /// 返回一个共享互斥锁，用于串行化工具配置加载中的运行根覆盖测试。
    fn runtime_root_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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
                "list": ["x", "y", 3]
            }
        });

        let normalized = normalize_tool_config_root(&raw).expect("tool config should be valid");
        assert_eq!(normalized["vulcan-codekit"]["a"], json!(1));
        assert_eq!(normalized["vulcan-codekit"]["list"], json!(["x", "y", 3]));
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
            "vulcan-mcp-tool-config-runtime-{}-{}",
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
