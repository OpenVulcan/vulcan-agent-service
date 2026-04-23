use crate::protocol::RequestContext;
use crate::tool_config::resolve_tool_estimation_override;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

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

/// Runtime cache state for client budgets, containing the parsed config and its source path.
/// 客户端预算运行时缓存状态，包含已解析配置与来源路径。
#[derive(Debug, Clone, Default)]
struct ClientBudgetRuntime {
    config: ClientBudgetConfig,
    source_path: Option<PathBuf>,
}

/// Client-budget load report used for startup logs and hot-reload return values.
/// 客户端预算加载报告，用于启动日志和热重载返回值。
#[derive(Debug, Clone, Serialize)]
pub struct ClientBudgetLoadReport {
    pub source_path: Option<String>,
    pub client_count: usize,
    pub client_patterns: Vec<String>,
    pub estimation: EffectiveBudgetEstimation,
    pub resolved_previews: BTreeMap<String, Value>,
}

/// Root client-budget configuration containing default estimation rules, fallback budgets, and per-client budget rules.
/// 客户端预算配置根对象，包含默认估算规则、默认预算以及按客户端匹配的预算规则。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClientBudgetConfig {
    #[serde(default)]
    pub defaults: ClientBudgetDefaults,
    #[serde(default)]
    pub clients: Vec<ClientBudgetRule>,
}

/// Default client-budget settings containing both fallback budget values and fallback estimation multipliers.
/// 客户端预算的默认配置，既包含预算默认值，也包含估算倍率默认值。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClientBudgetDefaults {
    #[serde(default)]
    pub estimation: BudgetEstimationConfig,
    #[serde(default)]
    pub budgets: BudgetScopesConfig,
}

/// Client budget matching rule activated by a client-name pattern.
/// 客户端预算匹配规则，按客户端名称 pattern 生效。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClientBudgetRule {
    pub pattern: String,
    #[serde(default)]
    pub estimation: BudgetEstimationConfig,
    #[serde(default)]
    pub budgets: BudgetScopesConfig,
}

/// Budget estimation config that only keeps tokens-to-bytes, safety ratio, and unlimited-to-bytes-cap conversion controls.
/// 预算估算倍率配置，仅保留 tokens→bytes、安全比例与 unlimited→bytes cap 三类统一控制。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BudgetEstimationConfig {
    pub bytes_per_token: Option<u64>,
    pub safe_bytes_ratio: Option<f64>,
    pub unlimited_bytes_cap: Option<u64>,
}

/// Budget scope config. The first key is the budget scope (such as tool_result or file_read), and the second key is the metric (such as tokens, lines, or bytes).
/// 预算场景配置，第一层 key 为场景名（如 tool_result、file_read），第二层 key 为度量名（如 tokens、lines、bytes）。
pub type BudgetScopesConfig = BTreeMap<String, BTreeMap<String, BudgetMetricConfig>>;

/// Configuration for one budget metric, including a default value and an ordered list of external config sources.
/// 单个预算度量配置，包含默认值与外部配置源解析列表。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BudgetMetricConfig {
    pub default: Option<i64>,
    #[serde(default)]
    pub config_sources: Vec<BudgetConfigSource>,
    #[serde(skip)]
    resolved_source_value: Option<ResolvedMetricValue>,
}

/// External budget source definition supporting env / json / toml inputs.
/// 预算配置外部来源，支持 env / json / toml 三类输入。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BudgetConfigSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub key: Option<String>,
    pub path: Option<String>,
    pub field: Option<String>,
}

/// Internal resolved budget value that preserves only the raw numeric value and its source.
/// 已解析预算值的内部表示，仅保留原始数值与来源。
/// `-1` means an external config explicitly declares unlimited, while `None` means the metric is absent.
/// `-1` 代表外部配置显式不限，`None` 代表该度量未提供。
#[derive(Debug, Clone)]
struct ResolvedMetricValue {
    value: Option<i64>,
    source: String,
}

/// Final client-budget snapshot exposed to Lua.
/// 最终暴露给 Lua 的客户端预算快照。
/// It directly exposes the `tool_result/file_read` scopes and no longer keeps the old nested `budgets` compatibility structure.
/// 直接提供 `tool_result/file_read` 两个 scope，不再继续兼容旧的 `budgets` 嵌套旧结构。
#[derive(Debug, Clone, Serialize)]
pub struct ClientBudgetSnapshot {
    pub client_name: Option<String>,
    pub tool_name: Option<String>,
    pub skill_name: Option<String>,
    pub matched_client_pattern: Option<String>,
    pub tool_result: EffectiveBudgetScope,
    pub file_read: EffectiveBudgetScope,
    pub tool_config: Value,
}

