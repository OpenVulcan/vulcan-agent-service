use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::client_budget::reload_client_budget_config;
use crate::grpc_client::VmmClient;
use crate::luaskills_host::{
    build_luaskills_engine_options, build_runtime_invocation_context, build_runtime_request_context,
    client_budget_snapshot_for_render, install_luaskills_log_callback, map_runtime_entry_to_mcp_tool,
};
use crate::protocol::*;
use crate::temp_maintenance::ensure_runtime_temp_dir;
use crate::tool_config::reload_tool_configs;
use crate::tool_result_format::{HostRenderOptions, render_tool_result_text};
use vulcan_luaskills::{
    LuaEngine, LuaVmPoolConfig, RuntimeHelpDetail, RuntimeSkillHelpDescriptor, ToolCacheConfig,
};

// ============================================================
// Shared MCP Server state
// ============================================================

#[derive(Clone)]
pub struct McpServer {
    inner: Arc<Mutex<ServerInner>>,
    // gRPC clients stored outside the mutex — they are Clone and do not
    // require exclusive access, so extracting them for tool calls no longer
    // blocks on other concurrent operations (tools/list, initialize, etc).
    #[allow(dead_code)] // reserved for VMM forwarding mode
    vmm: Option<VmmClient>,
    lua_engine: Option<Arc<LuaEngine>>,
}

struct ServerInner {
    tools: HashMap<String, Tool>,
    resources: Vec<Resource>,
    resource_templates: Vec<ResourceTemplate>,
    prompts: Vec<Prompt>,
    version: Option<String>,
    initialized: bool,
    client_capabilities: ClientCapabilities,
}

impl McpServer {
    pub fn new() -> Self {
        let inner = ServerInner {
            tools: HashMap::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
            version: None,
            initialized: false,
            client_capabilities: ClientCapabilities::default(),
        };
        let mut server = Self {
            inner: Arc::new(Mutex::new(inner)),
            vmm: None,
            lua_engine: None,
        };
        server.register_defaults();
        server
    }

    /// Configure the VMM (VulcanMemoryMesh) gRPC client endpoint.
    /// VMM is connected but NOT exposed as MCP tools — it will be invoked through a separate mechanism.
    pub async fn with_vmm(self, endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = VmmClient::connect(endpoint).await?;
        eprintln!("[MCP] VMM client connected: {}", endpoint);
        Ok(Self {
            vmm: Some(client),
            ..self
        })
    }

    /// Configure Lua skills from system and override directories.
    pub fn with_lua_skills(
        mut self,
        base_dir: &std::path::Path,
        override_dir: Option<&std::path::Path>,
        pool_config: LuaVmPoolConfig,
        cache_config: ToolCacheConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        install_luaskills_log_callback();
        let mut engine = LuaEngine::new(build_luaskills_engine_options(pool_config, cache_config)?)?;
        engine.load_from_dirs(base_dir, override_dir)?;
        let entries = engine.list_entries();
        eprintln!("[MCP] {} Lua skills loaded", entries.len());
        self.lua_engine = Some(Arc::new(engine));

        // Register Lua skills strictly as MCP tools.
        // 严格仅将 Lua skills 注册为 MCP tools。
        {
            let mut inner = self.inner.try_lock().unwrap();
            for entry in entries {
                let tool = map_runtime_entry_to_mcp_tool(&entry);
                inner.tools.insert(tool.name.clone(), tool);
            }
        }

        Ok(self)
    }

