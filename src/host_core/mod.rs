pub(crate) mod host_tools;
pub(crate) mod lifecycle;
pub(crate) mod luaskills_api;
pub(crate) mod mcp_views;
pub(crate) mod projections;
pub mod runtime;
pub(crate) mod runtime_config_tool;
pub(crate) mod service_impls;
pub(crate) mod services;
pub(crate) mod skill_manager;
pub(crate) mod skill_tools;
pub(crate) mod state;
pub(crate) mod tool_dispatch;

pub use host_tools::{host_tool_requires_lua_engine, is_host_tool_name};
pub use runtime::HostRuntime;
pub use state::{LuaSkillPackageDescriptor, LuaSkillToolDescriptor};

/// MCP-compatible alias retained while transports migrate to host runtime service traits.
/// 在传输层迁移到宿主运行时服务接口期间保留的 MCP 兼容别名。
pub type McpServer = HostRuntime;
