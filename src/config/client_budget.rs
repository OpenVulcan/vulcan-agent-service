use crate::transport::mcp::protocol::RequestContext;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

mod preview;
mod resolution;
mod types;

use preview::build_resolved_preview_map;
use resolution::{
    match_client_budget_rule, merge_effective_estimation, read_metric_from_source,
    resolve_scope_budget,
};
use types::ClientBudgetRuntime;
pub use types::{
    BudgetConfigSource, BudgetEstimationConfig, BudgetMetricConfig, BudgetScopesConfig,
    ClientBudgetConfig, ClientBudgetLoadReport, ClientBudgetRule, ClientBudgetSnapshot,
    EffectiveBudgetEstimation, EffectiveBudgetScope,
};

/// Default safe inline byte budget used when no external rule can be resolved.
/// 客户端预算默认的安全内联字节上限；当外部配置未提供任何规则时回退到该值。
const DEFAULT_INLINE_BYTES_LIMIT: u64 = 10_000;

/// Default token-to-byte multiplier. Per user requirement this defaults to 3.
/// 按 token 折算为字节时的默认倍率；用户要求默认按 3 倍处理。
const DEFAULT_BYTES_PER_TOKEN: u64 = 3;

/// Safety ratio applied by the host before exposing effective byte budgets to Lua, reserving 5% headroom by default.
/// 宿主在向 Lua 暴露实际字节预算前预留的安全比例，默认保留 5% 冗余。
const DEFAULT_SAFE_BYTES_RATIO: f64 = 0.95;

/// Default hard byte cap exposed to Lua when a client configuration explicitly declares "unlimited".
/// 当客户端配置显式“不限”时，对 Lua 暴露的实际字节上限默认封顶 200KB。
const DEFAULT_UNLIMITED_BYTES_CAP: u64 = 200 * 1024;
/// Environment variable used to override the client name that budget matching sees across all transports.
/// 用于覆盖预算匹配所见客户端名称的环境变量，适用于所有传输模式。
pub const CLIENT_MATCH_NAME_OVERRIDE_ENV: &str = "VULCAN_CLIENT_MATCH_NAME";
/// HTTP/SSE request header used to force the effective client name seen by host-side matching.
/// HTTP/SSE 请求头，宿主侧匹配会用它强制覆盖当前实际客户端名称。
pub const CLIENT_MATCH_NAME_OVERRIDE_HEADER: &str = "Vulcan-Client-Match-Name";

/// Global cached client-budget configuration with support for startup preload and explicit hot reload.
/// 全局缓存客户端预算配置，支持启动预载与显式热重载。
static CLIENT_BUDGET_RUNTIME: OnceLock<RwLock<ClientBudgetRuntime>> = OnceLock::new();
/// Optional explicit runtime-root override used to keep client-budget discovery aligned with one selected runtime.
/// 可选的显式 runtime_root 覆盖，用于让客户端预算发现链与当前选中的运行根保持一致。
static CLIENT_BUDGET_RUNTIME_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// Ensure the client-budget cache is initialized, loading it from disk immediately on first access.
/// 确保客户端预算缓存已初始化；若尚未初始化则立即从磁盘加载。
fn client_budget_runtime() -> &'static RwLock<ClientBudgetRuntime> {
    CLIENT_BUDGET_RUNTIME.get_or_init(|| {
        let runtime = load_client_budget_runtime().unwrap_or_default();
        RwLock::new(runtime)
    })
}

/// Return the shared runtime-root override store used by client-budget discovery.
/// 返回客户端预算发现链使用的共享运行根覆盖存储。
fn client_budget_runtime_root() -> &'static RwLock<Option<PathBuf>> {
    CLIENT_BUDGET_RUNTIME_ROOT.get_or_init(|| RwLock::new(None))
}

/// Initialize the runtime-root override used by client-budget preload and reload.
/// 初始化供客户端预算预载与热重载使用的运行根覆盖值。
pub fn initialize_client_budget_runtime_root(runtime_root: Option<&Path>) -> Result<(), String> {
    let mut guard = client_budget_runtime_root()
        .write()
        .map_err(|_| "client budget runtime-root lock poisoned".to_string())?;
    *guard = runtime_root.map(std::path::Path::to_path_buf);
    Ok(())
}

