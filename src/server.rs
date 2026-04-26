use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::Mutex;

use crate::client_budget::reload_client_budget_config;
use crate::config::Config;
use crate::grpc_client::VmmClient;
use crate::luaskills_host::{
    build_luaskills_engine_options, build_runtime_invocation_context,
    build_runtime_request_context, client_budget_snapshot_for_render,
    install_luaskills_log_callback, map_runtime_entry_to_mcp_tool,
};
use crate::protocol::*;
use crate::temp_maintenance::ensure_runtime_temp_dir;
use crate::tool_config::reload_tool_configs;
use crate::tool_result_format::{HostRenderOptions, render_tool_result_text};
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaVmPoolConfig, RuntimeEntryRegistryDelta, RuntimeHelpDetail,
    RuntimeSkillHelpDescriptor, RuntimeSkillLifecycleCallback, RuntimeSkillLifecycleEvent,
    RuntimeSkillRoot, SkillConfigEntry, ToolCacheConfig, runtime_config_store::SkillConfigStore,
    set_entry_registry_callback, set_skill_lifecycle_callback,
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
    lua_engine: Option<Arc<StdRwLock<LuaEngine>>>,
    lua_engine_options: Option<LuaEngineOptions>,
    lua_skill_roots: Option<Vec<RuntimeSkillRoot>>,
    runtime_skill_config_file_path: Option<PathBuf>,
}

/// Return whether one tool name belongs to the host-owned MCP tool surface.
/// 返回某个工具名是否属于宿主自有的 MCP 工具面。
pub fn is_host_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "vulcan-help-list" | "vulcan-help-detail" | "reload_vulcan_mcp_configs" | "luaskill-config"
    )
}

/// Return whether one host-owned MCP tool requires a ready Lua engine to succeed.
/// 返回某个宿主自有 MCP 工具在执行时是否依赖已就绪的 Lua 引擎。
pub fn host_tool_requires_lua_engine(tool_name: &str) -> bool {
    matches!(tool_name, "vulcan-help-list" | "vulcan-help-detail")
}

struct ServerInner {
    /// Host-owned MCP tools registered by the current host adapter and never mutated by LuaSkills runtime deltas.
    /// 当前宿主适配层拥有的 MCP 工具注册表，不会被 LuaSkills 运行时差异事件修改。
    host_tools: HashMap<String, Tool>,
    /// LuaSkills-managed dynamic MCP tools derived from runtime entries and fully driven by runtime registry deltas.
    /// 由 LuaSkills 运行时入口派生并完全受运行时注册表差异驱动的动态 MCP 工具注册表。
    skill_tools: HashMap<String, Tool>,
    resources: Vec<Resource>,
    resource_templates: Vec<ResourceTemplate>,
    prompts: Vec<Prompt>,
    version: Option<String>,
    initialized: bool,
    client_capabilities: ClientCapabilities,
}

/// Supported actions for the host-owned unified luaskill-config tool.
/// 宿主自有统一 luaskill-config 工具支持的动作集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RuntimeConfigAction {
    /// List config entries, optionally scoped to one skill namespace.
    /// 列出配置项，可选地限制到单个技能命名空间。
    List,
    /// Read one concrete config value by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 读取单个配置值。
    Get,
    /// Insert or replace one concrete config value by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 写入或替换单个配置值。
    Set,
    /// Delete one concrete config key by `(skill_id, key)`.
    /// 通过 `(skill_id, key)` 删除单个配置键。
    Delete,
}

impl RuntimeConfigAction {
    /// Render one stable action name used by logs and user-facing tool output.
    /// 渲染供日志与面向用户的工具输出使用的稳定动作名称。
    fn as_str(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Get => "get",
            Self::Set => "set",
            Self::Delete => "delete",
        }
    }
}

