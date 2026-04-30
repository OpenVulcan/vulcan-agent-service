use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::Mutex;

use crate::client_budget::reload_client_budget_config;
use crate::config::Config;
use crate::grpc_client::VmmClient;
use crate::luaskills_host::{
    build_grpc_runtime_invocation_context, build_grpc_runtime_request_context,
    build_luaskills_engine_options, build_runtime_invocation_context,
    build_runtime_request_context, client_budget_snapshot_for_render,
    grpc_client_budget_snapshot_for_render, install_luaskills_log_callback,
    map_runtime_entry_to_mcp_tool,
};
use crate::protocol::*;
use crate::temp_maintenance::ensure_runtime_temp_dir;
use crate::tool_config::reload_tool_configs;
use crate::tool_result_format::{HostRenderOptions, render_tool_result_text};
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    InstalledSkillRecord, LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig,
    RuntimeEntryDescriptor, RuntimeEntryRegistryDelta, RuntimeHelpDetail,
    RuntimeSkillHelpDescriptor, RuntimeSkillLifecycleCallback, RuntimeSkillLifecycleEvent,
    RuntimeSkillRoot, SkillApplyResult, SkillConfigEntry, SkillInstallRequest,
    SkillInstallSourceType, SkillManager, SkillManagerConfig, SkillUninstallOptions,
    SkillUninstallResult, ToolCacheConfig, runtime_config_store::SkillConfigStore,
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
        "vulcan-help-list"
            | "vulcan-help-detail"
            | "reload_vulcan_mcp_configs"
            | "luaskill-config"
            | "skill-manager"
    )
}

/// Return whether one host-owned MCP tool requires a ready Lua engine to succeed.
/// 返回某个宿主自有 MCP 工具在执行时是否依赖已就绪的 Lua 引擎。
pub fn host_tool_requires_lua_engine(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "vulcan-help-list" | "vulcan-help-detail" | "skill-manager"
    )
}

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

struct ServerInner {
    /// Host-owned MCP tools registered by the current host adapter and never mutated by LuaSkills runtime deltas.
    /// 当前宿主适配层拥有的 MCP 工具注册表，不会被 LuaSkills 运行时差异事件修改。
    host_tools: HashMap<String, Tool>,
    /// LuaSkills-managed dynamic MCP tools derived from runtime entries and fully driven by runtime registry deltas.
    /// 由 LuaSkills 运行时入口派生并完全受运行时注册表差异驱动的动态 MCP 工具注册表。
    skill_tools: HashMap<String, Tool>,
    /// LuaSkills runtime entry metadata keyed by canonical dynamic tool name.
    /// 按标准动态工具名索引的 LuaSkills 运行时入口元数据。
    skill_entries: HashMap<String, RuntimeEntryDescriptor>,
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

/// Supported actions for the host-owned skill-manager tool.
/// 宿主自有 skill-manager 工具支持的动作集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SkillManagerAction {
    /// List locally effective skills with paths and managed install records.
    /// 列出本地生效技能及其路径和受管安装记录。
    List,
    /// Install one managed skill from a source locator.
    /// 从来源定位值安装一个受管技能。
    Install,
    /// Update one installed managed skill by skill id.
    /// 通过技能标识更新一个已安装的受管技能。
    Update,
    /// Uninstall one installed skill by skill id while retaining databases.
    /// 通过技能标识卸载一个已安装技能并保留数据库。
    Uninstall,
}

/// Parsed arguments for one host-owned skill-manager tool call.
/// 一次宿主自有 skill-manager 工具调用解析后的参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct SkillManagerToolArguments {
    /// Action selector that chooses one of `list/install/update/uninstall`.
    /// 动作选择器，用于决定 `list/install/update/uninstall` 中的哪一种。
    action: SkillManagerAction,
    /// Optional install source locator such as `LuaSkills/vulcan-codekit` or a source YAML URL.
    /// 可选安装来源定位值，例如 `LuaSkills/vulcan-codekit` 或 source YAML 地址。
    source: Option<String>,
    /// Optional source type override; omitted values are inferred from `source`.
    /// 可选来源类型覆盖；未提供时从 `source` 自动推导。
    source_type: Option<SkillInstallSourceType>,
    /// Optional target skill id required by update and uninstall actions.
    /// update 与 uninstall 动作必填的可选目标技能标识。
    skill_id: Option<String>,
}

