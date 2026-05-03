use serde_json::Value;
use std::path::PathBuf;

use crate::host_core::model::{RuntimeSurfaceSummary, RuntimeToolDescriptor};
use crate::host_core::projections::build_runtime_tools;
use crate::host_core::runtime::HostRuntime;

impl HostRuntime {
    /// Mark the current protocol session as initialized after an adapter receives its initialized notification.
    /// 在适配层收到 initialized notification 后标记当前协议会话已经初始化。
    pub(crate) async fn mark_client_initialized(&self) {
        self.inner.lock().await.initialized = true;
    }

    /// Update the current client session metadata and return a snapshot of exposed runtime capabilities.
    /// 更新当前客户端会话元数据，并返回运行时已暴露能力的快照。
    pub(crate) fn update_client_session(
        &self,
        protocol_version: &str,
        client_capabilities: Value,
    ) -> Result<RuntimeSurfaceSummary, (i64, String)> {
        let mut inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Server is busy".to_string()))?;

        inner.version = Some(protocol_version.to_string());
        inner.client_capabilities = client_capabilities;

        Ok(RuntimeSurfaceSummary {
            has_tools: !inner.host_tools.is_empty() || !inner.skill_tools.is_empty(),
        })
    }

    /// List every runtime tool descriptor for protocol adapters.
    /// 为协议适配器列出全部运行时工具描述。
    pub(crate) fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDescriptor>, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(build_runtime_tools(&inner.host_tools, &inner.skill_tools))
    }

    /// Resolve one registered tool from either the host-owned or LuaSkills-managed registry.
    /// 从宿主自有或 LuaSkills 托管注册表解析单个已注册工具。
    pub(crate) async fn resolve_tool_descriptor(
        &self,
        tool_name: &str,
    ) -> Result<RuntimeToolDescriptor, (i64, String)> {
        let inner = self.inner.lock().await;
        inner
            .host_tools
            .get(tool_name)
            .or_else(|| inner.skill_tools.get(tool_name))
            .cloned()
            .ok_or_else(|| (-32602, format!("Unknown tool: {}", tool_name)))
    }

    /// Return whether a Lua engine is currently configured for dynamic tool dispatch.
    /// 返回当前是否已配置用于动态工具分发的 Lua 引擎。
    pub(crate) fn has_lua_engine(&self) -> bool {
        self.lua_engine.is_some()
    }

    /// Return the resources root used by runtime tool-result template rendering.
    /// 返回运行时工具结果模板渲染使用的资源根目录。
    pub(crate) fn tool_result_template_resources_root(&self) -> Option<PathBuf> {
        self.lua_engine_options
            .as_ref()
            .and_then(|options| options.host_options.resources_dir.clone())
    }
}