/// Read the current runtime-root override used by client-budget discovery.
/// 读取当前客户端预算发现链使用的运行根覆盖值。
fn current_client_budget_runtime_root() -> Option<PathBuf> {
    client_budget_runtime_root()
        .read()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Preload the client-budget config during startup so format issues surface early.
/// 启动时预载客户端预算配置，便于尽早暴露配置格式问题。
pub fn preload_client_budget_config() -> Result<ClientBudgetLoadReport, String> {
    let runtime = load_client_budget_runtime()?;
    let report = build_client_budget_load_report(&runtime);
    let mut guard = client_budget_runtime()
        .write()
        .map_err(|_| "client budget runtime lock poisoned".to_string())?;
    *guard = runtime;
    Ok(report)
}

/// Explicitly hot-reload client-budget config without reloading foundational configs such as `config.yaml`.
/// 显式热重载客户端预算配置，不会重新加载 `config.yaml` 等基础运行配置。
pub fn reload_client_budget_config() -> Result<ClientBudgetLoadReport, String> {
    preload_client_budget_config()
}

/// Resolve the client-budget snapshot for the current request; return a safe fallback when the config is missing or cannot be parsed.
/// 解析当前请求对应的客户端预算快照；当配置缺失或解析失败时，返回安全默认值。
pub fn resolve_client_budget_snapshot(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    if let Some(context) = request_context.filter(|context| context.disable_client_match_overrides)
    {
        let exact_client_name = context
            .exact_client_name
            .as_deref()
            .or_else(|| context.client_info.as_ref().map(|info| info.name.as_str()))
            .unwrap_or("");
        return resolve_grpc_client_budget_snapshot(exact_client_name, tool_name, skill_name);
    }

    if let Some(exact_client_name) = request_context
        .and_then(|context| context.exact_client_name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return resolve_grpc_client_budget_snapshot(exact_client_name, tool_name, skill_name);
    }

    let config = load_client_budget_config();
    let client_name = resolve_effective_client_match_name(request_context);
    let normalized_client_name = client_name.as_ref().map(|name| name.to_lowercase());

    let matched_client_rule = normalized_client_name
        .as_ref()
        .and_then(|name| match_client_budget_rule(&config.clients, name));

    build_client_budget_snapshot(
        &config,
        client_name,
        matched_client_rule.map(|rule| rule.pattern.clone()),
        matched_client_rule.map(|rule| &rule.estimation),
        matched_client_rule.map(|rule| &rule.budgets),
        tool_name,
        skill_name,
    )
}

/// Resolve a gRPC client-budget snapshot by exact `client_name` without environment or pattern matching.
/// 通过精确 `client_name` 解析 gRPC 客户端预算快照，不读取环境变量，也不执行 pattern 匹配。
pub fn resolve_grpc_client_budget_snapshot(
    client_name: &str,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    let config = load_client_budget_config();
    let normalized_client_name = client_name.trim().to_string();
    let client_name = if normalized_client_name.is_empty() {
        None
    } else {
        Some(normalized_client_name)
    };
    let matched_grpc_rule = client_name
        .as_ref()
        .and_then(|name| config.grpc_clients.get_key_value(name));

    build_client_budget_snapshot(
        &config,
        client_name,
        matched_grpc_rule.map(|(name, _)| name.clone()),
        matched_grpc_rule.map(|(_, rule)| &rule.estimation),
        matched_grpc_rule.map(|(_, rule)| &rule.budgets),
        tool_name,
        skill_name,
    )
}

/// Resolve the effective client name used by host-side matching and runtime request context exposure.
/// 解析宿主侧匹配与运行时请求上下文统一使用的最终客户端名称。
pub fn resolve_effective_client_match_name(
    request_context: Option<&RequestContext>,
) -> Option<String> {
    if let Some(context) = request_context.filter(|context| context.disable_client_match_overrides)
    {
        return context
            .exact_client_name
            .as_ref()
            .or_else(|| context.client_info.as_ref().map(|info| &info.name))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    request_context
        .and_then(|context| context.exact_client_name.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            request_context
                .and_then(|context| context.client_match_name_override.as_ref())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            std::env::var(CLIENT_MATCH_NAME_OVERRIDE_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            request_context
                .and_then(|context| context.client_info.as_ref())
                .map(|info| info.name.trim().to_string())
                .filter(|name| !name.is_empty())
        })
}

/// Load the client-budget config, preferring the in-memory cache and otherwise parsing the runtime YAML file.
/// 加载客户端预算配置；优先读取缓存，其次从运行时配置文件中解析。
fn load_client_budget_config() -> ClientBudgetConfig {
    match client_budget_runtime().read() {
        Ok(guard) => guard.config.clone(),
        Err(_) => ClientBudgetConfig::default(),
    }
}

/// Build one final budget snapshot from already-selected client rule fragments.
/// 基于已选定的客户端规则片段构造最终预算快照。
fn build_client_budget_snapshot(
    config: &ClientBudgetConfig,
    client_name: Option<String>,
    matched_client_pattern: Option<String>,
    matched_estimation: Option<&BudgetEstimationConfig>,
    matched_budgets: Option<&BudgetScopesConfig>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    let normalized_tool_name = tool_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let normalized_skill_name = skill_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let estimation = merge_effective_estimation(
        &config.defaults.estimation,
        matched_estimation,
        normalized_skill_name.as_deref(),
    );

    let scope_configs = matched_budgets
        .filter(|budgets| !budgets.is_empty())
        .unwrap_or(&config.defaults.budgets);

    let mut budgets = BTreeMap::new();
    for (scope_name, metric_configs) in scope_configs {
        let resolved_scope = resolve_scope_budget(metric_configs, &estimation);
        budgets.insert(scope_name.clone(), resolved_scope);
    }

    if !budgets.contains_key("file_read") {
        if let Some(tool_result_scope) = budgets.get("tool_result").cloned() {
            budgets.insert("file_read".to_string(), tool_result_scope);
        }
    }

    let tool_result = budgets
        .get("tool_result")
        .cloned()
        .unwrap_or_else(|| EffectiveBudgetScope {
            bytes: DEFAULT_INLINE_BYTES_LIMIT,
            lines: -1,
        });
    let file_read = budgets
        .get("file_read")
        .cloned()
        .unwrap_or_else(|| tool_result.clone());

    let tool_config =
        crate::config::tool_config::resolve_tool_config_value(normalized_skill_name.as_deref());

    ClientBudgetSnapshot {
        client_name,
        tool_name: normalized_tool_name.clone(),
        skill_name: normalized_skill_name,
        matched_client_pattern,
        tool_result,
        file_read,
        tool_config,
    }
}

/// Load the client-budget runtime state from disk.
/// 从磁盘加载客户端预算运行时状态。
fn load_client_budget_runtime() -> Result<ClientBudgetRuntime, String> {
    let source_path = find_client_budget_config_path();
    let Some(path) = source_path else {
        return Ok(ClientBudgetRuntime::default());
    };

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read client budget config {}: {}",
            path.display(),
            error
        )
    })?;
    let mut parsed: ClientBudgetConfig = serde_yaml::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse client budget config {}: {}",
            path.display(),
            error
        )
    })?;
    resolve_budget_sources_in_place(&mut parsed);

    Ok(ClientBudgetRuntime {
        config: parsed,
        source_path: Some(path),
    })
}

