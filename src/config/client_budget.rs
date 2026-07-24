use super::runtime_root::{clone_runtime_root_override, find_optional_runtime_config_file};
use crate::support::RuntimeRequestContext;
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

/// Global client-budget cache containing the explicitly committed state or a not-preloaded error.
/// 全局客户端预算缓存，保存显式提交的状态或未预载错误。
static CLIENT_BUDGET_RUNTIME: OnceLock<RwLock<ClientBudgetRuntimeState>> = OnceLock::new();
/// Optional explicit runtime-root override used to keep client-budget discovery aligned with one selected runtime.
/// 可选的显式 runtime_root 覆盖，用于让客户端预算发现链与当前选中的运行根保持一致。
static CLIENT_BUDGET_RUNTIME_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// Cached client-budget runtime state used to preserve load failures.
/// 客户端预算运行时缓存状态，用于保留加载失败。
pub(super) type ClientBudgetRuntimeState = Result<ClientBudgetRuntime, String>;

/// Return the explicitly preloaded client-budget cache without request-time disk or source I/O.
/// 返回显式预载的客户端预算缓存，且不在请求期执行磁盘或外部来源 I/O。
/// Returns the shared cache lock, initialized to an explicit not-preloaded error when necessary.
/// 返回共享缓存锁；必要时以显式的未预载错误初始化。
pub(super) fn client_budget_runtime() -> &'static RwLock<ClientBudgetRuntimeState> {
    CLIENT_BUDGET_RUNTIME.get_or_init(|| {
        RwLock::new(Err(
            "client budget runtime has not been preloaded".to_string()
        ))
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
fn current_client_budget_runtime_root() -> Result<Option<PathBuf>, String> {
    clone_runtime_root_override(
        client_budget_runtime_root().read(),
        "client budget runtime-root lock poisoned",
    )
}

/// Preload the client-budget config during startup so format issues surface early.
/// 启动时预载客户端预算配置，便于尽早暴露配置格式问题。
#[cfg(test)]
fn preload_client_budget_config() -> Result<ClientBudgetLoadReport, String> {
    let (runtime, report) = stage_client_budget_runtime()?;
    // Serialize standalone preload commits with aggregate reloads and request-time readers.
    // 将独立预载提交与聚合重载及请求期读取串行化。
    let _transaction_guard = super::runtime_config_write_guard()?;
    let mut guard = client_budget_runtime()
        .write()
        .map_err(|_| "client budget runtime lock poisoned".to_string())?;
    *guard = Ok(runtime);
    Ok(report)
}

/// Resolve the client-budget snapshot for the current request without hiding cached load errors.
/// 解析当前请求对应的客户端预算快照，且不隐藏缓存加载错误。
/// Parameters: `request_context` carries transport and client identity evidence for matching.
/// 参数：`request_context` 携带用于匹配的传输与客户端身份依据。
/// Parameters: `tool_name` is the optional runtime tool name being invoked.
/// 参数：`tool_name` 是当前调用的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the resolved budget snapshot or a cached configuration error.
/// 返回解析后的预算快照或缓存的配置错误。
pub fn resolve_client_budget_snapshot(
    request_context: Option<&RuntimeRequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<ClientBudgetSnapshot, String> {
    // Hold one shared generation across client-budget and tool-config cache reads.
    // 在客户端预算与工具配置缓存读取期间持有同一个共享版本。
    let _transaction_guard = super::runtime_config_read_guard()?;
    if let Some(context) = request_context.filter(|context| context.disable_client_match_overrides)
    {
        let exact_client_name = context
            .exact_client_name
            .as_deref()
            .or_else(|| context.client_info.as_ref().map(|info| info.name.as_str()))
            .unwrap_or("");
        return resolve_grpc_client_budget_snapshot_within_transaction(
            exact_client_name,
            tool_name,
            skill_name,
        );
    }

    if let Some(exact_client_name) = request_context
        .and_then(|context| context.exact_client_name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return resolve_grpc_client_budget_snapshot_within_transaction(
            exact_client_name,
            tool_name,
            skill_name,
        );
    }

    let config = load_client_budget_config()?;
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

/// Resolve a gRPC client-budget snapshot by trusted `client_name`, preferring exact overrides before shared pattern rules.
/// 通过受信任的 `client_name` 解析 gRPC 客户端预算快照，优先精确覆盖，再回落到统一 pattern 规则。
/// Parameters: `client_name` is the trusted gRPC client identity.
/// 参数：`client_name` 是受信任的 gRPC 客户端身份。
/// Parameters: `tool_name` is the optional runtime tool name being invoked.
/// 参数：`tool_name` 是当前调用的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the resolved budget snapshot or a cached configuration error.
/// 返回解析后的预算快照或缓存的配置错误。
pub fn resolve_grpc_client_budget_snapshot(
    client_name: &str,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<ClientBudgetSnapshot, String> {
    // Hold one shared generation across client-budget and tool-config cache reads.
    // 在客户端预算与工具配置缓存读取期间持有同一个共享版本。
    let _transaction_guard = super::runtime_config_read_guard()?;
    resolve_grpc_client_budget_snapshot_within_transaction(client_name, tool_name, skill_name)
}

/// Resolve one trusted gRPC budget snapshot while the caller holds the runtime-config transaction guard.
/// 在调用方持有运行时配置事务守卫时解析一份受信任 gRPC 预算快照。
/// Parameters: values match `resolve_grpc_client_budget_snapshot` after transaction acquisition.
/// 参数：各参数与获取事务守卫后的 `resolve_grpc_client_budget_snapshot` 一致。
/// Returns the resolved snapshot or a cached configuration error.
/// 返回解析后的快照或缓存配置错误。
fn resolve_grpc_client_budget_snapshot_within_transaction(
    client_name: &str,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<ClientBudgetSnapshot, String> {
    let config = load_client_budget_config()?;
    let normalized_client_name = client_name.trim().to_string();
    let client_name = if normalized_client_name.is_empty() {
        None
    } else {
        Some(normalized_client_name)
    };
    let matched_grpc_rule = client_name
        .as_ref()
        .and_then(|name| config.grpc_clients.get_key_value(name));
    let normalized_pattern_name = client_name.as_ref().map(|name| name.to_lowercase());
    let matched_client_rule = if matched_grpc_rule.is_some() {
        None
    } else {
        normalized_pattern_name
            .as_ref()
            .and_then(|name| match_client_budget_rule(&config.clients, name))
    };

    build_client_budget_snapshot(
        &config,
        client_name,
        matched_grpc_rule
            .map(|(name, _)| name.clone())
            .or_else(|| matched_client_rule.map(|rule| rule.pattern.clone())),
        matched_grpc_rule
            .map(|(_, rule)| &rule.estimation)
            .or_else(|| matched_client_rule.map(|rule| &rule.estimation)),
        matched_grpc_rule
            .map(|(_, rule)| &rule.budgets)
            .or_else(|| matched_client_rule.map(|rule| &rule.budgets)),
        tool_name,
        skill_name,
    )
}

/// Resolve the effective client name used by host-side matching and runtime request context exposure.
/// 解析宿主侧匹配与运行时请求上下文统一使用的最终客户端名称。
pub fn resolve_effective_client_match_name(
    request_context: Option<&RuntimeRequestContext>,
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

/// Load the client-budget config from the in-memory cache without hiding cached load errors.
/// 从内存缓存加载客户端预算配置，且不隐藏缓存加载错误。
fn load_client_budget_config() -> Result<ClientBudgetConfig, String> {
    let runtime_state = client_budget_runtime()
        .read()
        .map_err(|_| "client budget runtime lock poisoned".to_string())?;
    client_budget_config_from_runtime_state(&runtime_state)
}

/// Clone the client-budget config from one cached runtime state.
/// 从单个缓存运行时状态克隆客户端预算配置。
/// Parameters: `runtime_state` is the cached client-budget state to inspect.
/// 参数：`runtime_state` 是待检查的客户端预算缓存状态。
/// Returns the cloned config or the cached load error.
/// 返回克隆后的配置或缓存的加载错误。
fn client_budget_config_from_runtime_state(
    runtime_state: &ClientBudgetRuntimeState,
) -> Result<ClientBudgetConfig, String> {
    runtime_state
        .as_ref()
        .map(|runtime| runtime.config.clone())
        .map_err(|error| error.clone())
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
) -> Result<ClientBudgetSnapshot, String> {
    let normalized_tool_name = tool_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let normalized_skill_name = skill_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let tool_config = crate::config::tool_config::resolve_tool_config_value_within_transaction(
        normalized_skill_name.as_deref(),
    )?;
    // Convert the validated flat object into typed budget-estimation overrides before merging.
    // 在合并前把已校验的扁平对象转换为类型化预算估算覆盖。
    let tool_estimation_override =
        crate::config::tool_config::tool_estimation_override_from_config(&tool_config)?;

    let estimation = merge_effective_estimation(
        &config.defaults.estimation,
        matched_estimation,
        &tool_estimation_override,
    );

    let scope_configs = matched_budgets
        .filter(|budgets| !budgets.is_empty())
        .unwrap_or(&config.defaults.budgets);

    let mut budgets = BTreeMap::new();
    for (scope_name, metric_configs) in scope_configs {
        let resolved_scope = resolve_scope_budget(metric_configs, &estimation);
        budgets.insert(scope_name.clone(), resolved_scope);
    }

    if !budgets.contains_key("file_read")
        && let Some(tool_result_scope) = budgets.get("tool_result").cloned()
    {
        budgets.insert("file_read".to_string(), tool_result_scope);
    }

    let tool_result = budgets
        .get("tool_result")
        .cloned()
        .unwrap_or(EffectiveBudgetScope {
            bytes: DEFAULT_INLINE_BYTES_LIMIT,
            lines: -1,
        });
    let file_read = budgets
        .get("file_read")
        .cloned()
        .unwrap_or_else(|| tool_result.clone());

    Ok(ClientBudgetSnapshot {
        client_name,
        tool_name: normalized_tool_name.clone(),
        skill_name: normalized_skill_name,
        matched_client_pattern,
        tool_result,
        file_read,
        tool_config,
    })
}

/// Load the client-budget runtime state from disk.
/// 从磁盘加载客户端预算运行时状态。
fn load_client_budget_runtime() -> Result<ClientBudgetRuntime, String> {
    let source_path = find_client_budget_config_path()?;
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
    resolve_budget_sources_in_place(&mut parsed)?;

    Ok(ClientBudgetRuntime {
        config: parsed,
        source_path: Some(path),
    })
}

/// Stage one fully resolved client-budget runtime and report without mutating the shared cache.
/// 分阶段加载一份完整解析的客户端预算运行时及报告，不修改共享缓存。
/// Returns the staged runtime/report pair or the first discovery, parse, or external-source error.
/// 返回分阶段运行时与报告，或首个发现、解析或外部来源错误。
pub(super) fn stage_client_budget_runtime()
-> Result<(ClientBudgetRuntime, ClientBudgetLoadReport), String> {
    let runtime = load_client_budget_runtime()?;
    let report = build_client_budget_load_report(&runtime);
    Ok((runtime, report))
}

/// Find the client-budget config file, preferring the runtime output directory and then falling back to the repository template path.
/// 查找客户端预算配置文件；优先查运行时输出目录，其次回退到仓库内模板路径。
fn find_client_budget_config_path() -> Result<Option<PathBuf>, String> {
    find_optional_runtime_config_file(
        current_client_budget_runtime_root()?,
        "client_budgets.yaml",
        "client budget",
    )
}

/// Build a client-budget load report from the current runtime state.
/// 根据运行时状态构建客户端预算加载报告。
fn build_client_budget_load_report(runtime: &ClientBudgetRuntime) -> ClientBudgetLoadReport {
    // Load reports summarize client defaults without applying any skill-specific tool override.
    // 加载报告只汇总客户端默认值，不应用任何 skill 专用工具覆盖。
    let default_tool_override = crate::config::tool_config::ToolEstimationOverride::default();
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
        estimation: merge_effective_estimation(
            &runtime.config.defaults.estimation,
            None,
            &default_tool_override,
        ),
        resolved_previews: build_resolved_preview_map(&runtime.config),
    }
}

/// Pre-resolve all external budget sources during startup/reload and write the results back into the config so request handling no longer re-reads env vars or user config files.
/// 把所有外部预算来源在启动/重载阶段预解析并写回配置结构，避免请求期再重复读取环境变量或用户配置文件。
/// Returns success after all present external sources are validated, or the first source-resolution error.
/// 返回值：所有已存在外部来源完成校验后返回成功，或返回首个来源解析错误。
fn resolve_budget_sources_in_place(config: &mut ClientBudgetConfig) -> Result<(), String> {
    resolve_scope_sources_in_place(&mut config.defaults.budgets)?;
    for client_rule in &mut config.clients {
        resolve_scope_sources_in_place(&mut client_rule.budgets)?;
    }
    for grpc_client_rule in config.grpc_clients.values_mut() {
        resolve_scope_sources_in_place(&mut grpc_client_rule.budgets)?;
    }
    Ok(())
}

/// Pre-resolve every metric inside one budget-scope collection.
/// 对某个预算 scope 集合内的所有 metric 做预解析。
/// Returns success after every metric source chain is resolved or skipped, or the first malformed source error.
/// 返回值：每个 metric 来源链完成解析或跳过后返回成功，或返回首个格式错误来源。
fn resolve_scope_sources_in_place(scopes: &mut BudgetScopesConfig) -> Result<(), String> {
    for metric_configs in scopes.values_mut() {
        for metric_config in metric_configs.values_mut() {
            metric_config.resolved_source_value = None;
            for source in &metric_config.config_sources {
                let Some(resolved) = read_metric_from_source(source)? else {
                    continue;
                };
                metric_config.resolved_source_value = Some(resolved);
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
