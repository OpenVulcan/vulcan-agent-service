use crate::config::tool_config::resolve_tool_estimation_override;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::types::{EffectiveMetricNumericValue, ResolvedMetricValue};
use super::{
    BudgetConfigSource, BudgetEstimationConfig, BudgetMetricConfig, ClientBudgetRule,
    DEFAULT_BYTES_PER_TOKEN, DEFAULT_INLINE_BYTES_LIMIT, DEFAULT_SAFE_BYTES_RATIO,
    DEFAULT_UNLIMITED_BYTES_CAP, EffectiveBudgetEstimation, EffectiveBudgetScope,
};

/// Match a client-budget rule using simple `*` / `?` wildcard semantics.
/// 匹配客户端预算规则，使用简单的 `*` / `?` 通配符匹配。
pub(super) fn match_client_budget_rule<'a>(
    rules: &'a [ClientBudgetRule],
    client_name: &str,
) -> Option<&'a ClientBudgetRule> {
    rules
        .iter()
        .find(|rule| wildcard_match(&rule.pattern.to_lowercase(), &client_name.to_lowercase()))
}

/// Merge the default estimation config with budget-estimation overrides extracted from tool config.
/// 合并默认估算配置与工具配置中的预算估算覆盖，生成最终估算倍率。
pub(super) fn merge_effective_estimation(
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
pub(super) fn resolve_scope_budget(
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
pub(super) fn apply_safe_bytes_ratio(bytes: u64, ratio: f64) -> u64 {
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
pub(super) fn default_resolved_metric_value(
    metric_config: &BudgetMetricConfig,
) -> ResolvedMetricValue {
    ResolvedMetricValue {
        value: metric_config.default,
        source: "default".to_string(),
    }
}

/// Read a budget value from an external source, supporting env / json / toml.
/// 从外部配置源中读取预算值，支持 env / json / toml。
pub(super) fn read_metric_from_source(source: &BudgetConfigSource) -> Option<ResolvedMetricValue> {
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

/// Parse one textual budget literal. `-1` means explicit unlimited, while non-negative integers mean concrete limits.
/// 解析单个文本字面量预算值；`-1` 代表显式不限，非负整数代表具体额度。
pub(super) fn parse_metric_literal(
    raw_value: &str,
    source_name: &str,
) -> Option<ResolvedMetricValue> {
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