    fn register_defaults(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- vulcan-help-list: list strict LuaSkills help trees for host-side help wrappers ---
        inner.tools.insert(
            "vulcan-help-list".to_string(),
            Tool::with_annotations(
                "vulcan-help-list",
                "List all registered strict LuaSkills help trees and their available flow descriptions. This MCP wrapper renders host-side Markdown from lib/system structured help data.",
                json!({}),
                vec![],
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        // --- vulcan-help-detail: read one strict LuaSkills help flow and render it for MCP ---
        inner.tools.insert(
            "vulcan-help-detail".to_string(),
            Tool::with_annotations(
                "vulcan-help-detail",
                "Read one strict LuaSkills help flow from lib/system help data and render it as Markdown for MCP clients. Use flow=`main` to read the skill main help node.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit` or `vulcan-runtime`."},
                    "flow": {"type": "string", "description": "Help flow name. Use `main` for the skill main help node, or pass one declared workflow/topic name."}
                }),
                vec!["skill".to_string(), "flow".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        // --- reload_vulcan_mcp_configs: hot reload runtime client budget / tool config files ---
        inner.tools.insert(
            "reload_vulcan_mcp_configs".to_string(),
            Tool::with_annotations(
                "reload_vulcan_mcp_configs",
                "Reload hot-reloadable Vulcan MCP runtime config files. This refreshes client_budgets.yaml and tool_configs.yaml, but does not reload config.yaml or restart-bound transport settings. Use this only when the user explicitly asks to reload runtime configs; do not call it proactively during normal tool execution. / 热重载 Vulcan MCP 的运行时配置文件。当前会刷新 client_budgets.yaml 与 tool_configs.yaml，但不会重载 config.yaml 或需要重启才能生效的传输配置。仅在用户明确要求重载运行时配置时使用，常规工具执行过程中不要主动调用。",
                json!({}),
                vec![],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );
    }

    /// Handle a single JSON-RPC message and return the JSON response (if any).
    /// Thread-safe: can be called from any transport.
    pub async fn handle_message(&self, msg: &Value) -> Option<Value> {
        self.handle_message_with_context(msg, RequestContext::default())
            .await
    }

    /// Handle a JSON-RPC message with request-scoped client context.
    /// 使用请求级客户端上下文处理 JSON-RPC 消息。
    pub async fn handle_message_with_context(
        &self,
        msg: &Value,
        request_context: RequestContext,
    ) -> Option<Value> {
        // Batch request (array)
        if let Some(batch) = msg.as_array() {
            let mut responses = Vec::new();
            for item in batch {
                if let Some(resp) = self.handle_single(item, request_context.clone()).await {
                    responses.push(resp);
                }
            }
            if !responses.is_empty() {
                return Some(Value::Array(responses));
            }
            return None;
        }

        self.handle_single(msg, request_context).await
    }

    async fn handle_single(&self, msg: &Value, request_context: RequestContext) -> Option<Value> {
        if let Some(id) = msg.get("id").cloned() {
            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
            let params = msg.get("params").cloned();
            let result = self.handle_request(method, params, request_context).await;
            match result {
                Ok(value) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": value
                })),
                Err((code, message)) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": code,
                        "message": message
                    }
                })),
            }
        } else if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
            let params = msg.get("params").cloned();
            self.handle_notification(method, params, request_context)
                .await;
            None
        } else {
            None
        }
    }

    async fn handle_request(
        &self,
        method: &str,
        params: Option<Value>,
        request_context: RequestContext,
    ) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => self.handle_initialize(params),
            "ping" => Ok(json!({})),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(params, &request_context).await,
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resources_read(params, &request_context),
            "resources/templates/list" => self.handle_resource_templates_list(),
            "prompts/list" => self.handle_prompts_list(),
            "prompts/get" => self.handle_prompts_get(params, &request_context),
            "completion/complete" => self.handle_completion(params),
            _ => {
                eprintln!("[MCP] Unknown method: {}", method);
                Err((-32601, format!("Method not found: {}", method)))
            }
        }
    }

    async fn handle_notification(
        &self,
        method: &str,
        params: Option<Value>,
        _request_context: RequestContext,
    ) {
        match method {
            "notifications/initialized" => {
                self.inner.lock().await.initialized = true;
                eprintln!("[MCP] Client initialized");
            }
            "notifications/cancelled" => {
                if let Some(p) = params {
                    let cancel: Result<CancellationNotification, _> = serde_json::from_value(p);
                    if let Ok(c) = cancel {
                        eprintln!(
                            "[MCP] Request cancelled: {:?}, reason: {:?}",
                            c.request_id, c.reason
                        );
                    }
                }
            }
            "notifications/roots/list_changed" => {
                eprintln!("[MCP] Roots list changed notification received");
            }
            _ => {
                eprintln!("[MCP] Unknown notification: {}", method);
            }
        }
    }

    // ----------------------------------------------------------
    // Handlers (all take &self, lock inner state as needed)
    // ----------------------------------------------------------

    fn handle_initialize(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
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

        // Note: we use try_lock here since this is called from a sync context
        // by handle_request. In HTTP context, handle_request is async and
        // we'll use lock().await.
        // Actually handle_request is async so let me fix this.
        // We need to make handle_initialize async or use try_lock.
        // Let's just use the synchronous path since initialize is called once.

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
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let has_tools = !inner.tools.is_empty();
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
                 By default this server exposes Lua skill provided MCP tools, prompt \
                 completions, and host-wrapped strict help tools. LuaSkills Core resources, \
                 resource templates, and prompts are disabled in strict mode."
                    .to_string(),
            ),
        };

        serde_json::to_value(result).map_err(|e| (-32603, format!("Serialization error: {}", e)))
    }

    fn handle_tools_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        let tools: Vec<Tool> = inner.tools.values().cloned().collect();
        Ok(json!({ "tools": tools }))
    }

    async fn handle_tools_call(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let req: ToolCallRequest = serde_json::from_value(params.unwrap_or_default())
            .map_err(|e| (-32602, format!("Invalid tools/call params: {}", e)))?;

        let inner = self.inner.lock().await;
        let tool = inner
            .tools
            .get(&req.name)
            .ok_or_else(|| (-32602, format!("Unknown tool: {}", req.name)))?
            .clone();
        drop(inner);

        let args = req.arguments.unwrap_or_default();
        let result = match tool.name.as_str() {
            "vulcan-help-list" => {
                let engine = self.lua_engine.as_ref().ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured. Add lua_skills directory.".to_string(),
                    )
                })?;
                let help_tree = engine.list_skill_help();
                let markdown = render_help_list_markdown(&help_tree);
                ToolCallResult {
                    content: vec![TextContent::text(&markdown)],
                    is_error: None,
                }
            }

            "vulcan-help-detail" => {
                let engine = self.lua_engine.as_ref().ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured. Add lua_skills directory.".to_string(),
                    )
                })?;
                let skill_id = args
                    .get("skill")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| (-32602, "Missing required parameter: skill".to_string()))?
                    .to_string();
                let flow = args
                    .get("flow")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let flow = flow.ok_or_else(|| {
                    (-32602, "Missing required parameter: flow".to_string())
                })?;
                let engine_clone = engine.clone();
                let request_context = request_context.clone();
                let runtime_request_context = build_runtime_request_context(&request_context);
                let result = tokio::task::spawn_blocking(move || {
                    engine_clone.render_skill_help_detail(
                        &skill_id,
                        &flow,
                        Some(&runtime_request_context),
                    )
                })
                .await
                .map_err(|error| (-32603, format!("vulcan-help-detail spawn error: {}", error)))?;

                match result {
                    Ok(Some(detail)) => ToolCallResult {
                        content: vec![TextContent::text(&render_help_detail_markdown(&detail))],
                        is_error: None,
                    },
                    Ok(None) => ToolCallResult {
                        content: vec![TextContent::text("Skill help not found.")],
                        is_error: Some(true),
                    },
                    Err(error) => ToolCallResult {
                        content: vec![TextContent::text(&error)],
                        is_error: Some(true),
                    },
                }
            }

            "reload_vulcan_mcp_configs" => {
                let client_budget_report = reload_client_budget_config().map_err(|error| {
                    (-32603, format!("reload client budgets failed: {}", error))
                })?;
                let tool_config_report = reload_tool_configs()
                    .map_err(|error| (-32603, format!("reload tool configs failed: {}", error)))?;

                let reload_message = format!(
                    "Runtime MCP configs reloaded successfully.\n- client_budgets: patterns={}, source={}\n- tool_configs: tools={}, source={}\n- config.yaml: not reloaded",
                    client_budget_report.client_count,
                    client_budget_report
                        .source_path
                        .as_deref()
                        .unwrap_or("unavailable"),
                    tool_config_report.tool_count,
                    tool_config_report
                        .source_path
                        .as_deref()
                        .unwrap_or("unavailable")
                );

                ToolCallResult {
                    content: vec![TextContent::text(&reload_message)],
                    is_error: None,
                }
            }

            _ => {
                // Check if this is a Lua skill
                if let Some(engine) = &self.lua_engine {
                    if engine.is_skill(&tool.name) {
                        let engine_clone = engine.clone();
                        let tool_name = tool.name.clone();
                        let skill_name = engine.skill_name_for_tool(&tool.name);
                        let args_clone = args.clone();
                        let request_context = request_context.clone();
                        let budget_request_context = request_context.clone();
                        let invocation_context = build_runtime_invocation_context(
                            Some(&request_context),
                            Some(&tool_name),
                            skill_name.as_deref(),
                        );
                        let result = tokio::task::spawn_blocking(move || {
                            engine_clone.call_skill(&tool_name, &args_clone, Some(&invocation_context))
                        })
                        .await
                        .map_err(|e| (-32603, format!("Lua skill spawn error: {}", e)))?;
                        match result {
                            Ok(val) => {
                                let client_budget = client_budget_snapshot_for_render(
                                    Some(&budget_request_context),
                                    Some(&tool.name),
                                    skill_name.as_deref(),
                                );
                                let spill_root = ensure_runtime_temp_dir()
                                    .map_err(|error| {
                                        (-32603, format!(
                                            "resolve runtime spill dir failed: {}",
                                            error
                                        ))
                                    })?
                                    .join("mcp")
                                    .join("cache");
                                ToolCallResult {
                                    content: vec![TextContent::text(&render_tool_result_text(
                                        &val,
                                        skill_name.as_deref(),
                                        Some(&client_budget),
                                        &HostRenderOptions {
                                            spill_root: Some(spill_root),
                                        },
                                    ))],
                                    is_error: None,
                                }
                            }
                            Err(e) => ToolCallResult {
                                content: vec![TextContent::text(&e)],
                                is_error: Some(true),
                            },
                        }
                    } else {
                        return Err((-32603, format!("Tool not implemented: {}", tool.name)));
                    }
                } else {
                    return Err((-32603, format!("Tool not implemented: {}", tool.name)));
                }
            }
        };

        serde_json::to_value(result).map_err(|e| (-32603, format!("Serialization error: {}", e)))
    }

    fn handle_resources_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(json!({ "resources": inner.resources }))
    }

    fn handle_resources_read(
        &self,
        params: Option<Value>,
        _request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let uri = params
            .and_then(|p| p.get("uri").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| (-32602, "Missing required parameter: uri".to_string()))?;

        Err((-32602, format!("Resource not found: {}", uri)))
    }

    fn handle_resource_templates_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(json!({ "resourceTemplates": inner.resource_templates }))
    }

    fn handle_prompts_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(json!({ "prompts": inner.prompts }))
    }

    fn handle_prompts_get(
        &self,
        params: Option<Value>,
        _request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let params = params.unwrap_or_default();
        let name = params
            .get("name")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| (-32602, "Missing required parameter: name".to_string()))?;

        Err((-32602, format!("Prompt not found: {}", name)))
    }

    fn handle_completion(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
        let params = params.unwrap_or_default();

        let ref_type = params
            .get("ref")
            .and_then(|r| r.get("type"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| (-32602, "Missing ref.type".to_string()))?;

        let argument_name = params
            .get("argument")
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let argument_value = params
            .get("argument")
            .and_then(|a| a.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let ref_name = params
            .get("ref")
            .and_then(|r| r.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let prompt_completion_values = if ref_type == "ref/prompt" {
            self.lua_engine
                .as_ref()
                .and_then(|engine| engine.prompt_argument_completions(ref_name, argument_name))
        } else {
            None
        };

        let values: Vec<String> = match (ref_type, argument_name) {
            ("ref/prompt", _) if prompt_completion_values.is_some() => prompt_completion_values
                .unwrap_or_default()
                .into_iter()
                .filter(|value| {
                    if argument_value.is_empty() {
                        true
                    } else {
                        value
                            .to_ascii_lowercase()
                            .contains(&argument_value.to_ascii_lowercase())
                    }
                })
                .collect(),
            _ => vec![],
        };

        Ok(json!({
            "completion": {
                "values": values,
                "total": values.len() as u32,
                "hasMore": false
            }
        }))
    }

    /// Build a parse error response (no id).
    pub fn parse_error(message: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": message
            }
        })
    }
}

/// Render one structured help list payload into user-facing Markdown.
/// 把一份结构化帮助列表载荷渲染成面向用户的 Markdown 文本。
fn render_help_list_markdown(help_tree: &[RuntimeSkillHelpDescriptor]) -> String {
    if help_tree.is_empty() {
        return "# Vulcan Help List\n\nNo help trees are currently registered.".to_string();
    }

    let mut lines = vec!["# Vulcan Help List".to_string(), String::new()];
    for skill_help in help_tree {
        lines.push(format!("## `{}`", skill_help.skill_id));
        if !skill_help.main.description.trim().is_empty() {
            lines.push(skill_help.main.description.trim().to_string());
        }
        lines.push("- `main`".to_string());
        for flow in &skill_help.flows {
            if flow.description.trim().is_empty() {
                lines.push(format!("- `{}`", flow.flow_name));
            } else {
                lines.push(format!(
                    "- `{}`: {}",
                    flow.flow_name,
                    flow.description.trim()
                ));
            }
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Render one structured help detail payload into user-facing Markdown.
/// 把一份结构化帮助详情载荷渲染成面向用户的 Markdown 文本。
fn render_help_detail_markdown(detail: &RuntimeHelpDetail) -> String {
    detail.content.clone()
}
