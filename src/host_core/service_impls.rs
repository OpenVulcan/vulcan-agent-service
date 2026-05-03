use serde_json::Value;

use crate::host_core::runtime::HostRuntime;
use crate::host_core::services::{
    RuntimeHealthService, RuntimeHelpService, RuntimeSkillAdminService, RuntimeToolService,
};
use crate::transport::mcp::protocol::{RequestContext, ToolCallResult};

impl RuntimeToolService for HostRuntime {
    /// List every host and LuaSkills tool through the internal runtime service trait.
    /// 通过内部运行时服务接口列出全部宿主工具与 LuaSkills 工具。
    fn list_tools(&self) -> Result<Value, (i64, String)> {
        self.list_mcp_tools_value()
    }

    /// Invoke one tool through the internal runtime service trait.
    /// 通过内部运行时服务接口调用单个工具。
    async fn call_tool(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        self.call_mcp_tool_value(params, request_context).await
    }
}

impl RuntimeHelpService for HostRuntime {
    /// Render the LuaSkills help list through the internal runtime service trait.
    /// 通过内部运行时服务接口渲染 LuaSkills 帮助列表。
    fn list_help(&self) -> Result<String, (i64, String)> {
        self.list_luaskill_help()
    }

    /// Render one LuaSkills help flow through the internal runtime service trait.
    /// 通过内部运行时服务接口渲染单个 LuaSkills 帮助流程。
    async fn get_help(
        &self,
        skill_id: &str,
        flow: &str,
        client_name: &str,
        client_version: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<ToolCallResult, (i64, String)> {
        self.get_luaskill_help(skill_id, flow, client_name, client_version, request_id)
            .await
    }
}

impl RuntimeSkillAdminService for HostRuntime {
    /// List LuaSkill configuration through the internal runtime service trait.
    /// 通过内部运行时服务接口列出 LuaSkill 配置。
    fn list_skill_config(&self, skill_id: Option<String>) -> Result<String, (i64, String)> {
        self.list_luaskill_config(skill_id)
    }

    /// Read LuaSkill configuration through the internal runtime service trait.
    /// 通过内部运行时服务接口读取 LuaSkill 配置。
    fn get_skill_config(&self, skill_id: String, key: String) -> Result<String, (i64, String)> {
        self.get_luaskill_config(skill_id, key)
    }

    /// Write LuaSkill configuration through the internal runtime service trait.
    /// 通过内部运行时服务接口写入 LuaSkill 配置。
    fn set_skill_config(
        &self,
        skill_id: String,
        key: String,
        value: String,
    ) -> Result<String, (i64, String)> {
        self.set_luaskill_config(skill_id, key, value)
    }

    /// Delete LuaSkill configuration through the internal runtime service trait.
    /// 通过内部运行时服务接口删除 LuaSkill 配置。
    fn delete_skill_config(&self, skill_id: String, key: String) -> Result<String, (i64, String)> {
        self.delete_luaskill_config(skill_id, key)
    }

    /// List installed LuaSkills through the internal runtime service trait.
    /// 通过内部运行时服务接口列出已安装 LuaSkills。
    fn list_installed_skills(&self) -> Result<String, (i64, String)> {
        self.list_installed_luaskills()
    }

    /// Install one LuaSkill through the internal runtime service trait.
    /// 通过内部运行时服务接口安装单个 LuaSkill。
    async fn install_skill(
        &self,
        source: String,
        source_type: Option<String>,
    ) -> Result<ToolCallResult, (i64, String)> {
        self.install_luaskill(source, source_type).await
    }

    /// Update one LuaSkill through the internal runtime service trait.
    /// 通过内部运行时服务接口更新单个 LuaSkill。
    async fn update_skill(&self, skill_id: String) -> Result<ToolCallResult, (i64, String)> {
        self.update_luaskill(skill_id).await
    }

    /// Uninstall one LuaSkill through the internal runtime service trait.
    /// 通过内部运行时服务接口卸载单个 LuaSkill。
    async fn uninstall_skill(&self, skill_id: String) -> Result<ToolCallResult, (i64, String)> {
        self.uninstall_luaskill(skill_id).await
    }

    /// Reload runtime configs through the internal runtime service trait.
    /// 通过内部运行时服务接口重载运行时配置。
    fn reload_runtime_configs(&self) -> Result<String, (i64, String)> {
        self.reload_luaskill_runtime_configs()
    }
}

impl RuntimeHealthService for HostRuntime {
    /// Return the runtime product name through the internal runtime service trait.
    /// 通过内部运行时服务接口返回运行时产品名称。
    fn runtime_name(&self) -> &'static str {
        "vulcan-host"
    }

    /// Report whether Lua runtime is ready through the internal runtime service trait.
    /// 通过内部运行时服务接口报告 Lua 运行时是否就绪。
    fn has_lua_runtime(&self) -> bool {
        self.lua_engine.is_some()
    }

    /// Report whether the VMM backend is connected through the internal runtime service trait.
    /// 通过内部运行时服务接口报告 VMM 后端是否已连接。
    fn has_vmm_backend(&self) -> bool {
        self.vmm.is_some()
    }
}
