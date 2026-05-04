use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Runtime cache state for client budgets, containing the parsed config and its source path.
/// 客户端预算运行时缓存状态，包含已解析配置与来源路径。
#[derive(Debug, Clone, Default)]
pub(super) struct ClientBudgetRuntime {
    pub(super) config: ClientBudgetConfig,
    pub(super) source_path: Option<PathBuf>,
}

/// Client-budget load report used for startup logs and hot-reload return values.
/// 客户端预算加载报告，用于启动日志和热重载返回值。
#[derive(Debug, Clone, Serialize)]
pub struct ClientBudgetLoadReport {
    pub source_path: Option<String>,
    pub client_count: usize,
    pub client_patterns: Vec<String>,
    pub grpc_client_count: usize,
    pub grpc_client_names: Vec<String>,
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
    #[serde(default)]
    pub grpc_clients: BTreeMap<String, ExactClientBudgetRule>,
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

/// Exact gRPC client-budget override activated before shared client-name pattern rules.
/// gRPC 精确客户端预算覆盖规则，会在统一客户端名称 pattern 规则前优先生效。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExactClientBudgetRule {
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
    pub(super) resolved_source_value: Option<ResolvedMetricValue>,
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
pub(super) struct ResolvedMetricValue {
    pub(super) value: Option<i64>,
    pub(super) source: String,
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

/// Numeric form used while deciding one effective budget metric.
/// 决定单个有效预算度量时使用的数值形式。
pub(super) struct EffectiveMetricNumericValue {
    pub(super) metric_value: Option<u64>,
    pub(super) line_value: i64,
}
