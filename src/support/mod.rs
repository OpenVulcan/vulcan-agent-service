/// Shared transport-neutral runtime request context models.
/// 共享的传输无关运行时请求上下文模型。
pub mod runtime_context;
/// Lightweight host runtime logging helpers.
/// 轻量级宿主运行时日志辅助函数。
pub mod runtime_logging;
/// Runtime temporary directory resolution and cleanup helpers.
/// 运行时临时目录解析与清理辅助函数。
pub mod temp_maintenance;
/// Tool result rendering, overflow, and template helpers.
/// 工具结果渲染、溢出处理与模板辅助函数。
pub mod tool_result_format;

pub use runtime_context::{RuntimeClientInfo, RuntimeRequestContext};