/// Parsed arguments for one host-owned unified luaskill-config tool call.
/// 一次宿主统一 luaskill-config 工具调用解析后的参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct RuntimeConfigToolArguments {
    /// Action selector that chooses one of `list/get/set/delete`.
    /// 动作选择器，用于决定 `list/get/set/delete` 中的哪一种。
    action: RuntimeConfigAction,
    /// Optional target skill namespace used by `list`, and required by `get/set/delete`.
    /// `list` 可选使用、`get/set/delete` 必填的目标技能命名空间。
    skill_id: Option<String>,
    /// Optional config key used by `get/set/delete`.
    /// `get/set/delete` 使用的可选配置键。
    key: Option<String>,
    /// Optional string config value used by `set`.
    /// `set` 使用的可选字符串配置值。
    value: Option<String>,
}

impl McpServer {
    pub fn new() -> Self {
        let inner = ServerInner {
            host_tools: HashMap::new(),
            skill_tools: HashMap::new(),
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
            lua_engine_options: None,
            lua_skill_roots: None,
            runtime_skill_config_file_path: None,
        };
        server.register_defaults();
        server
    }

    /// Configure the explicit unified skill-config file path used by the host-owned luaskill-config tool.
    /// 配置宿主自有 luaskill-config 工具使用的显式统一 Skill 配置文件路径。
    pub fn with_runtime_skill_config_file_path(mut self, file_path: PathBuf) -> Self {
        self.runtime_skill_config_file_path = Some(file_path);
        self.register_runtime_config_tool();
        self
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
        config: &Config,
        skill_roots: &[RuntimeSkillRoot],
        pool_config: LuaVmPoolConfig,
        cache_config: ToolCacheConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        install_luaskills_log_callback();
        let engine_options = build_luaskills_engine_options(config, pool_config, cache_config)?;
        let mut engine = LuaEngine::new(engine_options.clone())?;
        engine.load_from_roots(skill_roots)?;
        let entries = engine.list_entries();
        eprintln!("[MCP] {} Lua skills loaded", entries.len());
        let engine = Arc::new(StdRwLock::new(engine));
        self.lua_engine = Some(engine.clone());
        self.lua_engine_options = Some(engine_options);
        self.lua_skill_roots = Some(skill_roots.to_vec());

        let callback_inner = self.inner.clone();
        set_entry_registry_callback(Some(Arc::new(move |delta: &RuntimeEntryRegistryDelta| {
            let mut inner = callback_inner.blocking_lock();
            apply_runtime_entry_registry_delta(&mut inner, delta);
        })));
        let lifecycle_callback: RuntimeSkillLifecycleCallback = Arc::new(
            |event: &RuntimeSkillLifecycleEvent| {
                eprintln!(
                    "[LuaSkills:lifecycle] plane={:?} action={:?} skill={} root={} dir={} status={} message={}",
                    event.plane,
                    event.action,
                    event.skill_id,
                    event.root_name.as_deref().unwrap_or(""),
                    event.skill_dir.as_deref().unwrap_or(""),
                    event.status,
                    event.message.as_deref().unwrap_or("")
                );
            },
        );
        set_skill_lifecycle_callback(Some(lifecycle_callback));
        self.register_lua_help_tools();

        // Register Lua skills strictly as MCP tools.
        // 严格仅将 Lua skills 注册为 MCP tools。
        {
            let mut inner = self.inner.try_lock().unwrap();
            for entry in entries {
                let tool = map_runtime_entry_to_mcp_tool(&entry);
                insert_skill_tool(&mut inner, tool);
            }
        }

        Ok(self)
    }

