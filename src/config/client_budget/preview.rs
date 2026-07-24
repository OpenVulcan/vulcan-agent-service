use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::resolution::{
    apply_safe_bytes_ratio, default_resolved_metric_value, merge_effective_estimation,
};
use super::{
    BudgetScopesConfig, ClientBudgetConfig, DEFAULT_INLINE_BYTES_LIMIT, EffectiveBudgetEstimation,
};
use crate::config::tool_config::ToolEstimationOverride;

/// Build a preview of the pre-resolved client budgets so startup and reload can print the actual loaded values directly.
/// 构建预解析后的客户端预算摘要，便于启动和热重载时直接输出实际读取值。
pub(super) fn build_resolved_preview_map(config: &ClientBudgetConfig) -> BTreeMap<String, Value> {
    let mut previews = BTreeMap::new();
    // Client-rule previews intentionally exclude request-time skill-specific overrides.
    // 客户端规则预览有意排除请求期的 skill 专用覆盖。
    let default_tool_override = ToolEstimationOverride::default();
    for client_rule in &config.clients {
        let estimation = merge_effective_estimation(
            &config.defaults.estimation,
            Some(&client_rule.estimation),
            &default_tool_override,
        );
        previews.insert(
            client_rule.pattern.clone(),
            build_scope_preview_value(&client_rule.budgets, &estimation),
        );
    }
    for (client_name, client_rule) in &config.grpc_clients {
        let estimation = merge_effective_estimation(
            &config.defaults.estimation,
            Some(&client_rule.estimation),
            &default_tool_override,
        );
        previews.insert(
            format!("grpc:{}", client_name),
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
