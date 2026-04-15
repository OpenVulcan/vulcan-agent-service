use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::grpc_client::{LanceDbClient, ScratchpadItem, ScratchpadStore, SqliteClient, VmmClient};
use crate::lua_engine::{LuaEngine, LuaVmPoolConfig};
use crate::protocol::*;

// ============================================================
// Built-in tool handlers
// ============================================================

/// 中文：将 Lua/JSON 返回值格式化为 MCP 文本内容；基础标量原样输出，数组和对象按 JSON 输出。
/// English: Format a Lua/JSON result into MCP text content; emit scalar values verbatim and serialize arrays/objects as JSON.
fn format_json_value_for_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn tool_add(args: &Value) -> ToolCallResult {
    let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
    ToolCallResult {
        content: vec![TextContent::text(&format!("{}", a + b))],
        is_error: None,
    }
}

fn tool_greet(args: &Value) -> ToolCallResult {
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
    ToolCallResult {
        content: vec![TextContent::text(&format!("Hello, {}!", name))],
        is_error: None,
    }
}

fn tool_time(_args: &Value) -> ToolCallResult {
    ToolCallResult {
        content: vec![TextContent::text(&utc_now())],
        is_error: None,
    }
}

fn utc_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let remaining = secs % 86400;
    let h = remaining / 3600;
    let m = (remaining % 3600) / 60;
    let s = remaining % 60;
    format!("Unix epoch day {}, {:02}:{:02}:{:02} UTC", days, h, m, s)
}

// ============================================================
// Shared MCP Server state
// ============================================================

#[derive(Clone)]
pub struct McpServer {
    inner: Arc<Mutex<ServerInner>>,
    // gRPC clients stored outside the mutex — they are Clone and do not
    // require exclusive access, so extracting them for tool calls no longer
    // blocks on other concurrent operations (tools/list, initialize, etc).
    lancedb: Option<LanceDbClient>,
    sqlite: Option<SqliteClient>,
    #[allow(dead_code)] // reserved for VMM forwarding mode
    vmm: Option<VmmClient>,
    scratchpad: Option<ScratchpadStore>,
    lua_engine: Option<Arc<LuaEngine>>,
}

struct ServerInner {
    tools: HashMap<String, Tool>,
    resources: Vec<Resource>,
    resource_templates: Vec<ResourceTemplate>,
    resource_data: HashMap<String, String>,
    prompts: Vec<Prompt>,
    version: Option<String>,
    initialized: bool,
    client_capabilities: ClientCapabilities,
    roots: Vec<Root>,
    log_level: String,
}

impl McpServer {
    pub fn new() -> Self {
        let inner = ServerInner {
            tools: HashMap::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            resource_data: HashMap::new(),
            prompts: Vec::new(),
            version: None,
            initialized: false,
            client_capabilities: ClientCapabilities::default(),
            roots: Vec::new(),
            log_level: "info".to_string(),
        };
        let mut server = Self {
            inner: Arc::new(Mutex::new(inner)),
            lancedb: None,
            sqlite: None,
            vmm: None,
            scratchpad: None,
            lua_engine: None,
        };
        server.register_defaults();
        server
    }

    /// Configure the LanceDb gRPC client endpoint.
    pub async fn with_lancedb(self, endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = LanceDbClient::connect(endpoint).await?;
        eprintln!("[MCP] LanceDb client connected: {}", endpoint);
        Ok(Self {
            lancedb: Some(client),
            ..self
        })
    }

