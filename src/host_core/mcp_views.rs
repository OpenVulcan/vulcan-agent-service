use serde_json::Value;
use std::path::PathBuf;

use crate::host_core::projections::{
    build_mcp_completion_value, build_mcp_prompt_get_value, build_mcp_prompts_value,
    build_mcp_resource_read_value, build_mcp_resource_templates_value, build_mcp_resources_value,
    build_mcp_tools_value,
};
use crate::host_core::runtime::HostRuntime;
use crate::transport::mcp::McpDispatcher;
use crate::transport::mcp::protocol::*;

impl HostRuntime {
    /// Handle a single MCP JSON-RPC message through the compatibility dispatcher path.
    /// 通过兼容 dispatcher 路径处理单条 MCP JSON-RPC 消息。
    #[allow(dead_code)]
    pub async fn handle_message(&self, msg: &Value) -> Option<Value> {
        McpDispatcher::new(self.clone()).handle_message(msg).await
    }

    /// Handle a contextual MCP JSON-RPC message through the compatibility dispatcher path.
    /// 通过兼容 dispatcher 路径处理带上下文的 MCP JSON-RPC 消息。
    #[allow(dead_code)]
    pub async fn handle_message_with_context(
        &self,
        msg: &Value,
        request_context: RequestContext,
    ) -> Option<Value> {
        McpDispatcher::new(self.clone())
            .handle_message_with_context(msg, request_context)
            .await
    }

    /// Mark the MCP runtime session initialized after the adapter receives the initialized notification.
    /// 在适配层收到 initialized notification 后标记 MCP 运行时会话已初始化。
    pub(crate) async fn mark_mcp_initialized(&self) {
        self.inner.lock().await.initialized = true;
    }

    /// Build the initialize response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 initialize 响应值。
    pub(crate) fn initialize_mcp_client_value(
        &self,
        params: Option<Value>,
    ) -> Result<Value, (i64, String)> {
        let req: InitializeRequest = serde_json::from_value(params.unwrap_or_default())
            .map_err(|e| (-32602, format!("Invalid initialize params: {}", e)))?;

        let negotiated = negotiate_version(&req.protocol_version).ok_or_else(|| {
            (
                -32602,
                format!(
                    "Unsupported protocol version: {}. Supported: {}, {}",
                    req.protocol_version,
                    PROTOCOL_VERSION_LATEST,
                    PROTOCOL_VERSION_COMPATIBLE.join(", ")
                ),
            )
        })?;

        // Initialize is expected to be short and infrequent, so a non-awaiting lock keeps this runtime value method synchronous.
        // initialize 预期短小且低频，因此使用非等待锁让这个运行时值方法保持同步。

        let mut inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Server is busy".to_string()))?;

        inner.version = Some(negotiated.to_string());
        inner.client_capabilities =
            serde_json::from_value(req.capabilities.clone()).unwrap_or_default();

        let client_name = req
            .client_info
            .as_ref()
            .map(|client| client.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let has_tools = !inner.host_tools.is_empty() || !inner.skill_tools.is_empty();
        let has_resources = !inner.resources.is_empty() || !inner.resource_templates.is_empty();
        let has_prompts = !inner.prompts.is_empty();
        let has_completions =
            has_feature(negotiated, FeatureFlag::Completions) && self.lua_engine.is_some();
        eprintln!("[MCP] Client: {} ({})", client_name, negotiated);
        eprintln!(
            "[MCP] Features: completions={}, streaming={}, tools={}, resources={}, prompts={}",
            has_feature(negotiated, FeatureFlag::Completions),
            has_feature(negotiated, FeatureFlag::Streaming),
            has_tools,
            has_resources,
            has_prompts,
        );

        let result = InitializeResult {
            protocol_version: negotiated.to_string(),
            capabilities: ServerCapabilities {
                tools: if has_tools {
                    Some(ToolCapability {
                        list_changed: Some(false),
                    })
                } else {
                    None
                },
                resources: if has_resources {
                    Some(ResourceCapability {
                        subscribe: Some(false),
                        list_changed: Some(false),
                    })
                } else {
                    None
                },
                prompts: if has_prompts {
                    Some(PromptCapability {
                        list_changed: Some(false),
                    })
                } else {
                    None
                },
                logging: None,
                completions: if has_completions {
                    Some(CompletionsCapability {})
                } else {
                    None
                },
            },
            server_info: ServerInfo {
                name: "vulcan-mcp-client".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "Vulcan MCP server supporting 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05. \
                 When Lua skills are loaded, this server exposes Lua skill provided MCP tools, \
                 prompt completions, and host-wrapped strict help tools. LuaSkills Core resources, \
                 resource templates, and prompts are disabled in strict mode."
                    .to_string(),
            ),
        };

        serde_json::to_value(result).map_err(|e| (-32603, format!("Serialization error: {}", e)))
    }

    /// Build the tools/list response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 tools/list 响应值。
    pub(crate) fn list_mcp_tools_value(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(build_mcp_tools_value(&inner.host_tools, &inner.skill_tools))
    }

    /// Resolve one registered MCP tool from either the host-owned or LuaSkills-managed registry.
    /// 从宿主自有或 LuaSkills 托管注册表解析单个已注册 MCP 工具。
    pub(crate) async fn resolve_mcp_tool(&self, tool_name: &str) -> Result<Tool, (i64, String)> {
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

    /// Return the resources root used by MCP tool-result template rendering.
    /// 返回 MCP 工具结果模板渲染使用的资源根目录。
    pub(crate) fn mcp_template_resources_root(&self) -> Option<PathBuf> {
        self.lua_engine_options
            .as_ref()
            .and_then(|options| options.host_options.resources_dir.clone())
    }

    /// Build the resources/list response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 resources/list 响应值。
    pub(crate) fn list_mcp_resources_value(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(build_mcp_resources_value(&inner.resources))
    }

    /// Build the resources/read response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 resources/read 响应值。
    pub(crate) fn read_mcp_resource_value(
        &self,
        params: Option<Value>,
        _request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        build_mcp_resource_read_value(params)
    }

    /// Build the resources/templates/list response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 resources/templates/list 响应值。
    pub(crate) fn list_mcp_resource_templates_value(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(build_mcp_resource_templates_value(
            &inner.resource_templates,
        ))
    }

    /// Build the prompts/list response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 prompts/list 响应值。
    pub(crate) fn list_mcp_prompts_value(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(build_mcp_prompts_value(&inner.prompts))
    }

    /// Build the prompts/get response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 prompts/get 响应值。
    pub(crate) fn get_mcp_prompt_value(
        &self,
        params: Option<Value>,
        _request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        build_mcp_prompt_get_value(params)
    }

    /// Build the completion/complete response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 completion/complete 响应值。
    pub(crate) fn complete_mcp_argument_value(
        &self,
        params: Option<Value>,
    ) -> Result<Value, (i64, String)> {
        build_mcp_completion_value(params, |ref_name, argument_name| {
            match self.lua_engine.as_ref() {
                Some(engine) => {
                    let engine = engine
                        .read()
                        .map_err(|_| (-32603, "Lua engine lock poisoned".to_string()))?;
                    Ok(engine.prompt_argument_completions(ref_name, argument_name))
                }
                None => Ok(None),
            }
        })
    }

    /// Build a parse error response (no id).
    /// 构建不含请求 id 的解析错误响应。
    pub fn parse_error(message: &str) -> Value {
        McpDispatcher::parse_error(message)
    }
}