impl McpServer {
    pub fn new() -> Self {
        let inner = ServerInner {
            host_tools: HashMap::new(),
            skill_tools: HashMap::new(),
            skill_entries: HashMap::new(),
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
                insert_skill_entry(&mut inner, entry);
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

        // --- skill-manager: host-owned LuaSkills install/update/uninstall/list management ---
        inner.host_tools.insert(
            "skill-manager".to_string(),
            Tool::with_annotations(
                "skill-manager",
                "Manage locally installed USER-layer LuaSkills packages through the host runtime. Supports `list`, `install`, `update`, and `uninstall`. Only call `install`, `update`, or `uninstall` when the user explicitly authorizes that exact operation; never perform skill lifecycle changes proactively. The target layer is fixed to USER and cannot be changed. Install derives the skill id from `source`; update and uninstall require `skill_id`. Uninstall retains SQLite and LanceDB data.",
                json!({
                    "action": {
                        "type": "string",
                        "description": "Operation to perform. Supported values: `list`, `install`, `update`, `uninstall`.",
                        "enum": ["list", "install", "update", "uninstall"]
                    },
                    "source": {
                        "type": "string",
                        "description": "Install source locator. Required for `install`; examples: `LuaSkills/vulcan-codekit` or `https://github.com/LuaSkills/vulcan-codekit`."
                    },
                    "source_type": {
                        "type": "string",
                        "description": "Optional install source type override. When omitted, GitHub repository locators are treated as `github`, and non-GitHub HTTP(S) URLs are treated as `url`.",
                        "enum": ["github", "url"]
                    },
                    "skill_id": {
                        "type": "string",
                        "description": "Target skill id. Required for `update` and `uninstall`; not used for `install` because it is derived from the source."
                    }
                }),
                vec!["action".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    user_confirmation_required: Some(true),
                    idempotent_hint: Some(false),
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

    /// Resolve the Lua runtime engine, full root chain, and host-forced USER target root.
    /// 解析 Lua 运行时引擎、完整根链以及宿主强制指定的 USER 目标根。
    fn resolve_lua_runtime_user_target(
        &self,
    ) -> Result<
        (
            Arc<StdRwLock<LuaEngine>>,
            Vec<RuntimeSkillRoot>,
            RuntimeSkillRoot,
        ),
        (i64, String),
    > {
        let (engine, roots) = self.resolve_lua_runtime_target()?;
        let target_root = select_skill_manager_user_root(&roots)?;
        Ok((engine, roots, target_root))
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

    /// List loaded LuaSkill packages from the dynamic runtime entry registry.
    /// 从动态运行时入口注册表列出已加载 LuaSkill 包。
    pub async fn list_luaskill_packages(
        &self,
    ) -> Result<Vec<LuaSkillPackageDescriptor>, (i64, String)> {
        let inner = self.inner.lock().await;
        let mut grouped: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        for entry in inner.skill_entries.values() {
            grouped
                .entry((
                    entry.skill_id.clone(),
                    entry.root_name.clone(),
                    entry.skill_dir.clone(),
                ))
                .or_default()
                .push(entry.canonical_name.clone());
        }

        Ok(grouped
            .into_iter()
            .map(|((skill_id, root_name, skill_dir), mut tool_names)| {
                tool_names.sort();
                LuaSkillPackageDescriptor {
                    skill_id,
                    root_name,
                    skill_dir,
                    tool_names,
                }
            })
            .collect())
    }

    /// List dynamic LuaSkill tools without including host-owned stable tools.
    /// 列出动态 LuaSkill 工具，不包含宿主自有稳定工具。
    pub async fn list_luaskill_tools(&self) -> Result<Vec<LuaSkillToolDescriptor>, (i64, String)> {
        let inner = self.inner.lock().await;
        let mut tools = Vec::new();
        for (tool_name, entry) in &inner.skill_entries {
            if let Some(tool) = inner.skill_tools.get(tool_name) {
                tools.push(build_luaskill_tool_descriptor(tool, entry));
            }
        }
        tools.sort_by(|left, right| left.tool.name.cmp(&right.tool.name));
        Ok(tools)
    }

    /// Get one dynamic LuaSkill tool descriptor by canonical tool name.
    /// 按标准工具名读取一个动态 LuaSkill 工具描述。
    pub async fn get_luaskill_tool(
        &self,
        tool_name: &str,
    ) -> Result<LuaSkillToolDescriptor, (i64, String)> {
        let tool_name = require_non_empty_grpc_field(tool_name, "tool_name")?;
        let inner = self.inner.lock().await;
        let entry = inner.skill_entries.get(&tool_name).ok_or_else(|| {
            (
                -32601,
                format!("Dynamic LuaSkill tool not found: {}", tool_name),
            )
        })?;
        let tool = inner.skill_tools.get(&tool_name).ok_or_else(|| {
            (
                -32603,
                format!("LuaSkill tool metadata is inconsistent: {}", tool_name),
            )
        })?;
        Ok(build_luaskill_tool_descriptor(tool, entry))
    }

    /// Invoke one dynamic LuaSkill tool through the gRPC-specific budget path.
    /// 通过 gRPC 专用预算路径调用一个动态 LuaSkill 工具。
    pub async fn call_luaskill_tool(
        &self,
        tool_name: &str,
        arguments: Value,
        client_name: &str,
        client_version: Option<&str>,
    ) -> Result<ToolCallResult, (i64, String)> {
        let tool_name = require_non_empty_grpc_field(tool_name, "tool_name")?;
        let client_name = require_non_empty_grpc_field(client_name, "client_name")?;
        let inner = self.inner.lock().await;
        let tool = inner.skill_tools.get(&tool_name).ok_or_else(|| {
            (
                -32601,
                format!(
                    "Dynamic LuaSkill tool not found or not callable through CallTool: {}",
                    tool_name
                ),
            )
        })?;
        let tool = tool.clone();
        drop(inner);

        let (target_engine, target_skill_roots) = self.resolve_lua_runtime_target()?;
        let is_skill = target_engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .is_skill(&tool.name);
        if !is_skill {
            return Err((
                -32603,
                format!("LuaSkills runtime no longer owns tool: {}", tool.name),
            ));
        }

        let skill_name = target_engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .skill_name_for_tool(&tool.name);
        let engine_clone = target_engine.clone();
        let tool_name_for_call = tool.name.clone();
        let args_clone = arguments.clone();
        let invocation_context = build_grpc_runtime_invocation_context(
            &client_name,
            client_version,
            Some(&tool_name_for_call),
            skill_name.as_deref(),
        );
        let result = tokio::task::spawn_blocking(move || {
            let engine = engine_clone
                .read()
                .map_err(|_| "Lua engine lock poisoned".to_string())?;
            engine.call_skill(&tool_name_for_call, &args_clone, Some(&invocation_context))
        })
        .await
        .map_err(|error| (-32603, format!("Lua skill spawn error: {}", error)))?;

        match result {
            Ok(value) => {
                let client_budget = grpc_client_budget_snapshot_for_render(
                    &client_name,
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
                Ok(ToolCallResult {
                    content: vec![TextContent::text(&render_tool_result_text(
                        &value,
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
                                .and_then(|options| options.host_options.resources_dir.clone()),
                        },
                    ))],
                    is_error: None,
                })
            }
            Err(error) => Ok(ToolCallResult {
                content: vec![TextContent::text(&error)],
                is_error: Some(true),
            }),
        }
    }

    /// Render the registered LuaSkills help tree for the gRPC stable help method.
    /// 为 gRPC 稳定帮助方法渲染已注册的 LuaSkills 帮助树。
    pub fn list_luaskill_help(&self) -> Result<String, (i64, String)> {
        let engine = self.resolve_lua_engine_for_environment()?;
        let help_tree = engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .list_skill_help();
        Ok(render_help_list_markdown(&help_tree))
    }

    /// Render one LuaSkills help flow for the gRPC stable help method.
    /// 为 gRPC 稳定帮助方法渲染一个 LuaSkills 帮助流程。
    pub async fn get_luaskill_help(
        &self,
        skill_id: &str,
        flow: &str,
        client_name: &str,
        client_version: Option<&str>,
    ) -> Result<ToolCallResult, (i64, String)> {
        let skill_id = require_non_empty_grpc_field(skill_id, "skill_id")?;
        let flow = require_non_empty_grpc_field(flow, "flow")?;
        let client_name = require_non_empty_grpc_field(client_name, "client_name")?;
        let engine = self.resolve_lua_engine_for_environment()?;
        let runtime_request_context =
            build_grpc_runtime_request_context(&client_name, client_version);
        let result = tokio::task::spawn_blocking(move || {
            let engine = engine
                .read()
                .map_err(|_| "Lua engine lock poisoned".to_string())?;
            engine.render_skill_help_detail(&skill_id, &flow, Some(&runtime_request_context))
        })
        .await
        .map_err(|error| (-32603, format!("vulcan-help-detail spawn error: {}", error)))?;

        match result {
            Ok(Some(detail)) => Ok(ToolCallResult {
                content: vec![TextContent::text(&render_help_detail_markdown(&detail))],
                is_error: None,
            }),
            Ok(None) => Ok(ToolCallResult {
                content: vec![TextContent::text("Skill help not found.")],
                is_error: Some(true),
            }),
            Err(error) => Ok(ToolCallResult {
                content: vec![TextContent::text(&error)],
                is_error: Some(true),
            }),
        }
    }

    /// List host-managed LuaSkill config values through a stable gRPC method.
    /// 通过稳定 gRPC 方法列出宿主管理的 LuaSkill 配置值。
    pub fn list_luaskill_config(&self, skill_id: Option<String>) -> Result<String, (i64, String)> {
        let store = self.resolve_runtime_skill_config_store()?;
        let request = RuntimeConfigToolArguments {
            action: RuntimeConfigAction::List,
            skill_id,
            key: None,
            value: None,
        };
        execute_runtime_config_tool(&store, &request)
    }

    /// Read one host-managed LuaSkill config value through a stable gRPC method.
    /// 通过稳定 gRPC 方法读取一个宿主管理的 LuaSkill 配置值。
    pub fn get_luaskill_config(
        &self,
        skill_id: String,
        key: String,
    ) -> Result<String, (i64, String)> {
        let store = self.resolve_runtime_skill_config_store()?;
        let request = RuntimeConfigToolArguments {
            action: RuntimeConfigAction::Get,
            skill_id: Some(skill_id),
            key: Some(key),
            value: None,
        };
        execute_runtime_config_tool(&store, &request)
    }

    /// Write one host-managed LuaSkill config value through a stable gRPC method.
    /// 通过稳定 gRPC 方法写入一个宿主管理的 LuaSkill 配置值。
    pub fn set_luaskill_config(
        &self,
        skill_id: String,
        key: String,
        value: String,
    ) -> Result<String, (i64, String)> {
        let store = self.resolve_runtime_skill_config_store()?;
        let request = RuntimeConfigToolArguments {
            action: RuntimeConfigAction::Set,
            skill_id: Some(skill_id),
            key: Some(key),
            value: Some(value),
        };
        execute_runtime_config_tool(&store, &request)
    }

    /// Delete one host-managed LuaSkill config value through a stable gRPC method.
    /// 通过稳定 gRPC 方法删除一个宿主管理的 LuaSkill 配置值。
    pub fn delete_luaskill_config(
        &self,
        skill_id: String,
        key: String,
    ) -> Result<String, (i64, String)> {
        let store = self.resolve_runtime_skill_config_store()?;
        let request = RuntimeConfigToolArguments {
            action: RuntimeConfigAction::Delete,
            skill_id: Some(skill_id),
            key: Some(key),
            value: None,
        };
        execute_runtime_config_tool(&store, &request)
    }

    /// Render the USER-layer managed LuaSkill inventory through a stable gRPC method.
    /// 通过稳定 gRPC 方法渲染 USER 层受管 LuaSkill 清单。
    pub fn list_installed_luaskills(&self) -> Result<String, (i64, String)> {
        self.render_skill_manager_list()
    }

    /// Install one USER-layer managed LuaSkill through a stable gRPC method.
    /// 通过稳定 gRPC 方法安装一个 USER 层受管 LuaSkill。
    pub async fn install_luaskill(
        &self,
        source: String,
        source_type: Option<String>,
    ) -> Result<ToolCallResult, (i64, String)> {
        let source = require_skill_manager_source(Some(source.as_str()), "install")?;
        let source_type = parse_optional_skill_install_source_type(source_type.as_deref())?
            .unwrap_or_else(|| infer_skill_install_source_type(&source, None));
        if matches!(source_type, SkillInstallSourceType::Url) {
            return Ok(render_skill_url_install_not_implemented_result());
        }
        self.execute_skill_install(SkillInstallRequest {
            skill_id: None,
            source: Some(source),
            source_type,
        })
        .await
    }

    /// Update one USER-layer managed LuaSkill through a stable gRPC method.
    /// 通过稳定 gRPC 方法更新一个 USER 层受管 LuaSkill。
    pub async fn update_luaskill(&self, skill_id: String) -> Result<ToolCallResult, (i64, String)> {
        let skill_id = require_skill_manager_skill_id(Some(skill_id.as_str()), "update")?;
        self.execute_skill_update(SkillInstallRequest {
            skill_id: Some(skill_id),
            source: None,
            source_type: SkillInstallSourceType::Github,
        })
        .await
    }

    /// Uninstall one USER-layer LuaSkill through a stable gRPC method.
    /// 通过稳定 gRPC 方法卸载一个 USER 层 LuaSkill。
    pub async fn uninstall_luaskill(
        &self,
        skill_id: String,
    ) -> Result<ToolCallResult, (i64, String)> {
        let skill_id = require_skill_manager_skill_id(Some(skill_id.as_str()), "uninstall")?;
        self.execute_skill_uninstall(skill_id).await
    }

    /// Reload hot-reloadable runtime configs through a stable gRPC method.
    /// 通过稳定 gRPC 方法重载可热重载运行时配置。
    pub fn reload_luaskill_runtime_configs(&self) -> Result<String, (i64, String)> {
        let client_budget_report = reload_client_budget_config()
            .map_err(|error| (-32603, format!("reload client budgets failed: {}", error)))?;
        let tool_config_report = reload_tool_configs()
            .map_err(|error| (-32603, format!("reload tool configs failed: {}", error)))?;

        Ok(format!(
            "Runtime MCP configs reloaded successfully.\n- client_budgets: patterns={}, grpc_clients={}, source={}\n- tool_configs: tools={}, source={}\n- config.yaml: not reloaded",
            client_budget_report.client_count,
            client_budget_report.grpc_client_count,
            client_budget_report
                .source_path
                .as_deref()
                .unwrap_or("unavailable"),
            tool_config_report.tool_count,
            tool_config_report
                .source_path
                .as_deref()
                .unwrap_or("unavailable")
        ))
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
                    "Runtime MCP configs reloaded successfully.\n- client_budgets: patterns={}, grpc_clients={}, source={}\n- tool_configs: tools={}, source={}\n- config.yaml: not reloaded",
                    client_budget_report.client_count,
                    client_budget_report.grpc_client_count,
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

            "skill-manager" => {
                let request = parse_skill_manager_tool_arguments(&args)?;
                self.execute_skill_manager_tool(request).await?
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

    /// Execute one parsed host-owned skill-manager request against the configured LuaSkills runtime.
    /// 针对已配置的 LuaSkills 运行时执行一次解析后的宿主自有 skill-manager 请求。
    async fn execute_skill_manager_tool(
        &self,
        request: SkillManagerToolArguments,
    ) -> Result<ToolCallResult, (i64, String)> {
        match request.action {
            SkillManagerAction::List => {
                let rendered = self.render_skill_manager_list()?;
                Ok(ToolCallResult {
                    content: vec![TextContent::text(&rendered)],
                    is_error: None,
                })
            }
            SkillManagerAction::Install => {
                let source = require_skill_manager_source(request.source.as_deref(), "install")?;
                let source_type = infer_skill_install_source_type(&source, request.source_type);
                if matches!(source_type, SkillInstallSourceType::Url) {
                    return Ok(render_skill_url_install_not_implemented_result());
                }
                let install_request = SkillInstallRequest {
                    skill_id: None,
                    source: Some(source),
                    source_type,
                };
                self.execute_skill_install(install_request).await
            }
            SkillManagerAction::Update => {
                let skill_id =
                    require_skill_manager_skill_id(request.skill_id.as_deref(), "update")?;
                let update_request = SkillInstallRequest {
                    skill_id: Some(skill_id),
                    source: None,
                    source_type: SkillInstallSourceType::Github,
                };
                self.execute_skill_update(update_request).await
            }
            SkillManagerAction::Uninstall => {
                let skill_id =
                    require_skill_manager_skill_id(request.skill_id.as_deref(), "uninstall")?;
                self.execute_skill_uninstall(skill_id).await
            }
        }
    }

    /// Render the local LuaSkills inventory with paths, enabled state, and managed install records.
    /// 渲染本地 LuaSkills 清单，包括路径、启用状态与受管安装记录。
    fn render_skill_manager_list(&self) -> Result<String, (i64, String)> {
        let roots = self
            .lua_skill_roots
            .as_ref()
            .ok_or_else(|| (-32603, "Lua skill roots are not configured.".to_string()))?;
        let target_root = select_skill_manager_user_root(roots)?;
        let engine_options = self
            .lua_engine_options
            .as_ref()
            .ok_or_else(|| (-32603, "Lua engine options are not configured.".to_string()))?;
        let layer_roots = vec![target_root];
        let instances = collect_effective_skill_instances_from_roots(&layer_roots)
            .map_err(|error| (-32603, format!("skill-manager list failed: {}", error)))?;

        if instances.is_empty() {
            return Ok("No LuaSkills are installed in the USER layer.".to_string());
        }

        let mut rendered = String::new();
        writeln!(&mut rendered, "# LuaSkills (USER)").expect("writing to String should not fail");
        for instance in instances {
            let root = RuntimeSkillRoot {
                name: instance.root_name.clone(),
                skills_dir: instance.skills_root.clone(),
            };
            let manager = build_skill_manager_for_root(&root, &engine_options.host_options)?;
            let install_record = manager
                .install_record(&instance.skill_id)
                .map_err(|error| {
                    (
                        -32603,
                        format!(
                            "skill-manager list failed to read install record for '{}': {}",
                            instance.skill_id, error
                        ),
                    )
                })?;
            let disabled_record = manager
                .disabled_record(&instance.skill_id)
                .map_err(|error| {
                    (
                        -32603,
                        format!(
                            "skill-manager list failed to read disabled record for '{}': {}",
                            instance.skill_id, error
                        ),
                    )
                })?;
            writeln!(&mut rendered).expect("writing to String should not fail");
            writeln!(&mut rendered, "## {}", instance.skill_id)
                .expect("writing to String should not fail");
            writeln!(&mut rendered, "- root: {}", instance.root_name)
                .expect("writing to String should not fail");
            writeln!(&mut rendered, "- path: {}", instance.actual_dir.display())
                .expect("writing to String should not fail");
            writeln!(&mut rendered, "- enabled: {}", disabled_record.is_none())
                .expect("writing to String should not fail");
            if let Some(record) = install_record {
                render_skill_install_record(&mut rendered, &record);
            } else {
                writeln!(&mut rendered, "- managed: false")
                    .expect("writing to String should not fail");
            }
            if let Some(record) = disabled_record {
                writeln!(
                    &mut rendered,
                    "- disabled_reason: {}",
                    record.reason.as_deref().unwrap_or("")
                )
                .expect("writing to String should not fail");
                writeln!(
                    &mut rendered,
                    "- disabled_at_unix_ms: {}",
                    record.disabled_at_unix_ms
                )
                .expect("writing to String should not fail");
            }
        }
        Ok(rendered)
    }

    /// Execute one managed skill install against the host-forced USER target root.
    /// 针对宿主强制指定的 USER 目标根执行一次受管技能安装。
    async fn execute_skill_install(
        &self,
        request: SkillInstallRequest,
    ) -> Result<ToolCallResult, (i64, String)> {
        let (engine, roots, target_root) = self.resolve_lua_runtime_user_target()?;
        let operation = tokio::task::spawn_blocking(move || {
            let mut engine = engine
                .write()
                .map_err(|_| "Lua engine lock poisoned.".to_string())?;
            engine
                .install_skill_in_root(&roots, &target_root, &request)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| {
            (
                -32603,
                format!("skill-manager install spawn error: {}", error),
            )
        })?;
        Ok(render_skill_apply_tool_result("install", operation))
    }

    /// Execute one managed skill update against the host-forced USER target root.
    /// 针对宿主强制指定的 USER 目标根执行一次受管技能更新。
    async fn execute_skill_update(
        &self,
        request: SkillInstallRequest,
    ) -> Result<ToolCallResult, (i64, String)> {
        let (engine, roots, target_root) = self.resolve_lua_runtime_user_target()?;
        let operation = tokio::task::spawn_blocking(move || {
            let mut engine = engine
                .write()
                .map_err(|_| "Lua engine lock poisoned.".to_string())?;
            engine
                .update_skill_in_root(&roots, &target_root, &request)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| {
            (
                -32603,
                format!("skill-manager update spawn error: {}", error),
            )
        })?;
        Ok(render_skill_apply_tool_result("update", operation))
    }

    /// Execute one USER-targeted skill uninstall while retaining all skill-owned databases.
    /// 执行一次以 USER 为目标的技能卸载，并保留该技能拥有的全部数据库。
    async fn execute_skill_uninstall(
        &self,
        skill_id: String,
    ) -> Result<ToolCallResult, (i64, String)> {
        let (engine, roots, target_root) = self.resolve_lua_runtime_user_target()?;
        let operation = tokio::task::spawn_blocking(move || {
            let mut engine = engine
                .write()
                .map_err(|_| "Lua engine lock poisoned.".to_string())?;
            engine
                .uninstall_skill_in_root(
                    &roots,
                    &target_root,
                    &skill_id,
                    &SkillUninstallOptions::default(),
                )
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| {
            (
                -32603,
                format!("skill-manager uninstall spawn error: {}", error),
            )
        })?;
        Ok(render_skill_uninstall_tool_result(operation))
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

/// Build one gRPC-facing LuaSkill tool descriptor from MCP tool schema and runtime entry metadata.
/// 基于 MCP 工具 schema 与运行时入口元数据构造一个面向 gRPC 的 LuaSkill 工具描述。
fn build_luaskill_tool_descriptor(
    tool: &Tool,
    entry: &RuntimeEntryDescriptor,
) -> LuaSkillToolDescriptor {
    LuaSkillToolDescriptor {
        tool: tool.clone(),
        skill_id: entry.skill_id.clone(),
        entry_name: entry.local_name.clone(),
        root_name: entry.root_name.clone(),
        skill_dir: entry.skill_dir.clone(),
    }
}

/// Require one non-empty gRPC field and return its trimmed value.
/// 要求一个 gRPC 字段非空，并返回去除首尾空白后的值。
fn require_non_empty_grpc_field(value: &str, field_name: &str) -> Result<String, (i64, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err((
            -32602,
            format!("gRPC LuaSkills request requires parameter: {}", field_name),
        ))
    } else {
        Ok(trimmed.to_string())
    }
}

/// Parse one skill-manager argument payload into the strongly typed host-tool request model.
/// 把一份 skill-manager 参数载荷解析为强类型宿主工具请求模型。
fn parse_skill_manager_tool_arguments(
    args: &Value,
) -> Result<SkillManagerToolArguments, (i64, String)> {
    if args.get("layer").is_some() {
        return Err((
            -32602,
            "skill-manager is locked to the USER layer and does not accept a layer parameter."
                .to_string(),
        ));
    }
    serde_json::from_value(args.clone()).map_err(|error| {
        (
            -32602,
            format!("Invalid skill-manager arguments: {}", error),
        )
    })
}

/// Parse an optional skill install source type supplied by a stable gRPC method.
/// 解析稳定 gRPC 方法传入的可选技能安装来源类型。
fn parse_optional_skill_install_source_type(
    value: Option<&str>,
) -> Result<Option<SkillInstallSourceType>, (i64, String)> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    match value {
        "github" => Ok(Some(SkillInstallSourceType::Github)),
        "url" => Ok(Some(SkillInstallSourceType::Url)),
        _ => Err((
            -32602,
            format!(
                "Unsupported skill install source_type '{}'; expected 'github' or 'url'.",
                value
            ),
        )),
    }
}

/// Select the concrete USER runtime root used by the user-facing skill-manager tool.
/// 选择面向用户的 skill-manager 工具固定使用的 USER 运行时根。
fn select_skill_manager_user_root(
    roots: &[RuntimeSkillRoot],
) -> Result<RuntimeSkillRoot, (i64, String)> {
    let selected = roots
        .iter()
        .find(|root| root.name.trim().eq_ignore_ascii_case("USER"));
    selected.cloned().ok_or_else(|| {
        (
            -32603,
            "skill-manager USER layer is not configured.".to_string(),
        )
    })
}

/// Require one non-empty install source for a skill-manager install action.
/// 要求 skill-manager 安装动作提供一个非空安装来源。
fn require_skill_manager_source(
    value: Option<&str>,
    action: &str,
) -> Result<String, (i64, String)> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            (
                -32602,
                format!(
                    "skill-manager action '{}' requires parameter: source",
                    action
                ),
            )
        })
}

/// Require one non-empty skill id for a skill-manager target action.
/// 要求 skill-manager 目标动作提供一个非空技能标识。
fn require_skill_manager_skill_id(
    value: Option<&str>,
    action: &str,
) -> Result<String, (i64, String)> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            (
                -32602,
                format!(
                    "skill-manager action '{}' requires parameter: skill_id",
                    action
                ),
            )
        })
}

/// Infer the install source type from one source locator unless the caller supplied an override.
/// 除非调用方提供覆盖值，否则从单个来源定位值推导安装来源类型。
fn infer_skill_install_source_type(
    source: &str,
    explicit: Option<SkillInstallSourceType>,
) -> SkillInstallSourceType {
    if let Some(source_type) = explicit {
        return source_type;
    }
    let source = source.trim().to_ascii_lowercase();
    if (source.starts_with("http://") || source.starts_with("https://"))
        && !source.contains("github.com/")
    {
        SkillInstallSourceType::Url
    } else {
        SkillInstallSourceType::Github
    }
}

/// Build one SkillManager that mirrors LuaEngine's root-relative lifecycle layout.
/// 构造一个与 LuaEngine 根目录相对生命周期布局保持一致的 SkillManager。
fn build_skill_manager_for_root(
    root: &RuntimeSkillRoot,
    host_options: &LuaRuntimeHostOptions,
) -> Result<SkillManager, (i64, String)> {
    let runtime_root = root
        .skills_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.skills_dir.clone());
    let lifecycle_root = runtime_root.join(host_options.state_dir_name.as_str());
    let download_cache_root = host_options.download_cache_root.clone().unwrap_or_else(|| {
        host_options
            .temp_dir
            .clone()
            .unwrap_or_else(|| runtime_root.join("temp"))
            .join("downloads")
    });
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root: root.clone(),
        lifecycle_root,
        download_cache_root,
        allow_network_download: host_options.allow_network_download,
        github_base_url: host_options.github_base_url.clone(),
        github_api_base_url: host_options.github_api_base_url.clone(),
    }))
}

/// Render one managed install record into the skill-manager list output.
/// 将单条受管安装记录渲染到 skill-manager 列表输出中。
fn render_skill_install_record(rendered: &mut String, record: &InstalledSkillRecord) {
    writeln!(rendered, "- managed: {}", record.managed).expect("writing to String should not fail");
    writeln!(rendered, "- version: {}", record.version).expect("writing to String should not fail");
    writeln!(
        rendered,
        "- source: {} {}",
        render_skill_install_source_type(record.source.source_type),
        record.source.locator
    )
    .expect("writing to String should not fail");
    if let Some(tag) = record.source.tag.as_deref() {
        writeln!(rendered, "- source_tag: {}", tag).expect("writing to String should not fail");
    }
    writeln!(
        rendered,
        "- installed_at_unix_ms: {}",
        record.installed_at_unix_ms
    )
    .expect("writing to String should not fail");
}

/// Render one skill install source type as a stable snake-case string.
/// 将单个技能安装来源类型渲染为稳定的蛇形命名字符串。
fn render_skill_install_source_type(source_type: SkillInstallSourceType) -> &'static str {
    match source_type {
        SkillInstallSourceType::Github => "github",
        SkillInstallSourceType::Url => "url",
    }
}