/// Snapshot of the final effective budget-estimation multipliers.
/// 最终生效的预算估算倍率快照。
#[derive(Debug, Clone, Serialize)]
pub struct EffectiveBudgetEstimation {
    pub bytes_per_token: u64,
    pub safe_bytes_ratio: f64,
    pub unlimited_bytes_cap: u64,
}

/// Final budget-scope info exposed to Lua; only directly consumable `bytes` and `lines` remain.
/// 最终暴露给 Lua 的单个预算场景信息；只保留直接可消费的 `bytes` 与 `lines`。
/// `bytes` always has a value, and `lines=-1` means unlimited.
/// `bytes` 永远有值，`lines=-1` 表示不限。
#[derive(Debug, Clone, Serialize, Default)]
pub struct EffectiveBudgetScope {
    pub bytes: u64,
    pub lines: i64,
}

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
    let config = load_client_budget_config();
    let client_name = resolve_effective_client_match_name(request_context);
    let normalized_client_name = client_name.as_ref().map(|name| name.to_lowercase());
    let normalized_tool_name = tool_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let normalized_skill_name = skill_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let matched_client_rule = normalized_client_name
        .as_ref()
        .and_then(|name| match_client_budget_rule(&config.clients, name));

    let estimation = merge_effective_estimation(
        &config.defaults.estimation,
        matched_client_rule.map(|rule| &rule.estimation),
        normalized_skill_name.as_deref(),
    );

    let scope_configs = matched_client_rule
        .map(|rule| &rule.budgets)
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
        crate::tool_config::resolve_tool_config_value(normalized_skill_name.as_deref());

    ClientBudgetSnapshot {
        client_name,
        tool_name: normalized_tool_name.clone(),
        skill_name: normalized_skill_name,
        matched_client_pattern: matched_client_rule.map(|rule| rule.pattern.clone()),
        tool_result,
        file_read,
        tool_config,
    }
}

/// Resolve the effective client name used by host-side matching and runtime request context exposure.
/// 解析宿主侧匹配与运行时请求上下文统一使用的最终客户端名称。
pub fn resolve_effective_client_match_name(
    request_context: Option<&RequestContext>,
) -> Option<String> {
    request_context
        .and_then(|context| context.client_match_name_override.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

/// Build a preview of the pre-resolved client budgets so startup and reload can print the actual loaded values directly.
/// 构建预解析后的客户端预算摘要，便于启动和热重载时直接输出实际读取值。
fn build_resolved_preview_map(config: &ClientBudgetConfig) -> BTreeMap<String, Value> {
    let mut previews = BTreeMap::new();
    for client_rule in &config.clients {
        let estimation = merge_effective_estimation(
            &config.defaults.estimation,
            Some(&client_rule.estimation),
            None,
        );
        previews.insert(
            client_rule.pattern.clone(),
            build_scope_preview_value(&client_rule.budgets, &estimation),
        );
    }
    previews
}

/// Convert one client's scope budgets into a structured preview value.
/// 把某个客户端规则下的 scope 预算转换成结构化摘要。
fn build_scope_preview_value(
    scopes: &BudgetScopesConfig,
    estimation: &EffectiveBudgetEstimation,
) -> Value {
    let mut scope_map = serde_json::Map::new();
    for (scope_name, metric_configs) in scopes {
        let mut source_set = BTreeMap::<String, ()>::new();
        let mut byte_candidates = Vec::new();
        let mut raw_tokens: Option<i64> = None;
        let mut raw_bytes: Option<i64> = None;
        let mut raw_lines: Option<i64> = None;

        for (metric_name, metric_config) in metric_configs {
            let resolved = metric_config
                .resolved_source_value
                .clone()
                .unwrap_or_else(|| default_resolved_metric_value(metric_config));
            source_set.insert(resolved.source.clone(), ());
            match metric_name.as_str() {
                "tokens" => {
                    if let Some(value) = resolved.value {
                        raw_tokens = Some(value);
                        if value == -1 {
                            byte_candidates.push(estimation.unlimited_bytes_cap);
                        } else if value >= 0 {
                            byte_candidates
                                .push((value as u64).saturating_mul(estimation.bytes_per_token));
                        }
                    }
                }
                "bytes" => {
                    if let Some(value) = resolved.value {
                        raw_bytes = Some(value);
                        if value == -1 {
                            byte_candidates.push(estimation.unlimited_bytes_cap);
                        } else if value >= 0 {
                            byte_candidates.push(value as u64);
                        }
                    }
                }
                "lines" => {
                    if let Some(value) = resolved.value {
                        raw_lines = Some(if value > 0 { value } else { -1 });
                    }
                }
                _ => {}
            }
        }

        let effective_bytes = apply_safe_bytes_ratio(
            byte_candidates
                .into_iter()
                .min()
                .unwrap_or(DEFAULT_INLINE_BYTES_LIMIT),
            estimation.safe_bytes_ratio,
        );
        let source = if source_set.is_empty() {
            "default".to_string()
        } else if source_set.len() == 1 {
            source_set
                .into_keys()
                .next()
                .unwrap_or_else(|| "default".to_string())
        } else {
            format!(
                "mixed({})",
                source_set.into_keys().collect::<Vec<_>>().join(",")
            )
        };

        scope_map.insert(
            scope_name.clone(),
            json!({
                "bytes": effective_bytes,
                "lines": raw_lines.unwrap_or(-1),
                "source": source,
                "raw_tokens": raw_tokens,
                "raw_bytes": raw_bytes,
                "raw_lines": raw_lines,
            }),
        );
    }
    Value::Object(scope_map)
}

/// Match a client-budget rule using simple `*` / `?` wildcard semantics.
/// 匹配客户端预算规则，使用简单的 `*` / `?` 通配符匹配。
fn match_client_budget_rule<'a>(
    rules: &'a [ClientBudgetRule],
    client_name: &str,
) -> Option<&'a ClientBudgetRule> {
    rules
        .iter()
        .find(|rule| wildcard_match(&rule.pattern.to_lowercase(), &client_name.to_lowercase()))
}

