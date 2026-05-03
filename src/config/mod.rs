/// Runtime application configuration loading and CLI configuration discovery.
/// 运行时应用配置加载与 CLI 配置发现。
pub mod app_config;
/// Client-specific output budget and estimation configuration.
/// 客户端维度输出预算与估算配置。
pub mod client_budget;
/// Host model provider configuration and validation.
/// 宿主模型供应商配置与校验。
pub mod model_config;
/// Tool-specific runtime configuration loading and lookup.
/// 工具维度运行时配置加载与查询。
pub mod tool_config;

/// Re-export application config types for the stable `crate::config::Config` entrypoint.
/// 为稳定的 `crate::config::Config` 入口重导出应用配置类型。
pub use app_config::*;
