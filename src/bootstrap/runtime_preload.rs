use crate::config;
use crate::config::Config;
use crate::config::client_budget::{
    initialize_client_budget_runtime_root, preload_client_budget_config,
};
use crate::config::model_config::{initialize_model_config_runtime_root, preload_model_config};
use crate::config::tool_config::{initialize_tool_config_runtime_root, preload_tool_configs};
use crate::model_provider::install_luaskills_model_callbacks;
use crate::support::runtime_logging::info as log_info;

use super::runtime_init::resolve_runtime_root_for_host;

/// Format the resolved client-budget preview into startup logs that are easy for humans to read directly.
/// 把客户端预算预解析摘要格式化成人可直接阅读的启动日志。
fn print_client_budget_preload_log(report: &config::client_budget::ClientBudgetLoadReport) {
    for (client_pattern, preview) in &report.resolved_previews {
        let Some(scope_object) = preview.as_object() else {
            continue;
        };
        for (scope_name, scope_value) in scope_object {
            let Some(scope) = scope_value.as_object() else {
                continue;
            };
            let raw_tokens = scope
                .get("raw_tokens")
                .and_then(|value| value.as_i64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string());
            let raw_bytes = scope
                .get("raw_bytes")
                .and_then(|value| value.as_i64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string());
            let raw_lines = scope
                .get("raw_lines")
                .and_then(|value| value.as_i64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string());
            let effective_bytes = scope
                .get("bytes")
                .and_then(|value| value.as_u64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let effective_lines = scope
                .get("lines")
                .and_then(|value| value.as_i64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let source = scope
                .get("source")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            log_info(format!(
                "[client_budget]client={},scope={},tokens={},bytes={},lines={},effective_bytes={},effective_lines={},source={}",
                client_pattern,
                scope_name,
                raw_tokens,
                raw_bytes,
                raw_lines,
                effective_bytes,
                effective_lines,
                source
            ));
        }
    }
}

/// Format the preloaded tool-config summary into startup logs that are directly readable by humans.
/// 将预加载的工具配置摘要格式化为人可直接阅读的启动日志。
fn print_tool_config_preload_log(report: &config::tool_config::ToolConfigLoadReport) {
    if report.tool_count == 0 {
        log_info(format!(
            "[tool_config]loaded tools=0,source={}",
            report.source_path.as_deref().unwrap_or("unavailable")
        ));
        return;
    }
    log_info(format!(
        "[tool_config]loaded tools={},names={},source={}",
        report.tool_count,
        report.tool_names.join(","),
        report.source_path.as_deref().unwrap_or("unavailable")
    ));
}

/// Format the preloaded model-config summary without exposing provider secrets.
/// 把模型配置预加载摘要格式化为日志，同时避免暴露供应商密钥。
fn print_model_config_preload_log(report: &config::model_config::ModelConfigLoadReport) {
    log_info(format!(
        "[model_config]provider={},provider_enabled={},embed={},embed_api_key={},embed_base_url={},llm={},llm_api_key={},llm_base_url={},source={}",
        report.provider,
        report.provider_enabled,
        report.embedding_enabled,
        report.embedding_api_key_configured,
        report.embedding_base_url_configured,
        report.llm_enabled,
        report.llm_api_key_configured,
        report.llm_base_url_configured,
        report.source_path.as_deref().unwrap_or("unavailable")
    ));
}

/// Preload hot-reloadable runtime config files before the host starts so configuration issues surface early.
/// 在宿主启动前预加载可热重载运行时配置，让配置问题尽早暴露。
pub(super) fn preload_runtime_mcp_configs(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let runtime_root = resolve_runtime_root_for_host(cfg)?;
    initialize_client_budget_runtime_root(runtime_root.as_deref())
        .map_err(|error| format!("failed to initialize client-budget runtime root: {error}"))?;
    initialize_tool_config_runtime_root(runtime_root.as_deref())
        .map_err(|error| format!("failed to initialize tool-config runtime root: {error}"))?;
    initialize_model_config_runtime_root(runtime_root.as_deref())
        .map_err(|error| format!("failed to initialize model-config runtime root: {error}"))?;
    let client_budget_report = preload_client_budget_config()
        .map_err(|error| format!("failed to preload client budget config: {error}"))?;
    let tool_config_report = preload_tool_configs()
        .map_err(|error| format!("failed to preload tool configs: {error}"))?;
    let model_config_report = preload_model_config()
        .map_err(|error| format!("failed to preload model configs: {error}"))?;
    // Model callbacks must be refreshed after preload so runtime calls see the latest provider settings.
    // 模型配置预载后必须刷新模型回调，确保运行时调用使用最新供应商设置。
    install_luaskills_model_callbacks();
    print_client_budget_preload_log(&client_budget_report);
    print_tool_config_preload_log(&tool_config_report);
    print_model_config_preload_log(&model_config_report);
    Ok(())
}