/// Find the client-budget config file, preferring the runtime output directory and then falling back to the repository template path.
/// 查找客户端预算配置文件；优先查运行时输出目录，其次回退到仓库内模板路径。
fn find_client_budget_config_path() -> Option<PathBuf> {
    if let Some(runtime_root) = current_client_budget_runtime_root() {
        let runtime_path = runtime_root.join("configs").join("client_budgets.yaml");
        if runtime_path.exists() && runtime_path.is_file() {
            return Some(runtime_path);
        }
        return None;
    }

    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent_dir = exe_dir.parent()?;
    let runtime_path = parent_dir.join("configs").join("client_budgets.yaml");
    if runtime_path.exists() && runtime_path.is_file() {
        return Some(runtime_path);
    }

    let repository_path = Path::new("runtime")
        .join("configs")
        .join("client_budgets.yaml");
    if repository_path.exists() && repository_path.is_file() {
        return Some(repository_path);
    }
    None
}

/// Build a client-budget load report from the current runtime state.
/// 根据运行时状态构建客户端预算加载报告。
fn build_client_budget_load_report(runtime: &ClientBudgetRuntime) -> ClientBudgetLoadReport {
    ClientBudgetLoadReport {
        source_path: runtime
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        client_count: runtime.config.clients.len(),
        client_patterns: runtime
            .config
            .clients
            .iter()
            .map(|rule| rule.pattern.clone())
            .collect(),
        grpc_client_count: runtime.config.grpc_clients.len(),
        grpc_client_names: runtime.config.grpc_clients.keys().cloned().collect(),
        estimation: merge_effective_estimation(&runtime.config.defaults.estimation, None, None),
        resolved_previews: build_resolved_preview_map(&runtime.config),
    }
}

/// Pre-resolve all external budget sources during startup/reload and write the results back into the config so request handling no longer re-reads env vars or user config files.
/// 把所有外部预算来源在启动/重载阶段预解析并写回配置结构，避免请求期再重复读取环境变量或用户配置文件。
fn resolve_budget_sources_in_place(config: &mut ClientBudgetConfig) {
    resolve_scope_sources_in_place(&mut config.defaults.budgets);
    for client_rule in &mut config.clients {
        resolve_scope_sources_in_place(&mut client_rule.budgets);
    }
    for grpc_client_rule in config.grpc_clients.values_mut() {
        resolve_scope_sources_in_place(&mut grpc_client_rule.budgets);
    }
}

/// Pre-resolve every metric inside one budget-scope collection.
/// 对某个预算 scope 集合内的所有 metric 做预解析。
fn resolve_scope_sources_in_place(scopes: &mut BudgetScopesConfig) {
    for metric_configs in scopes.values_mut() {
        for metric_config in metric_configs.values_mut() {
            metric_config.resolved_source_value = metric_config
                .config_sources
                .iter()
                .find_map(read_metric_from_source);
        }
    }
}

#[cfg(test)]
mod tests;