    fn register_defaults(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- reload_vulcan_mcp_configs: hot reload runtime client budget / tool config files ---
        inner.host_tools.insert(
            "reload_vulcan_mcp_configs".to_string(),
            Tool::with_annotations(
                "reload_vulcan_mcp_configs",
                "Reload hot-reloadable Vulcan MCP runtime config files. This refreshes client_budgets.yaml and tool_configs.yaml, but does not reload config.yaml or restart-bound transport settings. Use this only when the user explicitly asks to reload runtime configs; do not call it proactively during normal tool execution.",
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

    /// Register the host-owned luaskill-config tool after one effective unified config file path becomes available.
    /// 在生效的统一配置文件路径可用后注册宿主自有的 luaskill-config 工具。
    fn register_runtime_config_tool(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- luaskill-config: inspect or mutate the host-managed unified runtime skill config ---
        inner.host_tools.insert(
            "luaskill-config".to_string(),
            Tool::with_annotations(
                "luaskill-config",
                "Inspect or mutate the host-managed unified Lua skill configuration. Supports `list`, `get`, `set`, and `delete` across skill namespaces, and returns AI-friendly text instead of raw JSON. Only call this tool when the user explicitly asks to inspect or modify LuaSkill configuration. For `set` and `delete`, report the affected `skill_id`/`key` and the final tool result back to the user.",
                json!({
                    "action": {
                        "type": "string",
                        "description": "Operation to perform. Supported values: `list`, `get`, `set`, `delete`.",
                        "enum": ["list", "get", "set", "delete"]
                    },
                    "skill_id": {
                        "type": "string",
                        "description": "Target skill id. Optional for `list` to filter one namespace, required for `get`, `set`, and `delete`."
                    },
                    "key": {
                        "type": "string",
                        "description": "Config key. Required for `get`, `set`, and `delete`."
                    },
                    "value": {
                        "type": "string",
                        "description": "String config value. Required for `set`."
                    }
                }),
                vec!["action".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(false),
                },
            ),
        );
    }

    /// Register the host-wrapped Lua help tools only after the Lua engine has been configured successfully.
    /// 仅在 Lua 引擎成功完成配置后再注册宿主包装的 Lua help 工具。
    fn register_lua_help_tools(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- vulcan-help-list: list strict LuaSkills help trees for host-side help wrappers ---
        inner.host_tools.insert(
            "vulcan-help-list".to_string(),
            Tool::with_annotations(
                "vulcan-help-list",
                "List all registered strict LuaSkills help trees and their available flow descriptions. This MCP wrapper renders compact AI-facing Markdown from lib/system structured help data.",
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
        inner.host_tools.insert(
            "vulcan-help-detail".to_string(),
            Tool::with_annotations(
                "vulcan-help-detail",
                "Read one strict LuaSkills help flow from lib/system help data and render it as Markdown for MCP clients. Use flow=`main` to read the skill package description node.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit` or `vulcan-lua`."},
                    "flow": {"type": "string", "description": "Help flow name. Use `main` for the skill package description node, or pass one declared workflow/topic name."}
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
    }

    /// Resolve the default Lua engine used by the MCP host product surface.
    /// 解析 MCP 宿主产品面固定使用的默认 Lua 引擎。
    fn resolve_lua_engine_for_environment(
        &self,
    ) -> Result<Arc<StdRwLock<LuaEngine>>, (i64, String)> {
        self.lua_engine.clone().ok_or_else(|| {
            (
                -32603,
                "Lua engine not configured. Add skills directory.".to_string(),
            )
        })
    }

    /// Resolve the default Lua engine together with the effective default skill-root chain.
    /// 解析默认 Lua 引擎及其对应的默认技能根目录链。
    fn resolve_lua_runtime_target(
        &self,
    ) -> Result<(Arc<StdRwLock<LuaEngine>>, Vec<RuntimeSkillRoot>), (i64, String)> {
        let engine = self.lua_engine.as_ref().ok_or_else(|| {
            (
                -32603,
                "Lua engine not configured. Add skills directory.".to_string(),
            )
        })?;
        let skill_roots = self
            .lua_skill_roots
            .as_ref()
            .ok_or_else(|| (-32603, "Lua skill roots are not configured.".to_string()))?;
        Ok((engine.clone(), skill_roots.clone()))
    }

    /// Build one standalone skill-config store for the host-owned luaskill-config tool.
    /// 为宿主自有 luaskill-config 工具构造一份独立 Skill 配置存储。
    fn resolve_runtime_skill_config_store(&self) -> Result<SkillConfigStore, (i64, String)> {
        let file_path = self
            .runtime_skill_config_file_path
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                (
                    -32603,
                    "luaskill-config is unavailable because no runtime_root could be resolved."
                        .to_string(),
                )
            })?;
        let store = SkillConfigStore::new(Some(file_path.clone())).map_err(|error| {
            (
                -32603,
                format!("failed to initialize luaskill-config store: {}", error),
            )
        })?;
        Ok(store)
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

    fn handle_tools_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        let mut merged = std::collections::BTreeMap::new();
        for (name, tool) in &inner.skill_tools {
            merged.insert(name.clone(), tool.clone());
        }
        for (name, tool) in &inner.host_tools {
            merged.insert(name.clone(), tool.clone());
        }
        let tools: Vec<Tool> = merged.into_values().collect();
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
            .host_tools
            .get(&req.name)
            .or_else(|| inner.skill_tools.get(&req.name))
            .ok_or_else(|| (-32602, format!("Unknown tool: {}", req.name)))?
            .clone();
        drop(inner);

        let args = req.arguments.unwrap_or_default();
        let result = match tool.name.as_str() {
            "vulcan-help-list" => {
                let engine = self.resolve_lua_engine_for_environment()?;
                let help_tree = engine
                    .read()
                    .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
                    .list_skill_help();
                let markdown = render_help_list_markdown(&help_tree);
                ToolCallResult {
                    content: vec![TextContent::text(&markdown)],
                    is_error: None,
                }
            }

            "vulcan-help-detail" => {
                let engine = self.resolve_lua_engine_for_environment()?;
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
                let flow =
                    flow.ok_or_else(|| (-32602, "Missing required parameter: flow".to_string()))?;
                let engine_clone = engine.clone();
                let request_context = request_context.clone();
                let runtime_request_context = build_runtime_request_context(&request_context);
                let result = tokio::task::spawn_blocking(move || {
                    let engine = engine_clone
                        .read()
                        .map_err(|_| "Lua engine lock poisoned".to_string())?;
                    engine.render_skill_help_detail(
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

            "luaskill-config" => {
                let store = self.resolve_runtime_skill_config_store()?;
                let request = parse_runtime_config_tool_arguments(&args)?;
                let rendered = execute_runtime_config_tool(&store, &request)?;

                ToolCallResult {
                    content: vec![TextContent::text(&rendered)],
                    is_error: None,
                }
            }

            _ => {
                // Check if this is a Lua skill
                if self.lua_engine.is_some() {
                    let (target_engine, target_skill_roots) = self.resolve_lua_runtime_target()?;
                    let is_skill = target_engine
                        .read()
                        .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
                        .is_skill(&tool.name);
                    if is_skill {
                        let engine_clone = target_engine.clone();
                        let tool_name = tool.name.clone();
                        let skill_name = target_engine
                            .read()
                            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
                            .skill_name_for_tool(&tool.name);
                        let args_clone = args.clone();
                        let request_context = request_context.clone();
                        let budget_request_context = request_context.clone();
                        let invocation_context = build_runtime_invocation_context(
                            Some(&request_context),
                            Some(&tool_name),
                            skill_name.as_deref(),
                        );
                        let result = tokio::task::spawn_blocking(move || {
                            let engine = engine_clone
                                .read()
                                .map_err(|_| "Lua engine lock poisoned".to_string())?;
                            engine.call_skill(&tool_name, &args_clone, Some(&invocation_context))
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
                                        (
                                            -32603,
                                            format!("resolve runtime spill dir failed: {}", error),
                                        )
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
                                            template_skill_roots: target_skill_roots
                                                .iter()
                                                .map(|root| root.skills_dir.clone())
                                                .collect(),
                                            template_resources_root: self
                                                .lua_engine_options
                                                .as_ref()
                                                .and_then(|options| {
                                                    options.host_options.resources_dir.clone()
                                                }),
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
            match self.lua_engine.as_ref() {
                Some(engine) => {
                    let engine = engine
                        .read()
                        .map_err(|_| (-32603, "Lua engine lock poisoned".to_string()))?;
                    engine.prompt_argument_completions(ref_name, argument_name)
                }
                None => None,
            }
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
        let main_description = skill_help.main.description.trim();
        if main_description.is_empty() {
            lines.push("- `main`: skill package description".to_string());
        } else {
            lines.push(format!(
                "- `main`: skill package description. {}",
                main_description
            ));
        }
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

/// Parse one luaskill-config argument payload into the strongly typed host-tool request model.
/// 把一份 luaskill-config 参数载荷解析为强类型宿主工具请求模型。
fn parse_runtime_config_tool_arguments(
    args: &Value,
) -> Result<RuntimeConfigToolArguments, (i64, String)> {
    serde_json::from_value(args.clone()).map_err(|error| {
        (
            -32602,
            format!("Invalid luaskill-config arguments: {}", error),
        )
    })
}

/// Normalize one optional luaskill-config string field by trimming whitespace and dropping blanks.
/// 规范化一项可选 luaskill-config 字符串字段：去除首尾空白并丢弃空串。
fn normalize_optional_runtime_config_field(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Require one non-empty luaskill-config identifier field such as `skill_id` or `key`.
/// 要求一项非空的 luaskill-config 标识字段，例如 `skill_id` 或 `key`。
fn require_runtime_config_identifier_field(
    value: Option<&str>,
    field_name: &str,
    action: &RuntimeConfigAction,
) -> Result<String, (i64, String)> {
    normalize_optional_runtime_config_field(value).ok_or_else(|| {
        (
            -32602,
            format!(
                "luaskill-config action '{}' requires a non-empty parameter: {}",
                action.as_str(),
                field_name
            ),
        )
    })
}

/// Require one raw luaskill-config value field and preserve empty-string payloads for explicit writes.
/// 要求提供一项原始 luaskill-config 值字段，并保留空字符串这种显式写入载荷。
fn require_runtime_config_value_field(
    value: Option<&str>,
    action: &RuntimeConfigAction,
) -> Result<String, (i64, String)> {
    value.map(str::to_string).ok_or_else(|| {
        (
            -32602,
            format!(
                "luaskill-config action '{}' requires parameter: value",
                action.as_str()
            ),
        )
    })
}

/// Group flattened skill-config entries into one stable nested skill-to-key-value mapping.
/// 把扁平化 Skill 配置记录分组为稳定的“技能 -> 键值”嵌套映射。
fn group_runtime_config_entries(
    entries: &[SkillConfigEntry],
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut grouped = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(entry.skill_id.clone())
            .or_insert_with(BTreeMap::new)
            .insert(entry.key.clone(), entry.value.clone());
    }
    grouped
}

/// Render one config string value into a readable single-line literal for AI-oriented text output.
/// 把一个配置字符串渲染成适合面向 AI 文本输出的单行字面量。
fn render_runtime_config_value(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Render one grouped skill-config map as stable plain text without exposing the backing file path.
/// 把分组后的 Skill 配置映射渲染为稳定纯文本，且不暴露底层文件路径。
fn render_grouped_runtime_config_entries(
    grouped_entries: &BTreeMap<String, BTreeMap<String, String>>,
) -> String {
    let mut lines = Vec::new();
    for (index, (skill_id, values)) in grouped_entries.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(format!("skill_id: {}", skill_id));
        for (key, value) in values {
            lines.push(format!(
                "- {} = {}",
                key,
                render_runtime_config_value(value)
            ));
        }
    }
    lines.join("\n")
}

/// Execute one host-owned luaskill-config action against the standalone unified skill-config store.
/// 对独立统一 Skill 配置存储执行一次宿主自有 luaskill-config 动作。
fn execute_runtime_config_tool(
    store: &SkillConfigStore,
    request: &RuntimeConfigToolArguments,
) -> Result<String, (i64, String)> {
    match &request.action {
        RuntimeConfigAction::List => {
            let requested_skill_id =
                normalize_optional_runtime_config_field(request.skill_id.as_deref());
            let entries = store
                .list_entries(requested_skill_id.as_deref())
                .map_err(|error| (-32603, format!("luaskill-config list failed: {}", error)))?;
            let grouped_entries = group_runtime_config_entries(&entries);
            if grouped_entries.is_empty() {
                return Ok(requested_skill_id
                    .map(|skill_id| format!("No configuration is set for skill `{}`.", skill_id))
                    .unwrap_or_else(|| "No luaskill configuration is currently set.".to_string()));
            }

            let header = requested_skill_id
                .as_deref()
                .map(|skill_id| format!("Configuration for skill `{}`:", skill_id))
                .unwrap_or_else(|| {
                    format!(
                        "Found {} luaskill configuration namespaces:",
                        grouped_entries.len()
                    )
                });
            Ok(format!(
                "{}\n\n{}",
                header,
                render_grouped_runtime_config_entries(&grouped_entries)
            ))
        }
        RuntimeConfigAction::Get => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let value = store
                .get_value(&skill_id, &key)
                .map_err(|error| (-32603, format!("luaskill-config get failed: {}", error)))?;
            Ok(match value {
                Some(value) => format!(
                    "Configuration found.\n\nskill_id: {}\n- {} = {}",
                    skill_id,
                    key,
                    render_runtime_config_value(&value)
                ),
                None => format!(
                    "Configuration key `{}` does not exist under skill `{}`.",
                    key, skill_id
                ),
            })
        }
        RuntimeConfigAction::Set => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let value =
                require_runtime_config_value_field(request.value.as_deref(), &request.action)?;
            store
                .set_value(&skill_id, &key, &value)
                .map_err(|error| (-32603, format!("luaskill-config set failed: {}", error)))?;
            Ok(format!(
                "Configuration updated.\n\nskill_id: {}\n- {} = {}",
                skill_id,
                key,
                render_runtime_config_value(&value)
            ))
        }
        RuntimeConfigAction::Delete => {
            let skill_id = require_runtime_config_identifier_field(
                request.skill_id.as_deref(),
                "skill_id",
                &request.action,
            )?;
            let key = require_runtime_config_identifier_field(
                request.key.as_deref(),
                "key",
                &request.action,
            )?;
            let deleted = store
                .delete_value(&skill_id, &key)
                .map_err(|error| (-32603, format!("luaskill-config delete failed: {}", error)))?;
            Ok(if deleted {
                format!(
                    "Configuration key `{}` was deleted from skill `{}`.",
                    key, skill_id
                )
            } else {
                format!(
                    "Configuration key `{}` does not exist under skill `{}`, so nothing was deleted.",
                    key, skill_id
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::RuntimeHelpNodeDescriptor;
    use std::collections::HashSet;

    /// Build one unique temporary directory path for one server-module test case.
    /// 为 server 模块单个测试用例构建唯一的临时目录路径。
    fn unique_test_dir(name: &str) -> PathBuf {
        let unique = format!(
            "vulcan-mcp-server-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    fn make_help_descriptor() -> RuntimeSkillHelpDescriptor {
        RuntimeSkillHelpDescriptor {
            skill_id: "demo-skill".to_string(),
            skill_name: "Demo Skill".to_string(),
            skill_version: "1.2.3".to_string(),
            root_name: "ROOT".to_string(),
            skill_dir: "D:/runtime/skills/demo-skill".to_string(),
            main: RuntimeHelpNodeDescriptor {
                flow_name: "main".to_string(),
                description: "Summarize the package-level capability surface.".to_string(),
                related_entries: vec![],
                is_main: true,
            },
            flows: vec![RuntimeHelpNodeDescriptor {
                flow_name: "search".to_string(),
                description: "Search indexed project files.".to_string(),
                related_entries: vec![],
                is_main: false,
            }],
        }
    }

    #[test]
    fn render_help_list_markdown_omits_runtime_metadata_fields() {
        let markdown = render_help_list_markdown(&[make_help_descriptor()]);

        assert!(markdown.contains("## `demo-skill`"));
        assert!(markdown.contains("- `main`: skill package description."));
        assert!(!markdown.contains("version:"));
        assert!(!markdown.contains("root:"));
        assert!(!markdown.contains("dir:"));
    }

    #[test]
    fn render_help_list_markdown_labels_main_as_package_description() {
        let markdown = render_help_list_markdown(&[make_help_descriptor()]);

        assert!(markdown.contains(
            "- `main`: skill package description. Summarize the package-level capability surface."
        ));
        assert!(markdown.contains("- `search`: Search indexed project files."));
    }

    /// Minimal servers without a Lua engine should expose only engine-independent host tools in `tools/list`.
    /// 未加载 Lua 引擎的最小服务在 `tools/list` 中应只暴露与引擎无关的宿主工具。
    #[test]
    fn tools_list_hides_help_tools_when_lua_engine_is_unavailable() {
        let server = McpServer::new();
        let response = server
            .handle_tools_list()
            .expect("tools/list should succeed on minimal server");
        let tool_names: HashSet<String> = response
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array should exist")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();

        assert!(tool_names.contains("reload_vulcan_mcp_configs"));
        assert!(!tool_names.contains("luaskill-config"));
        assert!(!tool_names.contains("vulcan-help-list"));
        assert!(!tool_names.contains("vulcan-help-detail"));
    }

    /// Servers with one resolved runtime skill-config file path should expose luaskill-config even before Lua engine initialization.
    /// 具备已解析统一 Skill 配置文件路径的服务，即使尚未初始化 Lua 引擎，也应暴露 luaskill-config。
    #[test]
    fn tools_list_exposes_luaskill_config_when_host_path_is_available() {
        let root = unique_test_dir("luaskill-config-tools-list");
        let config_file_path = root.join("configs").join("skill_config.json");
        let server = McpServer::new().with_runtime_skill_config_file_path(config_file_path);
        let response = server
            .handle_tools_list()
            .expect("tools/list should succeed after luaskill-config registration");
        let tool_names: HashSet<String> = response
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array should exist")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();

        assert!(tool_names.contains("luaskill-config"));
    }

    /// Lua help tools should become visible only after the Lua runtime capability has been registered explicitly.
    /// Lua help 工具只应在显式注册了 Lua 运行时能力后才对外可见。
    #[test]
    fn register_lua_help_tools_exposes_help_tools_after_runtime_ready() {
        let mut server = McpServer::new();
        server.register_lua_help_tools();
        let response = server
            .handle_tools_list()
            .expect("tools/list should succeed after help registration");
        let tool_names: HashSet<String> = response
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array should exist")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();

        assert!(tool_names.contains("vulcan-help-list"));
        assert!(tool_names.contains("vulcan-help-detail"));
    }

    /// Luaskill-config should remain callable without any Lua engine because it now uses the standalone skill-config store directly.
    /// luaskill-config 现在直接使用独立 Skill 配置存储，因此在没有 Lua 引擎时也应可调用。
    #[test]
    fn luaskill_config_tool_works_without_lua_engine() {
        let root = unique_test_dir("luaskill-config-without-engine");
        let config_file_path = root.join("configs").join("skill_config.json");
        let server = McpServer::new().with_runtime_skill_config_file_path(config_file_path.clone());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        let set_response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "set",
                            "skill_id": "demo-skill",
                            "key": "api_token",
                            "value": "sk-runtime"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("luaskill-config set should produce one response");
        assert!(
            set_response.get("error").is_none(),
            "unexpected luaskill-config error: {set_response}"
        );

        let persisted: Value = serde_json::from_str(
            &std::fs::read_to_string(&config_file_path)
                .expect("luaskill-config file should be created"),
        )
        .expect("persisted luaskill-config JSON should parse");
        assert_eq!(persisted["skills"]["demo-skill"]["api_token"], "sk-runtime");

        let get_response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "get",
                            "skill_id": "demo-skill",
                            "key": "api_token"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("luaskill-config get should produce one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            get_response
                .get("result")
                .cloned()
                .expect("luaskill-config get should return a result"),
        )
        .expect("luaskill-config get result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert!(rendered.contains("skill_id: demo-skill"));
        assert!(rendered.contains("- api_token = \"sk-runtime\""));
        assert!(!rendered.contains("```json"));
        assert!(!rendered.contains("skill_config.json"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Empty luaskill-config listings should explicitly report that no configuration exists yet.
    /// 空的 luaskill-config 列表结果应明确提示当前还没有任何配置。
    #[test]
    fn luaskill_config_list_reports_empty_state() {
        let root = unique_test_dir("luaskill-config-empty-list");
        let config_file_path = root.join("configs").join("skill_config.json");
        let server = McpServer::new().with_runtime_skill_config_file_path(config_file_path);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("luaskill-config list should produce one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .expect("luaskill-config list should return a result"),
        )
        .expect("luaskill-config list result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert_eq!(rendered, "No luaskill configuration is currently set.");
        assert!(!rendered.contains("skill_config.json"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Non-empty luaskill-config listings should group entries by skill id and show stable key-value lines.
    /// 非空的 luaskill-config 列表结果应按 skill_id 分组并稳定展示键值行。
    #[test]
    fn luaskill_config_list_groups_entries_by_skill_id() {
        let root = unique_test_dir("luaskill-config-grouped-list");
        let config_file_path = root.join("configs").join("skill_config.json");
        let server = McpServer::new().with_runtime_skill_config_file_path(config_file_path);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime should build");

        for (skill_id, key, value) in [
            ("alpha-skill", "endpoint", "https://api.example.com"),
            ("alpha-skill", "token", "sk-alpha"),
            ("beta-skill", "region", "cn-sh"),
        ] {
            runtime
                .block_on(server.handle_message_with_context(
                    &json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/call",
                        "params": {
                            "name": "luaskill-config",
                            "arguments": {
                                "action": "set",
                                "skill_id": skill_id,
                                "key": key,
                                "value": value
                            }
                        }
                    }),
                    RequestContext::default(),
                ))
                .expect("luaskill-config set should succeed for grouped list setup");
        }

        let response = runtime
            .block_on(server.handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("luaskill-config list should produce one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .expect("luaskill-config grouped list should return a result"),
        )
        .expect("luaskill-config grouped list result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert!(rendered.contains("Found 2 luaskill configuration namespaces:"));
        assert!(rendered.contains("skill_id: alpha-skill"));
        assert!(rendered.contains("- endpoint = \"https://api.example.com\""));
        assert!(rendered.contains("- token = \"sk-alpha\""));
        assert!(rendered.contains("skill_id: beta-skill"));
        assert!(rendered.contains("- region = \"cn-sh\""));
        assert!(!rendered.contains("```json"));
        assert!(!rendered.contains("skill_config.json"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Apply one runtime entry-registry delta to the MCP host tool registry.
/// 把一份运行时入口注册表差异应用到 MCP 宿主工具注册表。
fn apply_runtime_entry_registry_delta(inner: &mut ServerInner, delta: &RuntimeEntryRegistryDelta) {
    for removed_name in &delta.removed_entry_names {
        inner.skill_tools.remove(removed_name);
    }
    for entry in &delta.updated_entries {
        let tool = map_runtime_entry_to_mcp_tool(entry);
        insert_skill_tool(inner, tool);
    }
    for entry in &delta.added_entries {
        let tool = map_runtime_entry_to_mcp_tool(entry);
        insert_skill_tool(inner, tool);
    }
}

/// Insert one LuaSkills-managed tool into the dynamic registry while rejecting host-reserved name collisions.
/// 将单个 LuaSkills 动态工具插入动态注册表，并拒绝与宿主保留名称发生冲突。
fn insert_skill_tool(inner: &mut ServerInner, tool: Tool) {
    if inner.host_tools.contains_key(&tool.name) {
        eprintln!(
            "[LuaSkills] Skip dynamic tool '{}' because it collides with a host-owned tool",
            tool.name
        );
        inner.skill_tools.remove(&tool.name);
        return;
    }
    inner.skill_tools.insert(tool.name.clone(), tool);
}

/// Render one structured help detail payload into user-facing Markdown.
/// 把一份结构化帮助详情载荷渲染成面向用户的 Markdown 文本。
fn render_help_detail_markdown(detail: &RuntimeHelpDetail) -> String {
    detail.content.clone()
}