/// Merge the default estimation config with budget-estimation overrides extracted from tool config.
/// 合并默认估算配置与工具配置中的预算估算覆盖，生成最终估算倍率。
fn merge_effective_estimation(
    defaults: &BudgetEstimationConfig,
    client_override: Option<&BudgetEstimationConfig>,
    skill_name: Option<&str>,
) -> EffectiveBudgetEstimation {
    let tool_override = resolve_tool_estimation_override(skill_name);
    EffectiveBudgetEstimation {
        bytes_per_token: tool_override
            .bytes_per_token
            .or(client_override.and_then(|override_config| override_config.bytes_per_token))
            .or(defaults.bytes_per_token)
            .unwrap_or(DEFAULT_BYTES_PER_TOKEN),
        safe_bytes_ratio: normalize_safe_bytes_ratio(
            client_override
                .and_then(|override_config| override_config.safe_bytes_ratio)
                .or(defaults.safe_bytes_ratio)
                .unwrap_or(DEFAULT_SAFE_BYTES_RATIO),
        ),
        unlimited_bytes_cap: tool_override
            .unlimited_bytes_cap
            .or(client_override.and_then(|override_config| override_config.unlimited_bytes_cap))
            .or(defaults.unlimited_bytes_cap)
            .unwrap_or(DEFAULT_UNLIMITED_BYTES_CAP),
    }
}

/// Resolve all metrics for a budget scope and derive the final `bytes/lines` numbers exposed to Lua.
/// 解析单个预算场景的所有度量，并折算出最终对 Lua 暴露的 `bytes/lines` 数值。
fn resolve_scope_budget(
    metric_configs: &BTreeMap<String, BudgetMetricConfig>,
    estimation: &EffectiveBudgetEstimation,
) -> EffectiveBudgetScope {
    let mut scope = EffectiveBudgetScope {
        bytes: DEFAULT_INLINE_BYTES_LIMIT,
        lines: -1,
    };
    let mut inline_byte_candidates = Vec::new();

    for (metric_name, metric_config) in metric_configs {
        let resolved_metric = resolve_metric_value(metric_config);

        match metric_name.as_str() {
            "tokens" => {
                let effective_numeric =
                    effective_metric_value(&resolved_metric, estimation, metric_name);
                if let Some(tokens) = effective_numeric.metric_value {
                    inline_byte_candidates.push(tokens.saturating_mul(estimation.bytes_per_token));
                }
            }
            "lines" => {
                let effective_numeric =
                    effective_metric_value(&resolved_metric, estimation, metric_name);
                scope.lines = effective_numeric.line_value;
            }
            "bytes" => {
                let effective_numeric =
                    effective_metric_value(&resolved_metric, estimation, metric_name);
                if let Some(bytes) = effective_numeric.metric_value {
                    inline_byte_candidates.push(bytes);
                }
            }
            _ => {}
        }
    }

    scope.bytes = apply_safe_bytes_ratio(
        inline_byte_candidates
            .into_iter()
            .min()
            .unwrap_or(DEFAULT_INLINE_BYTES_LIMIT),
        estimation.safe_bytes_ratio,
    );

    scope
}