/// Render the current explicit URL-install unsupported result before LuaSkills sees the request.
/// 在 LuaSkills 接收请求前渲染当前 URL 安装不支持的明确结果。
fn render_skill_url_install_not_implemented_result() -> ToolCallResult {
    ToolCallResult {
        content: vec![TextContent::text(
            "skill-manager install failed: managed URL install is not implemented yet; GitHub install is currently the only supported install source.",
        )],
        is_error: Some(true),
    }
}

/// Render one install or update operation result into an MCP tool result.
/// 将单个安装或更新操作结果渲染为 MCP 工具结果。
fn render_skill_apply_tool_result(
    action: &str,
    result: Result<SkillApplyResult, String>,
) -> ToolCallResult {
    match result {
        Ok(result) => ToolCallResult {
            content: vec![TextContent::text(&render_skill_apply_result(
                action, &result,
            ))],
            is_error: None,
        },
        Err(error) => ToolCallResult {
            content: vec![TextContent::text(&format!(
                "skill-manager {} failed: {}",
                action, error
            ))],
            is_error: Some(true),
        },
    }
}

/// Render one successful install or update operation result as compact Markdown.
/// 将单个成功安装或更新操作结果渲染为紧凑 Markdown。
fn render_skill_apply_result(action: &str, result: &SkillApplyResult) -> String {
    let mut rendered = String::new();
    writeln!(&mut rendered, "# skill-manager {}", action)
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- layer: USER").expect("writing to String should not fail");
    writeln!(&mut rendered, "- skill_id: {}", result.skill_id)
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- status: {}", result.status)
        .expect("writing to String should not fail");
    if let Some(version) = result.version.as_deref() {
        writeln!(&mut rendered, "- version: {}", version)
            .expect("writing to String should not fail");
    }
    if let Some(source_type) = result.source_type {
        writeln!(
            &mut rendered,
            "- source_type: {}",
            render_skill_install_source_type(source_type)
        )
        .expect("writing to String should not fail");
    }
    if let Some(source_locator) = result.source_locator.as_deref() {
        writeln!(&mut rendered, "- source: {}", source_locator)
            .expect("writing to String should not fail");
    }
    writeln!(&mut rendered, "- message: {}", result.message)
        .expect("writing to String should not fail");
    rendered
}

