use std::collections::HashMap;

use crate::transport::mcp::protocol::{
    ClientCapabilities, Prompt, Resource, ResourceTemplate, Tool,
};
use luaskills::RuntimeEntryDescriptor;

/// Loaded LuaSkill package descriptor exposed to the gRPC LuaSkills surface.
/// 暴露给 gRPC LuaSkills 接口的已加载技能包描述。
#[derive(Debug, Clone)]
pub struct LuaSkillPackageDescriptor {
    /// Skill identifier declared by the LuaSkill package.
    /// LuaSkill 包声明的技能标识。
    pub skill_id: String,
    /// Runtime root name that contributed this effective package.
    /// 提供该生效包的运行时根名称。
    pub root_name: String,
    /// Concrete skill directory path.
    /// 具体技能目录路径。
    pub skill_dir: String,
    /// Canonical dynamic tool names exposed by this package.
    /// 该技能包暴露出的标准动态工具名称。
    pub tool_names: Vec<String>,
}

/// Dynamic LuaSkill tool descriptor with both MCP schema and LuaSkills runtime metadata.
/// 同时包含 MCP schema 与 LuaSkills 运行时元数据的动态工具描述。
#[derive(Debug, Clone)]
pub struct LuaSkillToolDescriptor {
    /// MCP-compatible tool definition used by existing clients.
    /// 现有客户端使用的 MCP 兼容工具定义。
    pub tool: Tool,
    /// Skill identifier that owns the runtime entry.
    /// 拥有该运行时入口的技能标识。
    pub skill_id: String,
    /// Local entry name inside the owning LuaSkill package.
    /// 所属 LuaSkill 包内的本地入口名称。
    pub entry_name: String,
    /// Runtime root name that contributed the entry.
    /// 提供该入口的运行时根名称。
    pub root_name: String,
    /// Concrete skill directory path for diagnostics and inventory views.
    /// 用于诊断和清单展示的具体技能目录路径。
    pub skill_dir: String,
}

/// Mutable host runtime registry state shared behind the HostRuntime mutex.
/// 通过 HostRuntime 互斥锁共享的可变宿主运行时注册表状态。
pub(super) struct ServerInner {
    /// Host-owned MCP tools registered by the current host adapter and never mutated by LuaSkills runtime deltas.
    /// 当前宿主适配层拥有的 MCP 工具注册表，不会被 LuaSkills 运行时差异事件修改。
    pub(super) host_tools: HashMap<String, Tool>,
    /// LuaSkills-managed dynamic MCP tools derived from runtime entries and fully driven by runtime registry deltas.
    /// 由 LuaSkills 运行时入口派生并完全受运行时注册表差异驱动的动态 MCP 工具注册表。
    pub(super) skill_tools: HashMap<String, Tool>,
    /// LuaSkills runtime entry metadata keyed by canonical dynamic tool name.
    /// 按标准动态工具名索引的 LuaSkills 运行时入口元数据。
    pub(super) skill_entries: HashMap<String, RuntimeEntryDescriptor>,
    /// Static MCP resources exposed by host-owned tools and help projections.
    /// 由宿主自有工具与帮助投影暴露的静态 MCP resources。
    pub(super) resources: Vec<Resource>,
    /// Static MCP resource templates exposed by the host runtime.
    /// 宿主运行时暴露的静态 MCP resource templates。
    pub(super) resource_templates: Vec<ResourceTemplate>,
    /// Static MCP prompts exposed by the host runtime.
    /// 宿主运行时暴露的静态 MCP prompts。
    pub(super) prompts: Vec<Prompt>,
    /// Negotiated client protocol version after initialization.
    /// 初始化后协商得到的客户端协议版本。
    pub(super) version: Option<String>,
    /// Whether the MCP initialize handshake has completed.
    /// MCP initialize 握手是否已经完成。
    pub(super) initialized: bool,
    /// Client capability payload captured during initialization.
    /// 初始化期间捕获的客户端能力载荷。
    pub(super) client_capabilities: ClientCapabilities,
}