/// Convert a nominal byte budget into the host-exposed safe byte budget for Lua, always keeping at least 1 byte.
/// 把名义字节预算折算成宿主实际暴露给 Lua 的安全字节预算，始终保留至少 1 字节。
fn apply_safe_bytes_ratio(bytes: u64, ratio: f64) -> u64 {
    let normalized_ratio = normalize_safe_bytes_ratio(ratio);
    let adjusted = ((bytes as f64) * normalized_ratio).floor() as u64;
    adjusted.max(1)
}

/// Normalize the safety ratio, falling back to the default 0.95 when the input is invalid.
/// 归一化安全比例，非法值统一回退到默认的 0.95。
fn normalize_safe_bytes_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() || ratio <= 0.0 || ratio > 1.0 {
        DEFAULT_SAFE_BYTES_RATIO
    } else {
        ratio
    }
}

/// Resolve the final value for one budget metric, preferring external config sources and then falling back to the YAML default.
/// 解析单个预算度量的最终值，优先使用外部配置源，其次回退到 YAML 默认值。
fn resolve_metric_value(metric_config: &BudgetMetricConfig) -> ResolvedMetricValue {
    if let Some(parsed) = metric_config.resolved_source_value.clone() {
        return parsed;
    }

    default_resolved_metric_value(metric_config)
}

/// Convert the YAML default into the unified resolved budget representation.
/// 把默认配置转换成统一的已解析预算值表示。
/// A missing default means the metric is absent, while `-1` preserves the raw "explicit unlimited" meaning for later conversion.
/// 缺省值表示该度量未配置；`-1` 保留为“显式不限”的原始语义，后续再根据度量类型折算。
fn default_resolved_metric_value(metric_config: &BudgetMetricConfig) -> ResolvedMetricValue {
    ResolvedMetricValue {
        value: metric_config.default,
        source: "default".to_string(),
    }
}

/// Read a budget value from an external source, supporting env / json / toml.
/// 从外部配置源中读取预算值，支持 env / json / toml。
fn read_metric_from_source(source: &BudgetConfigSource) -> Option<ResolvedMetricValue> {
    match source.source_type.to_lowercase().as_str() {
        "env" => {
            let key = source.key.as_ref()?;
            let raw_value = std::env::var(key).ok()?;
            parse_metric_literal(&raw_value, "env")
        }
        "json" => {
            let path = expand_user_home(source.path.as_deref()?);
            let field = source.field.as_deref()?;
            let content = fs::read_to_string(path).ok()?;
            let json_value: Value = serde_json::from_str(&content).ok()?;
            let target = traverse_dotted_json_field(&json_value, field)?;
            parse_metric_json_value(target, "client_config")
        }
        "toml" => {
            let path = expand_user_home(source.path.as_deref()?);
            let field = source.field.as_deref()?;
            let content = fs::read_to_string(path).ok()?;
            let toml_value: toml::Value = toml::from_str(&content).ok()?;
            let json_value = serde_json::to_value(toml_value).ok()?;
            let target = traverse_dotted_json_field(&json_value, field)?;
            parse_metric_json_value(target, "client_config")
        }
        _ => None,
    }
}