/// Render one uninstall operation result into an MCP tool result.
/// 将单个卸载操作结果渲染为 MCP 工具结果。
fn render_skill_uninstall_tool_result(
    result: Result<SkillUninstallResult, String>,
) -> ToolCallResult {
    match result {
        Ok(result) => ToolCallResult {
            content: vec![TextContent::text(&render_skill_uninstall_result(&result))],
            is_error: None,
        },
        Err(error) => ToolCallResult {
            content: vec![TextContent::text(&format!(
                "skill-manager uninstall failed: {}",
                error
            ))],
            is_error: Some(true),
        },
    }
}

/// Render one successful uninstall operation result as compact Markdown.
/// 将单个成功卸载操作结果渲染为紧凑 Markdown。
fn render_skill_uninstall_result(result: &SkillUninstallResult) -> String {
    let mut rendered = String::new();
    writeln!(&mut rendered, "# skill-manager uninstall")
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- layer: USER").expect("writing to String should not fail");
    writeln!(&mut rendered, "- skill_id: {}", result.skill_id)
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- skill_removed: {}", result.skill_removed)
        .expect("writing to String should not fail");
    writeln!(&mut rendered, "- sqlite_removed: {}", result.sqlite_removed)
        .expect("writing to String should not fail");
    writeln!(
        &mut rendered,
        "- lancedb_removed: {}",
        result.lancedb_removed
    )
    .expect("writing to String should not fail");
    writeln!(
        &mut rendered,
        "- sqlite_retained: {}",
        result.sqlite_retained
    )
    .expect("writing to String should not fail");
    writeln!(
        &mut rendered,
        "- lancedb_retained: {}",
        result.lancedb_retained
    )
    .expect("writing to String should not fail");
    writeln!(&mut rendered, "- message: {}", result.message)
        .expect("writing to String should not fail");
    rendered
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

    /// Write one minimal enabled LuaSkill fixture into a specific skills root.
    /// 将一个最小可启用 LuaSkill 夹具写入指定 skills 根目录。
    fn write_minimal_skill_to_root(skill_root: &std::path::Path, skill_id: &str) -> PathBuf {
        let skill_dir = skill_root.join(skill_id);
        std::fs::create_dir_all(skill_dir.join("runtime"))
            .expect("minimal skill runtime directory should be created");
        std::fs::write(
            skill_dir.join("skill.yaml"),
            format!(
                "name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nentries:\n  - name: ping\n    description: Minimal ping entry.\n    lua_entry: runtime/ping.lua\n    lua_module: {skill_id}.ping\n"
            ),
        )
        .expect("minimal skill manifest should be written");
        std::fs::write(
            skill_dir.join("runtime").join("ping.lua"),
            "return function(args)\n  return 'ok'\nend\n",
        )
        .expect("minimal skill runtime entry should be written");
        skill_dir
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

    /// Skill-manager arguments should parse without any layer selector.
    /// 不携带任何层级选择器的 skill-manager 参数应能正常解析。
    #[test]
    fn skill_manager_arguments_parse_without_layer() {
        let request = parse_skill_manager_tool_arguments(&json!({
            "action": "list"
        }))
        .expect("skill-manager arguments should parse");

        assert!(request.source.is_none());
    }

    /// Explicit layer arguments should be rejected because skill-manager is locked to USER.
    /// 显式层级参数应被拒绝，因为 skill-manager 已固定到 USER。
    #[test]
    fn skill_manager_arguments_reject_layer() {
        let error = parse_skill_manager_tool_arguments(&json!({
            "action": "list",
            "layer": "ROOT"
        }))
        .expect_err("skill-manager should reject layer arguments");

        assert_eq!(error.0, -32602);
        assert!(error.1.contains("does not accept a layer parameter"));
    }

    /// USER selection should ignore other formal layers and return only the user root.
    /// USER 选择应忽略其他正式层级，只返回用户根。
    #[test]
    fn skill_manager_user_selection_uses_only_user_layer() {
        let roots = vec![
            RuntimeSkillRoot {
                name: "ROOT".to_string(),
                skills_dir: PathBuf::from("D:/runtime/skills"),
            },
            RuntimeSkillRoot {
                name: "PROJECT".to_string(),
                skills_dir: PathBuf::from("D:/project/skills"),
            },
            RuntimeSkillRoot {
                name: "USER".to_string(),
                skills_dir: PathBuf::from("D:/user/skills"),
            },
        ];

        let user_root = select_skill_manager_user_root(&roots).expect("USER layer should resolve");

        assert_eq!(user_root.name, "USER");
    }

    /// Skill-manager uninstall should inject USER as the lifecycle target when ROOT shadows the same skill id.
    /// 当 ROOT 遮蔽同名技能时，skill-manager 卸载应将 USER 注入为生命周期目标。
    #[test]
    fn skill_manager_uninstall_forces_user_target_when_root_shadows_skill() {
        let runtime_root = unique_test_dir("skill-manager-user-target");
        std::fs::create_dir_all(&runtime_root).expect("runtime root should be created");
        let root_layer = RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: runtime_root.join("root-space").join("skills"),
        };
        let user_layer = RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: runtime_root.join("user-space").join("skills"),
        };
        let skill_id = "user-shadow-skill";
        let root_skill_dir = write_minimal_skill_to_root(&root_layer.skills_dir, skill_id);
        let user_skill_dir = write_minimal_skill_to_root(&user_layer.skills_dir, skill_id);
        let config = Config {
            runtime_root: Some(runtime_root.to_string_lossy().to_string()),
            ..Config::default()
        };
        let server = McpServer::new()
            .with_lua_skills(
                &config,
                &[root_layer.clone(), user_layer.clone()],
                LuaVmPoolConfig {
                    min_size: 1,
                    max_size: 1,
                    idle_ttl_secs: 60,
                },
                ToolCacheConfig::default(),
            )
            .expect("server should load shadowed skill roots");
        let lifecycle_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<
            RuntimeSkillLifecycleEvent,
        >::new()));
        let lifecycle_events_callback = lifecycle_events.clone();
        set_skill_lifecycle_callback(Some(std::sync::Arc::new(
            move |event: &RuntimeSkillLifecycleEvent| {
                lifecycle_events_callback
                    .lock()
                    .expect("lifecycle events should not be poisoned")
                    .push(event.clone());
            },
        )));
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
                        "name": "skill-manager",
                        "arguments": {
                            "action": "uninstall",
                            "skill_id": skill_id
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager uninstall should return one response");
        let tool_result: ToolCallResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .expect("skill-manager uninstall should return result"),
        )
        .expect("skill-manager uninstall result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert_eq!(tool_result.is_error, None);
        assert!(rendered.contains("- layer: USER"));
        assert!(
            root_skill_dir.exists(),
            "ROOT skill should remain untouched by USER-locked uninstall"
        );
        assert!(
            !user_skill_dir.exists(),
            "USER skill should be removed even when ROOT owns the effective skill id"
        );
        let observed_events = lifecycle_events
            .lock()
            .expect("lifecycle events should not be poisoned");
        assert!(
            observed_events.iter().any(|event| {
                event.plane == luaskills::SkillOperationPlane::Skills
                    && event.root_name.as_deref() == Some("USER")
                    && event.skill_id == skill_id
            }),
            "skill-manager USER target should execute through the ordinary Skills plane"
        );
        drop(observed_events);
        set_skill_lifecycle_callback(None);
        let _ = std::fs::remove_dir_all(&runtime_root);
    }

    /// Layer arguments should be rejected before lifecycle dispatch.
    /// 层级参数应在生命周期分发前被拒绝。
    #[test]
    fn skill_manager_layer_parameter_returns_json_rpc_error() {
        let server = McpServer::new();
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
                        "name": "skill-manager",
                        "arguments": {
                            "action": "install",
                            "layer": "ROOT",
                            "source": "LuaSkills/vulcan-codekit"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager layer install should return one response");

        let message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(message.contains("does not accept a layer parameter"));
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

    /// Minimal servers without a Lua engine should expose default host tools but hide Lua help wrappers.
    /// 未加载 Lua 引擎的最小服务应暴露默认宿主工具，但隐藏 Lua help 包装工具。
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
        assert!(tool_names.contains("skill-manager"));
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

    /// URL installs should fail with the wrapper's explicit unsupported-source message.
    /// URL 安装应使用包装层明确的不支持来源提示失败。
    #[test]
    fn skill_manager_url_install_reports_not_implemented() {
        let server = McpServer::new();
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
                        "name": "skill-manager",
                        "arguments": {
                            "action": "install",
                            "source_type": "url",
                            "source": "https://example.test/vulcan-codekit.source.yaml"
                        }
                    }
                }),
                RequestContext::default(),
            ))
            .expect("skill-manager URL install should return one response");

        assert!(
            response.get("error").is_none(),
            "URL install should return a tool-level error, not JSON-RPC error: {response}"
        );
        let tool_result: ToolCallResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .expect("skill-manager URL install should return result"),
        )
        .expect("skill-manager URL install result should deserialize");
        let rendered = tool_result
            .content
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();

        assert_eq!(tool_result.is_error, Some(true));
        assert!(rendered.contains("managed URL install is not implemented yet"));
        assert!(!rendered.contains("requires skill_id"));
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
        inner.skill_entries.remove(removed_name);
    }
    for entry in &delta.updated_entries {
        insert_skill_entry(inner, entry.clone());
    }
    for entry in &delta.added_entries {
        insert_skill_entry(inner, entry.clone());
    }
}

/// Insert one LuaSkills runtime entry into the dynamic registry while rejecting host-reserved name collisions.
/// 将单个 LuaSkills 运行时入口插入动态注册表，并拒绝与宿主保留名称发生冲突。
fn insert_skill_entry(inner: &mut ServerInner, entry: RuntimeEntryDescriptor) {
    let tool = map_runtime_entry_to_mcp_tool(&entry);
    if inner.host_tools.contains_key(&tool.name) {
        eprintln!(
            "[LuaSkills] Skip dynamic tool '{}' because it collides with a host-owned tool",
            tool.name
        );
        inner.skill_tools.remove(&tool.name);
        inner.skill_entries.remove(&tool.name);
        return;
    }
    inner.skill_entries.insert(tool.name.clone(), entry);
    inner.skill_tools.insert(tool.name.clone(), tool);
}

/// Render one structured help detail payload into user-facing Markdown.
/// 把一份结构化帮助详情载荷渲染成面向用户的 Markdown 文本。
fn render_help_detail_markdown(detail: &RuntimeHelpDetail) -> String {
    detail.content.clone()
}
