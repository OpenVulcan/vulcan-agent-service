use crate::host_core::model::{
    RuntimeToolCallRequest, RuntimeToolCallResult, RuntimeToolDescriptor,
};
use crate::support::RuntimeRequestContext;

/// Internal service surface for runtime tool discovery and invocation.
/// 面向运行时工具发现与调用的内部服务能力面。
#[allow(dead_code)]
pub(crate) trait RuntimeToolService {
    /// List every tool currently exposed by the host runtime.
    /// 列出当前宿主运行时暴露的全部工具。
    fn list_tools(&self) -> Result<Vec<RuntimeToolDescriptor>, (i64, String)>;

    /// Invoke one tool through the transport-neutral runtime service surface.
    /// 通过传输无关的运行时服务能力面调用单个工具。
    async fn call_tool(
        &self,
        request: RuntimeToolCallRequest,
        request_context: &RuntimeRequestContext,
    ) -> Result<RuntimeToolCallResult, (i64, String)>;
}

/// Internal service surface for LuaSkills help projection.
/// 面向 LuaSkills 帮助信息投影的内部服务能力面。
#[allow(dead_code)]
pub(crate) trait RuntimeHelpService {
    /// Render the compact help tree list.
    /// 渲染紧凑的帮助树列表。
    fn list_help(&self) -> Result<String, (i64, String)>;

    /// Render one detailed help flow with the transport-provided client identity.
    /// 使用传输层提供的客户端身份渲染单个详细帮助流程。
    async fn get_help(
        &self,
        skill_id: &str,
        flow: &str,
        client_name: &str,
        client_version: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<RuntimeToolCallResult, (i64, String)>;
}

/// Internal service surface for LuaSkills configuration and package administration.
/// 面向 LuaSkills 配置与包管理的内部服务能力面。
#[allow(dead_code)]
pub(crate) trait RuntimeSkillAdminService {
    /// Dispatch one canonical LuaSkills runtime-config JSON request.
    /// 分发一份标准 LuaSkills runtime-config JSON 请求。
    /// Parameter `request_json` is the complete strict upstream request object.
    /// 参数：`request_json` 是完整的上游严格请求对象。
    /// Returns the stable upstream JSON response envelope or a host execution error.
    /// 返回稳定的上游 JSON 响应包络，或宿主执行错误。
    async fn dispatch_runtime_config(&self, request_json: String) -> Result<String, (i64, String)>;

    /// List installed LuaSkills in the mutable runtime layer.
    /// 列出可变运行层中已安装的 LuaSkills。
    fn list_installed_skills(&self) -> Result<String, (i64, String)>;

    /// Install one LuaSkill into the mutable runtime layer.
    /// 将单个 LuaSkill 安装到可变运行层。
    async fn install_skill(
        &self,
        source: String,
        source_type: Option<String>,
    ) -> Result<RuntimeToolCallResult, (i64, String)>;

    /// Update one installed LuaSkill in the mutable runtime layer.
    /// 更新可变运行层中的单个已安装 LuaSkill。
    async fn update_skill(&self, skill_id: String) -> Result<RuntimeToolCallResult, (i64, String)>;

    /// Uninstall one LuaSkill from the mutable runtime layer.
    /// 从可变运行层卸载单个 LuaSkill。
    async fn uninstall_skill(
        &self,
        skill_id: String,
    ) -> Result<RuntimeToolCallResult, (i64, String)>;

    /// Reload hot-reloadable runtime configuration files.
    /// 重新加载支持热重载的运行时配置文件。
    fn reload_runtime_configs(&self) -> Result<String, (i64, String)>;
}

/// Internal service surface for coarse runtime health probes.
/// 面向粗粒度运行时健康探测的内部服务能力面。
#[allow(dead_code)]
pub(crate) trait RuntimeHealthService {
    /// Return the stable internal runtime product name.
    /// 返回稳定的内部运行时产品名称。
    fn runtime_name(&self) -> &'static str;

    /// Return whether the Lua runtime has been initialized.
    /// 返回 Lua 运行时是否已经初始化。
    fn has_lua_runtime(&self) -> bool;

    /// Return whether the VMM backend client has been connected.
    /// 返回 VMM 后端客户端是否已经连接。
    fn has_vmm_backend(&self) -> bool;
}