/// Convert a resolved metric value into the actual numeric value exposed to Lua.
/// 将解析后的度量值折算成对 Lua 暴露的实际数值。
/// - `bytes` / `tokens` use the safe byte cap when the raw value is `-1`.
/// - 当原始值为 `-1` 时，`bytes` / `tokens` 会统一收敛到安全字节上限。
/// - `lines` become `-1` when the raw value is `<= 0`.
/// - 当原始值 `<= 0` 时，`lines` 会统一暴露为 `-1`。
fn effective_metric_value(
    resolved: &ResolvedMetricValue,
    estimation: &EffectiveBudgetEstimation,
    metric_name: &str,
) -> EffectiveMetricNumericValue {
    let metric_value = match resolved.value {
        Some(value) if value == -1 => match metric_name {
            "bytes" | "tokens" => Some(estimation.unlimited_bytes_cap),
            "lines" => None,
            _ => None,
        },
        Some(value) if value >= 0 => Some(value as u64),
        _ => None,
    };

    let line_value = match resolved.value {
        Some(value) if metric_name == "lines" && value > 0 => value,
        _ => -1,
    };

    EffectiveMetricNumericValue {
        metric_value,
        line_value,
    }
}

/// Internal helper that carries the converted numeric value for one metric.
/// 内部辅助结构，用来承载单个度量折算后的数值。
struct EffectiveMetricNumericValue {
    metric_value: Option<u64>,
    line_value: i64,
}

/// Parse one textual budget literal. `-1` means explicit unlimited, while non-negative integers mean concrete limits.
/// 解析单个文本字面量预算值；`-1` 代表显式不限，非负整数代表具体额度。
fn parse_metric_literal(raw_value: &str, source_name: &str) -> Option<ResolvedMetricValue> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let numeric = trimmed.parse::<i64>().ok()?;
    if numeric < -1 {
        return None;
    }

    Some(ResolvedMetricValue {
        value: Some(numeric),
        source: source_name.to_string(),
    })
}

/// Parse a budget literal from a JSON value, supporting both number and string representations.
/// 从 JSON 值中解析预算字面量；支持 number 和 string 两种表示。
fn parse_metric_json_value(target: &Value, source_name: &str) -> Option<ResolvedMetricValue> {
    match target {
        Value::Number(number) => parse_metric_literal(&number.to_string(), source_name),
        Value::String(text) => parse_metric_literal(text, source_name),
        _ => None,
    }
}

/// Traverse a dotted JSON field path such as `tools.truncateToolOutputLines`.
/// 沿点路径读取 JSON 字段，例如 `tools.truncateToolOutputLines`。
fn traverse_dotted_json_field<'a>(value: &'a Value, field_path: &str) -> Option<&'a Value> {
    let mut current = value;
    for field in field_path.split('.') {
        let object = current.as_object()?;
        current = object.get(field)?;
    }
    Some(current)
}

/// Expand `~` into the current user's home directory so config paths remain portable.
/// 展开 `~` 为当前用户 home 目录，便于配置文件路径跨环境复用。
fn expand_user_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Return the current user's home directory in a cross-platform way for Windows and Unix-like systems.
/// 获取当前用户 home 目录，兼容 Windows 与类 Unix 平台。
fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Perform simple `*` / `?` wildcard matching for client-name and tool-name rules.
/// 执行简单的 `*` / `?` 通配符匹配，用于客户端名与工具名规则。
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; text_chars.len() + 1]; pattern_chars.len() + 1];
    dp[0][0] = true;

    for index in 0..pattern_chars.len() {
        if pattern_chars[index] == '*' {
            dp[index + 1][0] = dp[index][0];
        }
    }

    for pattern_index in 0..pattern_chars.len() {
        for text_index in 0..text_chars.len() {
            dp[pattern_index + 1][text_index + 1] = match pattern_chars[pattern_index] {
                '*' => dp[pattern_index][text_index + 1] || dp[pattern_index + 1][text_index],
                '?' => dp[pattern_index][text_index],
                current => dp[pattern_index][text_index] && current == text_chars[text_index],
            };
        }
    }

    dp[pattern_chars.len()][text_chars.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ClientInfo;
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
        let yaml = include_str!("../runtime/configs/client_budgets.yaml");
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
    }

    /// Verify that when a client does not explicitly define file_read, it falls back to the same client's tool_result budget.
    /// 验证当客户端未显式配置 file_read 时，会自动回退复用同客户端的 tool_result 预算。
    #[test]
    fn resolve_client_budget_snapshot_falls_back_file_read_to_tool_result() {
        let yaml = include_str!("../runtime/configs/client_budgets.yaml");
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

        initialize_client_budget_runtime_root(Some(&root))
            .expect("runtime root init should succeed");
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
        initialize_client_budget_runtime_root(Some(&root))
            .expect("runtime root init should succeed");
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
}