    /// Configure the Sqlite gRPC client endpoint.
    pub async fn with_sqlite(self, endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = SqliteClient::connect(endpoint).await?;
        eprintln!("[MCP] Sqlite client connected: {}", endpoint);
        Ok(Self {
            sqlite: Some(client),
            ..self
        })
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

    /// Configure the scratchpad store using the vldb_sqlite gRPC endpoint.
    /// Requires --sqlite to be set (scratchpad stores data in SQLite with vmcp_ prefix).
    pub async fn with_scratchpad_from_sqlite(self) -> Result<Self, Box<dyn std::error::Error>> {
        let sqlite = self
            .sqlite
            .clone()
            .ok_or("Scratchpad requires --sqlite endpoint to be set")?;
        let store = ScratchpadStore::create(sqlite).await?;
        eprintln!("[MCP] Scratchpad store initialized (vmcp_ tables via SQLite)");
        Ok(Self {
            scratchpad: Some(store),
            ..self
        })
    }

    /// Configure Lua skills from system and override directories.
    pub fn with_lua_skills(
        mut self,
        base_dir: &std::path::Path,
        override_dir: Option<&std::path::Path>,
        pool_config: LuaVmPoolConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut engine = LuaEngine::new(pool_config)?;
        engine.load_from_dirs(base_dir, override_dir)?;
        let skills = engine.list_skills();
        let resources = engine.list_resources();
        let resource_templates = engine.list_resource_templates();
        let prompts = engine.list_prompts();
        eprintln!("[MCP] {} Lua skills loaded", skills.len());
        self.lua_engine = Some(Arc::new(engine));

        // Register Lua skills as MCP tools/resources/prompts.
        {
            let mut inner = self.inner.try_lock().unwrap();
            for tool in skills {
                inner.tools.insert(tool.name.clone(), tool);
            }
            inner.resources.extend(resources);
            inner.resource_templates.extend(resource_templates);
            inner.prompts.extend(prompts);
        }

        Ok(self)
    }

    fn register_defaults(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- Tools ---
        let annotations = ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        };
        inner.tools.insert(
            "add".to_string(),
            Tool::with_annotations(
                "add",
                "Add two numbers together",
                json!({
                    "a": {"type": "number", "description": "First operand"},
                    "b": {"type": "number", "description": "Second operand"}
                }),
                vec!["a".to_string(), "b".to_string()],
                annotations.clone(),
            ),
        );
        inner.tools.insert(
            "greet".to_string(),
            Tool::with_annotations(
                "greet",
                "Greet someone by name",
                json!({
                    "name": {"type": "string", "description": "Name of the person to greet"}
                }),
                vec!["name".to_string()],
                annotations.clone(),
            ),
        );
        inner.tools.insert(
            "current_time".to_string(),
            Tool::with_annotations(
                "current_time",
                "Get the current UTC time",
                json!({}),
                vec![],
                annotations,
            ),
        );

        // --- Resources ---
        inner.resources.push(Resource {
            uri: "info://server".to_string(),
            name: "Server Info".to_string(),
            description: Some("Basic server information".to_string()),
            mime_type: Some("text/plain".to_string()),
            size: None,
        });
        inner.resource_data.insert(
            "info://server".to_string(),
            "Minimal MCP server supporting protocol versions 2025-11-25 (primary), \
             2025-06-18, 2025-03-26, and 2024-11-05."
                .to_string(),
        );

        inner.resources.push(Resource {
            uri: "info://protocol".to_string(),
            name: "Protocol Version".to_string(),
            description: Some("MCP protocol version in use".to_string()),
            mime_type: Some("text/plain".to_string()),
            size: None,
        });
        inner.resource_data.insert(
            "info://protocol".to_string(),
            "Latest: 2025-11-25. Compatible: 2025-06-18, 2025-03-26, 2024-11-05".to_string(),
        );

        // --- Resource Templates (2025-03-26+) ---
        inner.resource_templates.push(ResourceTemplate {
            uri_template: "echo://{message}".to_string(),
            name: "Echo".to_string(),
            description: Some("Echo back a message as a resource".to_string()),
            mime_type: Some("text/plain".to_string()),
        });

        // --- Prompts ---
        inner.prompts.push(Prompt {
            name: "code_review".to_string(),
            description: Some("Generate a code review prompt".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "language".to_string(),
                description: Some("Programming language".to_string()),
                required: Some(true),
            }]),
        });
        inner.prompts.push(Prompt {
            name: "explain_code".to_string(),
            description: Some("Ask for a code explanation".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "language".to_string(),
                description: Some("Programming language".to_string()),
                required: Some(false),
            }]),
        });

        // --- Roots ---
        inner.roots.push(Root {
            uri: "file:///workspace".to_string(),
            name: Some("Workspace".to_string()),
        });

        // --- LanceDb gRPC tools ---
        let db_annotations = ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(false),
        };
        inner.tools.insert(
            "lancedb_create_table".to_string(),
            Tool::with_annotations(
                "lancedb_create_table",
                "Create a LanceDb table with specified columns",
                json!({
                    "table_name": {"type": "string", "description": "Name of the table to create"},
                    "columns": {"type": "array", "description": "Column definitions (array of {name, column_type, vector_dim, nullable})"},
                    "overwrite": {"type": "boolean", "description": "Overwrite if table exists"}
                }),
                vec!["table_name".to_string(), "columns".to_string()],
                db_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "lancedb_upsert".to_string(),
            Tool::with_annotations(
                "lancedb_upsert",
                "Upsert data into a LanceDb table (JSON rows or Arrow IPC)",
                json!({
                    "table_name": {"type": "string", "description": "Target table name"},
                    "input_format": {"type": "string", "enum": ["json_rows", "arrow_ipc"], "description": "Data format"},
                    "data": {"type": "string", "description": "JSON array string or base64-encoded Arrow IPC data"},
                    "key_columns": {"type": "array", "description": "Columns to use as upsert keys"}
                }),
                vec!["table_name".to_string(), "input_format".to_string(), "data".to_string()],
                db_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "lancedb_search".to_string(),
            Tool::with_annotations(
                "lancedb_search",
                "Vector search on a LanceDb table",
                json!({
                    "table_name": {"type": "string", "description": "Target table name"},
                    "vector": {"type": "array", "description": "Search vector (array of floats)"},
                    "limit": {"type": "number", "description": "Max results (default 10)"},
                    "filter": {"type": "string", "description": "SQL filter expression"},
                    "vector_column": {"type": "string", "description": "Name of the vector column"},
                    "output_format": {"type": "string", "enum": ["json_rows", "arrow_ipc"], "description": "Output format"}
                }),
                vec!["table_name".to_string(), "vector".to_string()],
                ToolAnnotations { read_only_hint: Some(true), ..db_annotations.clone() },
            ),
        );
        inner.tools.insert(
            "lancedb_delete".to_string(),
            Tool::with_annotations(
                "lancedb_delete",
                "Delete rows from a LanceDb table",
                json!({
                    "table_name": {"type": "string", "description": "Target table name"},
                    "condition": {"type": "string", "description": "SQL WHERE condition"}
                }),
                vec!["table_name".to_string(), "condition".to_string()],
                db_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "lancedb_drop_table".to_string(),
            Tool::with_annotations(
                "lancedb_drop_table",
                "Drop a LanceDb table",
                json!({
                    "table_name": {"type": "string", "description": "Table to drop"}
                }),
                vec!["table_name".to_string()],
                db_annotations.clone(),
            ),
        );

        // --- Sqlite gRPC tools ---
        let sql_annotations = ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(false),
        };
        inner.tools.insert(
            "sqlite_execute".to_string(),
            Tool::with_annotations(
                "sqlite_execute",
                "Execute a single SQL statement on the SQLite database",
                json!({
                    "sql": {"type": "string", "description": "SQL statement"},
                    "params": {"type": "array", "description": "Parameter values (supports int, float, string, bool, null)"}
                }),
                vec!["sql".to_string()],
                ToolAnnotations { read_only_hint: Some(true), ..sql_annotations.clone() },
            ),
        );
        inner.tools.insert(
            "sqlite_execute_batch".to_string(),
            Tool::with_annotations(
                "sqlite_execute_batch",
                "Execute a batch of parameterized SQL statements",
                json!({
                    "sql": {"type": "string", "description": "SQL statement with placeholders"},
                    "items": {"type": "array", "description": "Array of parameter arrays, one per execution"}
                }),
                vec!["sql".to_string(), "items".to_string()],
                sql_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "sqlite_query".to_string(),
            Tool::with_annotations(
                "sqlite_query",
                "Execute a SQL query and return results as JSON or Arrow IPC",
                json!({
                    "sql": {"type": "string", "description": "SQL SELECT query"},
                    "params": {"type": "array", "description": "Parameter values"},
                    "output": {"type": "string", "enum": ["json", "arrow"], "description": "Output format (default: json)"}
                }),
                vec!["sql".to_string()],
                ToolAnnotations { read_only_hint: Some(true), ..sql_annotations },
            ),
        );

        // --- Scratchpad (DWM working memory via SQLite, vmcp_ tables) ---
        let sp_annotations = ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(false),
        };
        inner.tools.insert(
            "vmcp_scratchpad_upsert".to_string(),
            Tool::with_annotations(
                "vmcp_scratchpad_upsert",
                "Write deterministic working-memory key/value anchors into the isolated DWM scratchpad (stored in SQLite with vmcp_ prefix). Supports single key/value or batch items.",
                json!({
                    "project_id": {"type": "number", "description": "Project ID"},
                    "user_id": {"type": "number", "description": "User ID"},
                    "session_id": {"type": "string", "description": "Session key"},
                    "plan_name": {"type": "string", "description": "Canonical plan name (max 128 chars)"},
                    "key": {"type": "string", "description": "Single key (use if no items array)"},
                    "value": {"type": "string", "description": "Single value (use if no items array)"},
                    "items": {"type": "array", "description": "Batch of {key, value} items (max 32)"}
                }),
                vec!["project_id".to_string(), "user_id".to_string(), "session_id".to_string(), "plan_name".to_string()],
                sp_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "vmcp_scratchpad_delete".to_string(),
            Tool::with_annotations(
                "vmcp_scratchpad_delete",
                "Remove one or more keys from the isolated DWM scratchpad",
                json!({
                    "project_id": {"type": "number", "description": "Project ID"},
                    "user_id": {"type": "number", "description": "User ID"},
                    "session_id": {"type": "string", "description": "Session key"},
                    "plan_name": {"type": "string", "description": "Canonical plan name"},
                    "key": {"type": "string", "description": "Single key to delete"},
                    "keys": {"type": "array", "description": "Array of keys to delete"}
                }),
                vec![
                    "project_id".to_string(),
                    "user_id".to_string(),
                    "session_id".to_string(),
                    "plan_name".to_string(),
                ],
                sp_annotations.clone(),
            ),
        );
        inner.tools.insert(
            "vmcp_scratchpad_get".to_string(),
            Tool::with_annotations(
                "vmcp_scratchpad_get",
                "Read key/value anchors from the isolated DWM scratchpad (all or filtered by keys)",
                json!({
                    "project_id": {"type": "number", "description": "Project ID"},
                    "user_id": {"type": "number", "description": "User ID"},
                    "session_id": {"type": "string", "description": "Session key"},
                    "keys": {"type": "array", "description": "Optional array of keys to filter (empty = all)"}
                }),
                vec!["project_id".to_string(), "user_id".to_string(), "session_id".to_string()],
                ToolAnnotations { read_only_hint: Some(true), destructive_hint: Some(false), ..sp_annotations.clone() },
            ),
        );
        inner.tools.insert(
            "vmcp_scratchpad_list_keys".to_string(),
            Tool::with_annotations(
                "vmcp_scratchpad_list_keys",
                "List all keys in the isolated DWM scratchpad without fetching values",
                json!({
                    "project_id": {"type": "number", "description": "Project ID"},
                    "user_id": {"type": "number", "description": "User ID"},
                    "session_id": {"type": "string", "description": "Session key"}
                }),
                vec![
                    "project_id".to_string(),
                    "user_id".to_string(),
                    "session_id".to_string(),
                ],
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    ..sp_annotations.clone()
                },
            ),
        );
        inner.tools.insert(
            "vmcp_scratchpad_clean".to_string(),
            Tool::with_annotations(
                "vmcp_scratchpad_clean",
                "Clear the entire isolated DWM scratchpad for the current project/user/session scope",
                json!({
                    "project_id": {"type": "number", "description": "Project ID"},
                    "user_id": {"type": "number", "description": "User ID"},
                    "session_id": {"type": "string", "description": "Session key"}
                }),
                vec!["project_id".to_string(), "user_id".to_string(), "session_id".to_string()],
                sp_annotations,
            ),
        );

        // --- runlua: execute arbitrary Lua code ---
        inner.tools.insert(
            "runlua".to_string(),
            Tool::with_annotations(
                "runlua",
                "Execute arbitrary Lua (LuaJIT) code. Pass 'code' as a Lua script string and optional 'args' as a JSON object. The script has access to vulcan module (fs_list, fs_read, fs_write, fs_exists, fs_is_dir, path_join, cwd, exec, osinfo, json_encode, json_decode, cache_put, cache_get, cache_delete, call, log).",
                json!({
                    "code": {"type": "string", "description": "Lua code to execute. Use 'return <value>' to return results. Available: vulcan.fs_list(dir), vulcan.fs_read(path), vulcan.fs_write(path,content), vulcan.fs_exists(path), vulcan.fs_is_dir(path), vulcan.path_join(...), vulcan.cwd(), vulcan.exec(spec), vulcan.osinfo(), vulcan.json_encode(t), vulcan.json_decode(s), vulcan.cache_put(tool,value,ttl_sec), vulcan.cache_get(tool,cache_id), vulcan.cache_delete(tool,cache_id), vulcan.call(skill,args), vulcan.log(level,msg)"},
                    "args": {"type": "object", "description": "Arguments passed to the Lua code as 'args' variable"}
                }),
                vec!["code".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(false),
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
            "roots/list" => self.handle_roots_list(),
            "completion/complete" => self.handle_completion(params),
            "sampling/createMessage" => self.handle_sampling(params),
            "elicitation/create" => self.handle_elicitation(params),
            "logging/setLevel" => self.handle_set_log_level(params),
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
        eprintln!("[MCP] Client: {} ({})", client_name, negotiated);
        eprintln!(
            "[MCP] Features: sampling={}, roots={}, completions={}, logging={}, streaming={}",
            has_feature(negotiated, FeatureFlag::Sampling),
            has_feature(negotiated, FeatureFlag::Roots),
            has_feature(negotiated, FeatureFlag::Completions),
            has_feature(negotiated, FeatureFlag::StructuredLogging),
            has_feature(negotiated, FeatureFlag::Streaming),
        );

        let result = InitializeResult {
            protocol_version: negotiated.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourceCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
                prompts: Some(PromptCapability {
                    list_changed: Some(false),
                }),
                logging: if has_feature(negotiated, FeatureFlag::StructuredLogging) {
                    Some(LoggingCapability {
                        enabled: Some(true),
                    })
                } else {
                    None
                },
                completions: if has_feature(negotiated, FeatureFlag::Completions) {
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
                "Minimal MCP server supporting 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05. \
                 Features: tools (add, greet, current_time), resources, prompts, \
                 completions, roots, sampling, elicitation, structured logging."
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
            "add" => tool_add(&args),
            "greet" => tool_greet(&args),
            "current_time" => tool_time(&args),

            // --- LanceDb gRPC tools ---
            "lancedb_create_table" => {
                let client = self
                    .lancedb
                    .as_ref()
                    .ok_or_else(|| (-32603, "LanceDb client not configured".to_string()))?
                    .clone();
                let table_name = args
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let overwrite = args
                    .get("overwrite")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let columns = parse_column_defs(&args)
                    .map_err(|e| (-32602, format!("Invalid columns: {}", e)))?;
                match client.create_table(table_name, columns, overwrite).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "lancedb_upsert" => {
                let client = self
                    .lancedb
                    .clone()
                    .ok_or_else(|| (-32603, "LanceDb client not configured".to_string()))?;
                let table_name = args
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let format_str = args
                    .get("input_format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("json_rows");
                let input_format = match format_str {
                    "arrow_ipc" => crate::pb_lancedb::InputFormat::ArrowIpc,
                    _ => crate::pb_lancedb::InputFormat::JsonRows,
                };
                let data_str = args.get("data").and_then(|v| v.as_str()).unwrap_or("");
                let data = if input_format == crate::pb_lancedb::InputFormat::ArrowIpc {
                    base64_decode(data_str)
                        .map_err(|e| (-32602, format!("Invalid base64 data: {}", e)))?
                } else {
                    data_str.as_bytes().to_vec()
                };
                let key_columns: Vec<String> = args
                    .get("key_columns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                match client
                    .vector_upsert(table_name, input_format, data, key_columns)
                    .await
                {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "lancedb_search" => {
                let client = self
                    .lancedb
                    .clone()
                    .ok_or_else(|| (-32603, "LanceDb client not configured".to_string()))?;
                let table_name = args
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let vector: Vec<f32> = args
                    .get("vector")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_f64())
                            .map(|f| f as f32)
                            .collect()
                    })
                    .unwrap_or_default();
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
                let filter = args
                    .get("filter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let vector_column = args
                    .get("vector_column")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let output_format_str = args
                    .get("output_format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("json_rows");
                let output_format = match output_format_str {
                    "arrow_ipc" => crate::pb_lancedb::OutputFormat::ArrowIpc,
                    _ => crate::pb_lancedb::OutputFormat::JsonRows,
                };
                match client
                    .vector_search(
                        table_name,
                        vector,
                        limit,
                        filter,
                        vector_column,
                        output_format,
                    )
                    .await
                {
                    Ok(data) => {
                        let text = if output_format == crate::pb_lancedb::OutputFormat::JsonRows {
                            String::from_utf8_lossy(&data).to_string()
                        } else {
                            base64_encode(&data)
                        };
                        ToolCallResult {
                            content: vec![TextContent::text(&text)],
                            is_error: None,
                        }
                    }
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "lancedb_delete" => {
                let client = self
                    .lancedb
                    .clone()
                    .ok_or_else(|| (-32603, "LanceDb client not configured".to_string()))?;
                let table_name = args
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let condition = args
                    .get("condition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match client.delete(table_name, condition).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "lancedb_drop_table" => {
                let client = self
                    .lancedb
                    .clone()
                    .ok_or_else(|| (-32603, "LanceDb client not configured".to_string()))?;
                let table_name = args
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match client.drop_table(table_name).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }

            // --- Sqlite gRPC tools ---
            "sqlite_execute" => {
                let client = self
                    .sqlite
                    .clone()
                    .ok_or_else(|| (-32603, "Sqlite client not configured".to_string()))?;
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                let params = parse_sqlite_params(&args);
                match client.execute_script(sql, params).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "sqlite_execute_batch" => {
                let client = self
                    .sqlite
                    .clone()
                    .ok_or_else(|| (-32603, "Sqlite client not configured".to_string()))?;
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                let params: Vec<Vec<crate::pb_sqlite::SqliteValue>> = args
                    .get("items")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|item| parse_sqlite_params_array(item))
                            .collect()
                    })
                    .unwrap_or_default();
                match client.execute_batch(sql, params).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "sqlite_query" => {
                let client = self
                    .sqlite
                    .clone()
                    .ok_or_else(|| (-32603, "Sqlite client not configured".to_string()))?;
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                let output = args
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("json");
                let params = parse_sqlite_params(&args);
                if output == "arrow" {
                    match client.query_stream(sql, params).await {
                        Ok(data) => ToolCallResult {
                            content: vec![TextContent::text(&base64_encode(&data))],
                            is_error: None,
                        },
                        Err(e) => ToolCallResult {
                            content: vec![TextContent::text(&e)],
                            is_error: Some(true),
                        },
                    }
                } else {
                    match client.query_json(sql, params).await {
                        Ok(msg) => ToolCallResult {
                            content: vec![TextContent::text(&msg)],
                            is_error: None,
                        },
                        Err(e) => ToolCallResult {
                            content: vec![TextContent::text(&e)],
                            is_error: Some(true),
                        },
                    }
                }
            }

            // --- Scratchpad (DWM working memory via SQLite, vmcp_ tables) ---
            "vmcp_scratchpad_upsert" => {
                let store = self.scratchpad.clone().ok_or_else(|| {
                    (
                        -32603,
                        "Scratchpad store not configured. Use --sqlite to enable.".to_string(),
                    )
                })?;
                let project_id = args.get("project_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let session_id = args
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let plan_name = args.get("plan_name").and_then(|v| v.as_str()).unwrap_or("");
                // Single key/value or batch items
                let items = if let Some(arr) = args.get("items").and_then(|v| v.as_array()) {
                    arr.iter()
                        .filter_map(|item| {
                            Some(ScratchpadItem {
                                key: item
                                    .get("key")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                value: item
                                    .get("value")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                } else if let (Some(k), Some(v)) = (
                    args.get("key").and_then(|v| v.as_str()),
                    args.get("value").and_then(|v| v.as_str()),
                ) {
                    vec![ScratchpadItem {
                        key: k.to_string(),
                        value: v.to_string(),
                    }]
                } else {
                    return Err((-32602, "Either key+value or items array is required".into()));
                };
                match store
                    .upsert(project_id, user_id, session_id, plan_name, items)
                    .await
                {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "vmcp_scratchpad_delete" => {
                let store = self.scratchpad.clone().ok_or_else(|| {
                    (
                        -32603,
                        "Scratchpad store not configured. Use --sqlite to enable.".to_string(),
                    )
                })?;
                let project_id = args.get("project_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let session_id = args
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let plan_name = args.get("plan_name").and_then(|v| v.as_str()).unwrap_or("");
                let keys = if let Some(k) = args.get("key").and_then(|v| v.as_str()) {
                    vec![k.to_string()]
                } else {
                    args.get("keys")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                if keys.is_empty() {
                    return Err((-32602, "Either key or keys array is required".into()));
                }
                match store
                    .delete(project_id, user_id, session_id, plan_name, keys)
                    .await
                {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "vmcp_scratchpad_get" => {
                let store = self.scratchpad.clone().ok_or_else(|| {
                    (
                        -32603,
                        "Scratchpad store not configured. Use --sqlite to enable.".to_string(),
                    )
                })?;
                let project_id = args.get("project_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let session_id = args
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let keys: Vec<String> = args
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                match store.get(project_id, user_id, session_id, keys).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "vmcp_scratchpad_list_keys" => {
                let store = self.scratchpad.clone().ok_or_else(|| {
                    (
                        -32603,
                        "Scratchpad store not configured. Use --sqlite to enable.".to_string(),
                    )
                })?;
                let project_id = args.get("project_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let session_id = args
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match store.list_keys(project_id, user_id, session_id).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }
            "vmcp_scratchpad_clean" => {
                let store = self.scratchpad.clone().ok_or_else(|| {
                    (
                        -32603,
                        "Scratchpad store not configured. Use --sqlite to enable.".to_string(),
                    )
                })?;
                let project_id = args.get("project_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let session_id = args
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match store.clean(project_id, user_id, session_id).await {
                    Ok(msg) => ToolCallResult {
                        content: vec![TextContent::text(&msg)],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }

            // --- runlua: execute arbitrary Lua code ---
            "runlua" => {
                let engine = self.lua_engine.as_ref().ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured. Add lua_skills directory.".to_string(),
                    )
                })?;
                let code = args
                    .get("code")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| (-32602, "Missing required parameter: code".to_string()))?;
                let code = code.to_string();
                let call_args = args.get("args").cloned().unwrap_or(json!({}));
                let engine_clone = engine.clone();
                let request_context = request_context.clone();
                let result = tokio::task::spawn_blocking(move || {
                    engine_clone.run_lua(&code, &call_args, Some(&request_context))
                })
                .await
                .map_err(|e| (-32603, format!("runlua spawn error: {}", e)))?;
                match result {
                    Ok(val) => ToolCallResult {
                        content: vec![TextContent::text(&format_json_value_for_text(&val))],
                        is_error: None,
                    },
                    Err(e) => ToolCallResult {
                        content: vec![TextContent::text(&e)],
                        is_error: Some(true),
                    },
                }
            }

            _ => {
                // Check if this is a Lua skill
                if let Some(engine) = &self.lua_engine {
                    if engine.is_skill(&tool.name) {
                        let engine_clone = engine.clone();
                        let tool_name = tool.name.clone();
                        let args_clone = args.clone();
                        let request_context = request_context.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            engine_clone.call_skill(&tool_name, &args_clone, Some(&request_context))
                        })
                        .await
                        .map_err(|e| (-32603, format!("Lua skill spawn error: {}", e)))?;
                        match result {
                            Ok(val) => ToolCallResult {
                                content: vec![TextContent::text(&format_json_value_for_text(&val))],
                                is_error: None,
                            },
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
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let uri = params
            .and_then(|p| p.get("uri").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| (-32602, "Missing required parameter: uri".to_string()))?;

        {
            let inner = self
                .inner
                .try_lock()
                .map_err(|_| (-32603, "Busy".to_string()))?;

            if let Some(content) = inner.resource_data.get(&uri) {
                let mime = inner
                    .resources
                    .iter()
                    .find(|r| r.uri == uri)
                    .and_then(|r| r.mime_type.clone());
                let result = ResourceReadResult {
                    contents: vec![ResourceContents::text(&uri, content, mime)],
                };
                return serde_json::to_value(result)
                    .map_err(|e| (-32603, format!("Serialization error: {}", e)));
            }

            // Check resource templates (echo://{message})
            if uri.starts_with("echo://") {
                let message = uri.strip_prefix("echo://").unwrap_or("");
                let result = ResourceReadResult {
                    contents: vec![ResourceContents::text(
                        &uri,
                        &format!("Echo: {}", message),
                        Some("text/plain".to_string()),
                    )],
                };
                return serde_json::to_value(result)
                    .map_err(|e| (-32603, format!("Serialization error: {}", e)));
            }
        }

        if let Some(engine) = &self.lua_engine {
            if let Some(result) = engine
                .read_resource(&uri, Some(request_context))
                .map_err(|e| (-32603, format!("Lua skill resource error: {}", e)))?
            {
                return serde_json::to_value(result)
                    .map_err(|e| (-32603, format!("Serialization error: {}", e)));
            }
        }

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
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let params = params.unwrap_or_default();
        let name = params
            .get("name")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| (-32602, "Missing required parameter: name".to_string()))?;

        let language = params
            .get("arguments")
            .and_then(|a| a.get("language"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match name.as_str() {
            "code_review" => {
                let result = PromptGetResult {
                    description: Some("Code review prompt".to_string()),
                    messages: vec![
                        PromptMessage {
                            role: "user".to_string(),
                            content: TextContent::text(&format!(
                                "Please review the following {} code for correctness, performance, and best practices:",
                                language
                            )),
                        },
                        PromptMessage {
                            role: "user".to_string(),
                            content: TextContent::text("<code goes here>"),
                        },
                    ],
                };
                serde_json::to_value(result)
                    .map_err(|e| (-32603, format!("Serialization error: {}", e)))
            }
            "explain_code" => {
                let result = PromptGetResult {
                    description: Some("Code explanation prompt".to_string()),
                    messages: vec![
                        PromptMessage {
                            role: "user".to_string(),
                            content: TextContent::text(&format!(
                                "Please explain the following {} code in detail:",
                                language
                            )),
                        },
                        PromptMessage {
                            role: "user".to_string(),
                            content: TextContent::text("<code goes here>"),
                        },
                    ],
                };
                serde_json::to_value(result)
                    .map_err(|e| (-32603, format!("Serialization error: {}", e)))
            }
            _ => {
                if let Some(engine) = &self.lua_engine {
                    if let Some(result) = engine
                        .get_prompt(
                            &name,
                            params.get("arguments").unwrap_or(&Value::Null),
                            Some(request_context),
                        )
                        .map_err(|e| (-32603, format!("Lua skill prompt error: {}", e)))?
                    {
                        return serde_json::to_value(result)
                            .map_err(|e| (-32603, format!("Serialization error: {}", e)));
                    }
                }
                Err((-32602, format!("Prompt not found: {}", name)))
            }
        }
    }

    fn handle_roots_list(&self) -> Result<Value, (i64, String)> {
        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        Ok(json!({ "roots": inner.roots }))
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

        let inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        let values: Vec<String> = match (ref_type, argument_name) {
            ("ref/prompt", "language") => {
                let all = vec![
                    "rust",
                    "python",
                    "javascript",
                    "typescript",
                    "go",
                    "java",
                    "c++",
                    "ruby",
                ];
                all.into_iter()
                    .filter(|s| s.starts_with(argument_value))
                    .map(String::from)
                    .collect()
            }
            ("ref/resource", _) => inner.resources.iter().map(|r| r.name.clone()).collect(),
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

    fn handle_sampling(&self, _params: Option<Value>) -> Result<Value, (i64, String)> {
        eprintln!("[MCP] Sampling request received (demo mode)");
        let result = SamplingResult {
            role: "assistant".to_string(),
            content: TextContent::text("This is a mock sampling response."),
            model: "demo-model".to_string(),
            stop_reason: Some("end_turn".to_string()),
        };
        serde_json::to_value(result).map_err(|e| (-32603, format!("Serialization error: {}", e)))
    }

    fn handle_elicitation(&self, _params: Option<Value>) -> Result<Value, (i64, String)> {
        eprintln!("[MCP] Elicitation request received (demo mode)");
        let result = ElicitationResult {
            action: "accept".to_string(),
            content: Some(json!({"confirmed": true})),
        };
        serde_json::to_value(result).map_err(|e| (-32603, format!("Serialization error: {}", e)))
    }

    fn handle_set_log_level(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
        let level = params
            .and_then(|p| p.get("level").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| (-32602, "Missing required parameter: level".to_string()))?;

        let mut inner = self
            .inner
            .try_lock()
            .map_err(|_| (-32603, "Busy".to_string()))?;
        inner.log_level = level.clone();
        eprintln!("[MCP] Log level set to: {}", level);
        Ok(json!({}))
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

// ============================================================
// Helper: base64 encode/decode
// ============================================================

fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let _ = write!(
            result,
            "{}",
            CHARS[((triple >> 18) & 0x3F) as usize] as char
        );
        let _ = write!(
            result,
            "{}",
            CHARS[((triple >> 12) & 0x3F) as usize] as char
        );
        if chunk.len() > 1 {
            let _ = write!(result, "{}", CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            let _ = write!(result, "{}", CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut decode_map = [255u8; 256];
    for (i, &c) in table.iter().enumerate() {
        decode_map[c as usize] = i as u8;
    }
    let input = input.trim_end_matches('=');
    let mut result = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let w = decode_chunk(&decode_map, &bytes[i..i + 4])?;
        result.extend_from_slice(&w);
        i += 4;
    }
    if i < bytes.len() {
        let remaining = &bytes[i..];
        let mut buf = [0u8; 4];
        buf[..remaining.len()].copy_from_slice(remaining);
        let w = decode_chunk(&decode_map, &buf)?;
        let take = match remaining.len() {
            2 => 1,
            3 => 2,
            _ => 0,
        };
        result.extend_from_slice(&w[..take]);
    }
    Ok(result)
}

fn decode_chunk(map: &[u8; 256], chunk: &[u8]) -> Result<[u8; 3], String> {
    let mut vals = [0u8; 4];
    for (i, &c) in chunk.iter().enumerate() {
        let v = if c == b'=' {
            0
        } else if c as usize >= 256 || map[c as usize] == 255 {
            return Err(format!("Invalid base64 char: {}", c));
        } else {
            map[c as usize]
        };
        vals[i] = v;
    }
    let triple = ((vals[0] as u32) << 18)
        | ((vals[1] as u32) << 12)
        | ((vals[2] as u32) << 6)
        | (vals[3] as u32);
    Ok([
        (triple >> 16) as u8,
        ((triple >> 8) & 0xFF) as u8,
        (triple & 0xFF) as u8,
    ])
}

// ============================================================
// Helper: parse SqliteValue params from JSON args
// ============================================================

fn parse_sqlite_params(args: &Value) -> Vec<crate::pb_sqlite::SqliteValue> {
    use crate::grpc_client::*;
    args.get("params")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| match v {
                    Value::Null => sqlite_null(),
                    Value::Bool(b) => sqlite_bool(*b),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            sqlite_int64(i)
                        } else if let Some(f) = n.as_f64() {
                            sqlite_float64(f)
                        } else {
                            sqlite_null()
                        }
                    }
                    Value::String(s) => sqlite_string(s),
                    _ => sqlite_null(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ============================================================
// Helper: parse ColumnDef from JSON args
// ============================================================

fn column_type_from_str(s: &str) -> crate::pb_lancedb::ColumnType {
    match s {
        "string" | "COLUMN_TYPE_STRING" => crate::pb_lancedb::ColumnType::String,
        "int64" | "COLUMN_TYPE_INT64" => crate::pb_lancedb::ColumnType::Int64,
        "float64" | "COLUMN_TYPE_FLOAT64" => crate::pb_lancedb::ColumnType::Float64,
        "bool" | "COLUMN_TYPE_BOOL" => crate::pb_lancedb::ColumnType::Bool,
        "vector_float32" | "COLUMN_TYPE_VECTOR_FLOAT32" => {
            crate::pb_lancedb::ColumnType::VectorFloat32
        }
        "float32" | "COLUMN_TYPE_FLOAT32" => crate::pb_lancedb::ColumnType::Float32,
        "uint64" | "COLUMN_TYPE_UINT64" => crate::pb_lancedb::ColumnType::Uint64,
        "int32" | "COLUMN_TYPE_INT32" => crate::pb_lancedb::ColumnType::Int32,
        "uint32" | "COLUMN_TYPE_UINT32" => crate::pb_lancedb::ColumnType::Uint32,
        _ => crate::pb_lancedb::ColumnType::Unspecified,
    }
}

fn parse_column_defs(args: &Value) -> Result<Vec<crate::pb_lancedb::ColumnDef>, String> {
    let cols = args
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing 'columns' array".to_string())?;

    cols.iter()
        .map(|c| {
            let name = c
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let column_type_str = c
                .get("column_type")
                .and_then(|v| v.as_str())
                .unwrap_or("string");
            let column_type = column_type_from_str(column_type_str);
            let vector_dim = c.get("vector_dim").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let nullable = c.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(crate::pb_lancedb::ColumnDef {
                name,
                column_type: column_type.into(),
                vector_dim,
                nullable,
            })
        })
        .collect()
}

// ============================================================
// Helper: parse a single item's params array for batch execution
// ============================================================

fn parse_sqlite_params_array(item: &Value) -> Vec<crate::pb_sqlite::SqliteValue> {
    use crate::grpc_client::*;
    item.as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| match v {
                    Value::Null => sqlite_null(),
                    Value::Bool(b) => sqlite_bool(*b),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            sqlite_int64(i)
                        } else if let Some(f) = n.as_f64() {
                            sqlite_float64(f)
                        } else {
                            sqlite_null()
                        }
                    }
                    Value::String(s) => sqlite_string(s),
                    _ => sqlite_null(),
                })
                .collect()
        })
        .unwrap_or_default()
}
