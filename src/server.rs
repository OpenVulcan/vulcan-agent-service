use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::Mutex;

use crate::client_budget::reload_client_budget_config;
use crate::grpc_client::VmmClient;
use crate::config::Config;
use crate::luaskills_host::{
    build_luaskills_engine_options, build_runtime_invocation_context, build_runtime_request_context,
    client_budget_snapshot_for_render, install_luaskills_log_callback, map_runtime_entry_to_mcp_tool,
    normalize_skill_root_key, resolve_runtime_root_from_config, validate_unique_skill_root_spaces,
};
use crate::protocol::*;
use crate::temp_maintenance::ensure_runtime_temp_dir;
use crate::tool_config::reload_tool_configs;
use crate::tool_result_format::{HostRenderOptions, render_tool_result_text};
use vulcan_luaskills::{
    LuaEngine, LuaEngineOptions, LuaVmPoolConfig, RuntimeEntryRegistryDelta, RuntimeHelpDetail,
    RuntimeSkillLifecycleEvent, RuntimeSkillLifecycleCallback, RuntimeSkillHelpDescriptor,
    RuntimeSkillRoot,
    SkillUninstallOptions, ToolCacheConfig, set_entry_registry_callback,
    set_skill_lifecycle_callback,
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
    lua_environment_registry_dir: Option<PathBuf>,
    lua_project_environments: Arc<StdRwLock<HashMap<String, LuaProjectEnvironment>>>,
}

#[derive(Clone)]
struct LuaProjectEnvironment {
    environment_id: String,
    skill_roots: Vec<RuntimeSkillRoot>,
    engine: Arc<StdRwLock<LuaEngine>>,
}

/// English: Persisted project-environment record stored under the host runtime state directory.
/// 存放在宿主运行时状态目录中的项目环境持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedProjectEnvironmentRecord {
    environment_id: String,
    skills_dir: String,
}

struct ServerInner {
    /// English: Host-owned MCP tools registered by the current host adapter and never mutated by LuaSkills runtime deltas.
    /// 当前宿主适配层拥有的 MCP 工具注册表，不会被 LuaSkills 运行时差异事件修改。
    host_tools: HashMap<String, Tool>,
    /// English: LuaSkills-managed dynamic MCP tools derived from runtime entries and fully driven by runtime registry deltas.
    /// 由 LuaSkills 运行时入口派生并完全受运行时注册表差异驱动的动态 MCP 工具注册表。
    skill_tools: HashMap<String, Tool>,
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
            lua_environment_registry_dir: None,
            lua_project_environments: Arc::new(StdRwLock::new(HashMap::new())),
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
        self.lua_environment_registry_dir = resolve_runtime_root_from_config(config)
            .map(|runtime_root| runtime_root.join("state").join("environments"));

        let callback_inner = self.inner.clone();
        set_entry_registry_callback(Some(Arc::new(move |delta: &RuntimeEntryRegistryDelta| {
            let mut inner = callback_inner.blocking_lock();
            apply_runtime_entry_registry_delta(&mut inner, delta);
        })));
        let lifecycle_callback: RuntimeSkillLifecycleCallback =
            Arc::new(|event: &RuntimeSkillLifecycleEvent| {
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
            });
        set_skill_lifecycle_callback(Some(lifecycle_callback));

        // Register Lua skills strictly as MCP tools.
        // 严格仅将 Lua skills 注册为 MCP tools。
        {
            let mut inner = self.inner.try_lock().unwrap();
            for entry in entries {
                let tool = map_runtime_entry_to_mcp_tool(&entry);
                insert_skill_tool(&mut inner, tool);
            }
        }

        self.restore_persisted_project_environments()?;

        Ok(self)
    }

    fn register_defaults(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- vulcan-help-list: list strict LuaSkills help trees for host-side help wrappers ---
        inner.host_tools.insert(
            "vulcan-help-list".to_string(),
            Tool::with_annotations(
                "vulcan-help-list",
                "List all registered strict LuaSkills help trees and their available flow descriptions. This MCP wrapper renders host-side Markdown from lib/system structured help data.",
                json!({
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When omitted, the default environment is used."}
                }),
                vec![],
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-skill-list".to_string(),
            Tool::with_annotations(
                "vulcan-skill-list",
                "List currently effective LuaSkills packages together with their resolved root name and physical skill directory so host or IDE integrations can understand which concrete skill instance is active.",
                json!({
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When omitted, the default environment is used."}
                }),
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
                "Read one strict LuaSkills help flow from lib/system help data and render it as Markdown for MCP clients. Use flow=`main` to read the skill main help node.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit` or `vulcan-runtime`."},
                    "flow": {"type": "string", "description": "Help flow name. Use `main` for the skill main help node, or pass one declared workflow/topic name."},
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When omitted, the default environment is used."}
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

        inner.host_tools.insert(
            "vulcan-environment-init".to_string(),
            Tool::with_annotations(
                "vulcan-environment-init",
                "Initialize or refresh one explicit project environment. The project environment root becomes the highest-priority named skill root and is layered above the default root chain.",
                json!({
                    "environment_id": {"type": "string", "description": "Stable project environment id, for example `project-a` or `workspace/foo`."},
                    "skills_dir": {"type": "string", "description": "Physical skills directory of the project environment, for example `D:/project/.vulcan/luaskills`."}
                }),
                vec!["environment_id".to_string(), "skills_dir".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-environment-list".to_string(),
            Tool::with_annotations(
                "vulcan-environment-list",
                "List initialized project environments together with their ordered skill roots.",
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

        inner.host_tools.insert(
            "vulcan-environment-reload".to_string(),
            Tool::with_annotations(
                "vulcan-environment-reload",
                "Reload one explicit project environment from its persisted or active skills directory and rebuild its effective LuaSkills runtime view.",
                json!({
                    "environment_id": {"type": "string", "description": "Stable project environment id to reload."}
                }),
                vec!["environment_id".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-environment-remove".to_string(),
            Tool::with_annotations(
                "vulcan-environment-remove",
                "Remove one explicit project environment from the active registry and its persisted environment record. The physical project skills directory is retained by default.",
                json!({
                    "environment_id": {"type": "string", "description": "Stable project environment id to remove."}
                }),
                vec!["environment_id".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-environment-inspect".to_string(),
            Tool::with_annotations(
                "vulcan-environment-inspect",
                "Inspect one explicit project environment, including its ordered skill roots and currently effective skills.",
                json!({
                    "environment_id": {"type": "string", "description": "Stable project environment id to inspect."}
                }),
                vec!["environment_id".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-skill-enable".to_string(),
            Tool::with_annotations(
                "vulcan-skill-enable",
                "Enable one non-protected LuaSkills package through the ordinary skills plane and let the MCP host refresh its registered tools automatically.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit`."},
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When provided, the operation runs against that project environment instead of the default environment."}
                }),
                vec!["skill".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-skill-disable".to_string(),
            Tool::with_annotations(
                "vulcan-skill-disable",
                "Disable one non-protected LuaSkills package through the ordinary skills plane and let the MCP host refresh its registered tools automatically.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit`."},
                    "reason": {"type": "string", "description": "Optional disable reason recorded into the skill state marker."},
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When provided, the operation runs against that project environment instead of the default environment."}
                }),
                vec!["skill".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-skill-uninstall".to_string(),
            Tool::with_annotations(
                "vulcan-skill-uninstall",
                "Uninstall one non-protected LuaSkills package through the ordinary skills plane and let the MCP host refresh its registered tools automatically. SQLite and LanceDB data are retained by default unless explicit removal flags are set.",
                json!({
                    "skill": {"type": "string", "description": "Target skill id, for example `vulcan-codekit`."},
                    "remove_sqlite": {"type": "boolean", "description": "When true, also remove the skill-owned SQLite database directory. Default false."},
                    "remove_lancedb": {"type": "boolean", "description": "When true, also remove the skill-owned LanceDB database directory. Default false."},
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When provided, the operation runs against that project environment instead of the default environment."}
                }),
                vec!["skill".to_string()],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(false),
                },
            ),
        );

        inner.host_tools.insert(
            "vulcan-skill-reload".to_string(),
            Tool::with_annotations(
                "vulcan-skill-reload",
                "Reload LuaSkills from the current base and override directories, then let the MCP host refresh its registered tools from runtime entry deltas.",
                json!({
                    "environment_id": {"type": "string", "description": "Optional explicit environment id. When provided, the operation reloads that project environment instead of the default environment."}
                }),
                vec![],
                ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            ),
        );

        // --- reload_vulcan_mcp_configs: hot reload runtime client budget / tool config files ---
        inner.host_tools.insert(
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

    /// English: Resolve the Lua engine for one optional project environment id, falling back to the default environment.
    /// 根据可选项目环境标识解析对应的 Lua 引擎，缺省时回退到默认环境。
    fn resolve_lua_engine_for_environment(
        &self,
        environment_id: Option<&str>,
    ) -> Option<Arc<StdRwLock<LuaEngine>>> {
        let normalized = environment_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match normalized {
            None => self.lua_engine.clone(),
            Some(environment_id) => self
                .lua_project_environments
                .read()
                .ok()
                .and_then(|registry| registry.get(environment_id).cloned())
                .map(|environment| environment.engine),
        }
    }

    /// English: Resolve the target Lua engine together with the effective skill-root chain for one optional environment id.
    /// 为一个可选环境标识解析目标 Lua 引擎及其对应的有效技能根目录链。
    fn resolve_lua_runtime_target(
        &self,
        environment_id: Option<&str>,
    ) -> Result<(Arc<StdRwLock<LuaEngine>>, Vec<RuntimeSkillRoot>), (i64, String)> {
        let normalized = environment_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match normalized {
            None => {
                let engine = self.lua_engine.as_ref().ok_or_else(|| {
                    (-32603, "Lua engine not configured. Add skills directory.".to_string())
                })?;
                let skill_roots = self.lua_skill_roots.as_ref().ok_or_else(|| {
                    (-32603, "Lua skill roots are not configured.".to_string())
                })?;
                Ok((engine.clone(), skill_roots.clone()))
            }
            Some(environment_id) => {
                let environment = self
                    .lua_project_environments
                    .read()
                    .ok()
                    .and_then(|registry| registry.get(environment_id).cloned())
                    .ok_or_else(|| {
                        (
                            -32602,
                            format!("Lua project environment '{}' is not initialized.", environment_id),
                        )
                    })?;
                Ok((environment.engine, environment.skill_roots))
            }
        }
    }

    /// English: Initialize or refresh one explicit project environment and return its resolved descriptor.
    /// 初始化或刷新单个显式项目环境，并返回其已解析描述信息。
    async fn initialize_project_environment(
        &self,
        environment_id: &str,
        skills_dir: &str,
    ) -> Result<LuaProjectEnvironment, (i64, String)> {
        let environment_id = environment_id.trim();
        let skills_dir = skills_dir.trim();
        if environment_id.is_empty() {
            return Err((-32602, "environment_id must not be empty".to_string()));
        }
        if skills_dir.is_empty() {
            return Err((-32602, "skills_dir must not be empty".to_string()));
        }

        let project_skills_dir = std::path::PathBuf::from(skills_dir);
        let environment_id_owned = environment_id.to_string();
        let server = self.clone();
        let project_skills_dir_for_build = project_skills_dir.clone();
        let environment = tokio::task::spawn_blocking(move || {
            server.build_project_environment_sync(
                &environment_id_owned,
                &project_skills_dir_for_build,
                true,
            )
        })
        .await
        .map_err(|error| {
            (
                -32603,
                format!("Project environment init spawn error: {}", error),
            )
        })?
        .map_err(|error| (-32603, error))?;

        self.persist_project_environment_record(environment_id, &project_skills_dir)
            .map_err(|error| (-32603, error))?;
        self.lua_project_environments
            .write()
            .map_err(|_| (-32603, "Project environment registry lock poisoned.".to_string()))?
            .insert(environment_id.to_string(), environment.clone());
        Ok(environment)
    }

    /// English: Return all initialized project environments sorted by environment id.
    /// 返回按环境标识排序后的全部已初始化项目环境。
    fn list_project_environments(&self) -> Vec<LuaProjectEnvironment> {
        let mut environments = self
            .lua_project_environments
            .read()
            .ok()
            .map(|registry| registry.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        environments.sort_by(|left, right| left.environment_id.cmp(&right.environment_id));
        environments
    }

    /// English: Return the state-directory root that stores persisted project-environment records.
    /// 返回用于存放项目环境持久化记录的状态目录根路径。
    fn environment_registry_dir(&self) -> Result<PathBuf, String> {
        self.lua_environment_registry_dir
            .clone()
            .ok_or_else(|| "Lua environment registry directory is not initialized.".to_string())
    }

    /// English: Persist one project-environment record so the host can restore it on the next startup.
    /// 持久化一份项目环境记录，便于宿主在下次启动时恢复该环境。
    fn persist_project_environment_record(
        &self,
        environment_id: &str,
        skills_dir: &Path,
    ) -> Result<(), String> {
        let registry_dir = self.environment_registry_dir()?;
        fs::create_dir_all(&registry_dir).map_err(|error| {
            format!(
                "Failed to create environment registry directory {}: {}",
                registry_dir.display(),
                error
            )
        })?;
        let record = PersistedProjectEnvironmentRecord {
            environment_id: environment_id.to_string(),
            skills_dir: skills_dir.display().to_string(),
        };
        let record_path = registry_dir.join(environment_record_file_name(environment_id));
        let content = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("Failed to serialize project environment record: {}", error))?;
        fs::write(&record_path, content).map_err(|error| {
            format!(
                "Failed to write project environment record {}: {}",
                record_path.display(),
                error
            )
        })
    }

    /// English: Delete one persisted project-environment record by environment id.
    /// 按环境标识删除单条项目环境持久化记录。
    fn remove_project_environment_record(&self, environment_id: &str) -> Result<bool, String> {
        let registry_dir = self.environment_registry_dir()?;
        let record_path = registry_dir.join(environment_record_file_name(environment_id));
        if !record_path.exists() {
            return Ok(false);
        }
        fs::remove_file(&record_path).map_err(|error| {
            format!(
                "Failed to remove project environment record {}: {}",
                record_path.display(),
                error
            )
        })?;
        Ok(true)
    }

    /// English: Load all persisted project-environment records from the current host state directory.
    /// 从当前宿主状态目录加载全部项目环境持久化记录。
    fn load_persisted_project_environment_records(
        &self,
    ) -> Result<Vec<PersistedProjectEnvironmentRecord>, String> {
        let registry_dir = self.environment_registry_dir()?;
        if !registry_dir.exists() {
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        for entry in fs::read_dir(&registry_dir).map_err(|error| {
            format!(
                "Failed to read environment registry directory {}: {}",
                registry_dir.display(),
                error
            )
        })? {
            let entry = entry.map_err(|error| {
                format!(
                    "Failed to iterate environment registry directory {}: {}",
                    registry_dir.display(),
                    error
                )
            })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let content = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "Failed to read project environment record {}: {}",
                    path.display(),
                    error
                )
            })?;
            let record: PersistedProjectEnvironmentRecord = serde_json::from_str(&content)
                .map_err(|error| {
                    format!(
                        "Failed to parse project environment record {}: {}",
                        path.display(),
                        error
                    )
                })?;
            records.push(record);
        }
        records.sort_by(|left, right| left.environment_id.cmp(&right.environment_id));
        Ok(records)
    }

    /// English: Restore persisted project environments whose physical skills directories still exist.
    /// 恢复物理技能目录仍然存在的已持久化项目环境。
    fn restore_persisted_project_environments(&self) -> Result<(), Box<dyn std::error::Error>> {
        let records = self.load_persisted_project_environment_records()?;
        for record in records {
            let skills_dir = PathBuf::from(&record.skills_dir);
            if !skills_dir.exists() {
                eprintln!(
                    "[LuaSkills] Skip restoring environment '{}' because skills dir does not exist: {}",
                    record.environment_id,
                    skills_dir.display()
                );
                continue;
            }
            match self.build_project_environment_sync(&record.environment_id, &skills_dir, false) {
                Ok(environment) => {
                    if let Ok(mut registry) = self.lua_project_environments.write() {
                        registry.insert(record.environment_id.clone(), environment);
                    }
                }
                Err(error) => {
                    eprintln!(
                        "[LuaSkills] Failed to restore persisted environment '{}': {}",
                        record.environment_id, error
                    );
                }
            }
        }
        Ok(())
    }

    /// English: Build one project environment synchronously from the current default root chain and host engine options.
    /// 基于当前默认根链与宿主引擎选项同步构建单个项目环境。
    fn build_project_environment_sync(
        &self,
        environment_id: &str,
        skills_dir: &Path,
        create_if_missing: bool,
    ) -> Result<LuaProjectEnvironment, String> {
        let engine_options = self
            .lua_engine_options
            .clone()
            .ok_or_else(|| "Lua engine options are not initialized.".to_string())?;
        let default_roots = self
            .lua_skill_roots
            .clone()
            .ok_or_else(|| "Default Lua skill roots are not initialized.".to_string())?;
        if create_if_missing {
            fs::create_dir_all(skills_dir).map_err(|error| {
                format!(
                    "Failed to create project skills directory {}: {}",
                    skills_dir.display(),
                    error
                )
            })?;
        } else if !skills_dir.exists() {
            return Err(format!(
                "Project skills directory does not exist: {}",
                skills_dir.display()
            ));
        }

        let mut project_roots = vec![RuntimeSkillRoot {
            name: environment_id.to_string(),
            skills_dir: skills_dir.to_path_buf(),
        }];
        let mut seen_root_keys = std::collections::HashSet::new();
        seen_root_keys.insert(normalize_skill_root_key(skills_dir));
        for root in default_roots {
            let normalized_key = normalize_skill_root_key(&root.skills_dir);
            if seen_root_keys.insert(normalized_key) {
                project_roots.push(root);
            }
        }
        validate_unique_skill_root_spaces(&project_roots).map_err(|error| {
            format!(
                "invalid project environment root chain for '{}': {} / 项目环境 '{}' 的技能根目录链无效：{}",
                environment_id, error, environment_id, error
            )
        })?;

        let mut engine = LuaEngine::new(engine_options).map_err(|error| error.to_string())?;
        engine
            .load_from_roots(&project_roots)
            .map_err(|error| error.to_string())?;

        Ok(LuaProjectEnvironment {
            environment_id: environment_id.to_string(),
            skill_roots: project_roots,
            engine: Arc::new(StdRwLock::new(engine)),
        })
    }

    /// English: Reload one explicit project environment and update its persisted host record.
    /// 重新加载单个显式项目环境，并同步更新其宿主持久化记录。
    async fn reload_project_environment(
        &self,
        environment_id: &str,
    ) -> Result<LuaProjectEnvironment, (i64, String)> {
        let environment_id = environment_id.trim();
        if environment_id.is_empty() {
            return Err((-32602, "environment_id must not be empty".to_string()));
        }

        let skills_dir = self
            .lua_project_environments
            .read()
            .ok()
            .and_then(|registry| registry.get(environment_id).cloned())
            .and_then(|environment| environment.skill_roots.first().cloned())
            .map(|root| root.skills_dir)
            .or_else(|| {
                self.load_persisted_project_environment_records()
                    .ok()
                    .and_then(|records| {
                        records
                            .into_iter()
                            .find(|record| record.environment_id == environment_id)
                            .map(|record| PathBuf::from(record.skills_dir))
                    })
            })
            .ok_or_else(|| (-32602, format!("Environment '{}' is not registered.", environment_id)))?;

        let environment_id_owned = environment_id.to_string();
        let environment = {
            let skills_dir_clone = skills_dir.clone();
            let server = self.clone();
            tokio::task::spawn_blocking(move || {
                server.build_project_environment_sync(
                    &environment_id_owned,
                    &skills_dir_clone,
                    false,
                )
            })
            .await
            .map_err(|error| {
                (
                    -32603,
                    format!("Project environment reload spawn error: {}", error),
                )
            })?
            .map_err(|error| (-32603, error))?
        };

        self.persist_project_environment_record(environment_id, &skills_dir)
            .map_err(|error| (-32603, error))?;
        self.lua_project_environments
            .write()
            .map_err(|_| (-32603, "Project environment registry lock poisoned.".to_string()))?
            .insert(environment_id.to_string(), environment.clone());
        Ok(environment)
    }

    /// English: Remove one explicit project environment from memory and persistence without touching the physical skills directory.
    /// 从内存与持久化中移除单个显式项目环境，但不触碰实际技能目录。
    fn remove_project_environment(
        &self,
        environment_id: &str,
    ) -> Result<(Option<LuaProjectEnvironment>, bool), (i64, String)> {
        let environment_id = environment_id.trim();
        if environment_id.is_empty() {
            return Err((-32602, "environment_id must not be empty".to_string()));
        }
        let removed = self
            .lua_project_environments
            .write()
            .map_err(|_| (-32603, "Project environment registry lock poisoned.".to_string()))?
            .remove(environment_id);
        let record_removed = self
            .remove_project_environment_record(environment_id)
            .map_err(|error| (-32603, error))?;
        Ok((removed, record_removed))
    }

    /// English: Inspect one explicit project environment and return its resolved descriptor together with effective skills.
    /// 检查单个显式项目环境，并返回其已解析描述及当前生效技能列表。
    fn inspect_project_environment(
        &self,
        environment_id: &str,
    ) -> Result<(LuaProjectEnvironment, Vec<RuntimeSkillHelpDescriptor>), (i64, String)> {
        let environment_id = environment_id.trim();
        if environment_id.is_empty() {
            return Err((-32602, "environment_id must not be empty".to_string()));
        }
        let environment = self
            .lua_project_environments
            .read()
            .ok()
            .and_then(|registry| registry.get(environment_id).cloned())
            .ok_or_else(|| (-32602, format!("Environment '{}' is not initialized.", environment_id)))?;
        let effective_skills = environment
            .engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .list_skill_help();
        Ok((environment, effective_skills))
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
            "vulcan-skill-enable" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let (engine, skill_roots) =
                    self.resolve_lua_runtime_target(environment_id.as_deref())?;
                let skill_id = required_string_argument(&args, "skill")?;
                let skill_id_for_call = skill_id.clone();
                tokio::task::spawn_blocking(move || {
                    let mut engine = engine
                        .write()
                        .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
                    engine
                        .enable_skill(&skill_roots, &skill_id_for_call)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| (-32603, format!("vulcan-skill-enable spawn error: {}", error)))?
                .map_err(|error| (-32603, error))?;
                ToolCallResult {
                    content: vec![TextContent::text(&format!("Skill '{}' enabled.", skill_id))],
                    is_error: None,
                }
            }

            "vulcan-skill-disable" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let (engine, skill_roots) =
                    self.resolve_lua_runtime_target(environment_id.as_deref())?;
                let skill_id = required_string_argument(&args, "skill")?;
                let reason = args
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let skill_id_for_call = skill_id.clone();
                tokio::task::spawn_blocking(move || {
                    let mut engine = engine
                        .write()
                        .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
                    engine
                        .disable_skill_in_roots(
                            &skill_roots,
                            &skill_id_for_call,
                            reason.as_deref(),
                        )
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| (-32603, format!("vulcan-skill-disable spawn error: {}", error)))?
                .map_err(|error| (-32603, error))?;
                ToolCallResult {
                    content: vec![TextContent::text(&format!("Skill '{}' disabled.", skill_id))],
                    is_error: None,
                }
            }

            "vulcan-skill-uninstall" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let (engine, skill_roots) =
                    self.resolve_lua_runtime_target(environment_id.as_deref())?;
                let skill_id = required_string_argument(&args, "skill")?;
                let remove_sqlite = optional_bool_argument(&args, "remove_sqlite", false)?;
                let remove_lancedb = optional_bool_argument(&args, "remove_lancedb", false)?;
                let skill_id_for_call = skill_id.clone();
                let uninstall_options = SkillUninstallOptions {
                    remove_sqlite,
                    remove_lancedb,
                };
                let uninstall_result = tokio::task::spawn_blocking(move || {
                    let mut engine = engine
                        .write()
                        .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
                    engine
                        .uninstall_skill(
                            &skill_roots,
                            &skill_id_for_call,
                            &uninstall_options,
                        )
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| (-32603, format!("vulcan-skill-uninstall spawn error: {}", error)))?
                .map_err(|error| (-32603, error))?;
                ToolCallResult {
                    content: vec![TextContent::text(&format!(
                        "Skill '{}' uninstalled.\n- SQLite removed: {}\n- SQLite retained: {}\n- LanceDB removed: {}\n- LanceDB retained: {}",
                        uninstall_result.skill_id,
                        uninstall_result.sqlite_removed,
                        uninstall_result.sqlite_retained,
                        uninstall_result.lancedb_removed,
                        uninstall_result.lancedb_retained
                    ))],
                    is_error: None,
                }
            }

            "vulcan-skill-reload" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let (engine, skill_roots) =
                    self.resolve_lua_runtime_target(environment_id.as_deref())?;
                tokio::task::spawn_blocking(move || {
                    let mut engine = engine
                        .write()
                        .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
                    engine
                        .reload_from_roots(&skill_roots)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| (-32603, format!("vulcan-skill-reload spawn error: {}", error)))?
                .map_err(|error| (-32603, error))?;
                ToolCallResult {
                    content: vec![TextContent::text("LuaSkills reloaded.")],
                    is_error: None,
                }
            }

            "vulcan-help-list" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let engine = self.resolve_lua_engine_for_environment(environment_id.as_deref()).ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured or environment not initialized.".to_string(),
                    )
                })?;
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

            "vulcan-skill-list" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let engine = self.resolve_lua_engine_for_environment(environment_id.as_deref()).ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured or environment not initialized.".to_string(),
                    )
                })?;
                let skill_tree = engine
                    .read()
                    .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
                    .list_skill_help();
                let markdown = render_skill_list_markdown(&skill_tree);
                ToolCallResult {
                    content: vec![TextContent::text(&markdown)],
                    is_error: None,
                }
            }

            "vulcan-environment-init" => {
                let environment_id = required_string_argument(&args, "environment_id")?;
                let skills_dir = required_string_argument(&args, "skills_dir")?;
                let initialized = self
                    .initialize_project_environment(&environment_id, &skills_dir)
                    .await?;
                ToolCallResult {
                    content: vec![TextContent::text(&render_environment_detail_markdown(&initialized))],
                    is_error: None,
                }
            }

            "vulcan-environment-list" => {
                let environments = self.list_project_environments();
                ToolCallResult {
                    content: vec![TextContent::text(&render_environment_list_markdown(&environments))],
                    is_error: None,
                }
            }

            "vulcan-environment-reload" => {
                let environment_id = required_string_argument(&args, "environment_id")?;
                let reloaded = self.reload_project_environment(&environment_id).await?;
                ToolCallResult {
                    content: vec![TextContent::text(&render_environment_detail_markdown(&reloaded))],
                    is_error: None,
                }
            }

            "vulcan-environment-remove" => {
                let environment_id = required_string_argument(&args, "environment_id")?;
                let (removed_environment, record_removed) =
                    self.remove_project_environment(&environment_id)?;
                let markdown = render_environment_remove_markdown(
                    &environment_id,
                    removed_environment.as_ref(),
                    record_removed,
                );
                ToolCallResult {
                    content: vec![TextContent::text(&markdown)],
                    is_error: None,
                }
            }

            "vulcan-environment-inspect" => {
                let environment_id = required_string_argument(&args, "environment_id")?;
                let (environment, effective_skills) =
                    self.inspect_project_environment(&environment_id)?;
                ToolCallResult {
                    content: vec![TextContent::text(&render_environment_inspect_markdown(
                        &environment,
                        &effective_skills,
                    ))],
                    is_error: None,
                }
            }

            "vulcan-help-detail" => {
                let environment_id = optional_string_argument(&args, "environment_id");
                let engine = self.resolve_lua_engine_for_environment(environment_id.as_deref()).ok_or_else(|| {
                    (
                        -32603,
                        "Lua engine not configured or environment not initialized.".to_string(),
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
                    let engine = engine_clone
                        .read()
                        .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
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

            _ => {
                // Check if this is a Lua skill
                if let Some(engine) = &self.lua_engine {
                    let environment_id = optional_string_argument(&args, "environment_id");
                    let target_engine = match environment_id.as_deref() {
                        Some(environment_id) => self
                            .resolve_lua_engine_for_environment(Some(environment_id))
                            .ok_or_else(|| {
                                (
                                    -32602,
                                    format!(
                                        "Lua project environment '{}' is not initialized.",
                                        environment_id
                                    ),
                                )
                            })?,
                        None => engine.clone(),
                    };
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
                        let mut args_clone = args.clone();
                        if let Some(object) = args_clone.as_object_mut() {
                            object.remove("environment_id");
                        }
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
                                .map_err(|_| "Lua engine lock poisoned / Lua 引擎锁已损坏".to_string())?;
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
                .and_then(|engine| {
                    engine
                        .read()
                        .ok()
                        .and_then(|engine| engine.prompt_argument_completions(ref_name, argument_name))
                })
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
        lines.push(format!(
            "- root: `{}`\n- dir: `{}`",
            skill_help.root_name, skill_help.skill_dir
        ));
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

/// Render one structured skill list payload into user-facing Markdown.
/// 把当前生效技能列表渲染成面向用户的 Markdown 文本。
fn render_skill_list_markdown(help_tree: &[RuntimeSkillHelpDescriptor]) -> String {
    if help_tree.is_empty() {
        return "# Vulcan Skill List\n\nNo LuaSkills packages are currently active.".to_string();
    }

    let mut lines = vec!["# Vulcan Skill List".to_string(), String::new()];
    for skill_help in help_tree {
        lines.push(format!("## `{}`", skill_help.skill_id));
        lines.push(format!("- root: `{}`", skill_help.root_name));
        lines.push(format!("- dir: `{}`", skill_help.skill_dir));
        if !skill_help.main.description.trim().is_empty() {
            lines.push(format!("- description: {}", skill_help.main.description.trim()));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// English: Return one required non-empty string argument from the current tool call payload.
/// 从当前工具调用参数中读取一个必填且非空的字符串参数。
fn required_string_argument(args: &Value, key: &str) -> Result<String, (i64, String)> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| (-32602, format!("Missing required parameter: {}", key)))
}

/// English: Return one optional non-empty string argument from the current tool call payload.
/// 从当前工具调用参数中读取一个可选且非空的字符串参数。
fn optional_string_argument(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

/// English: Return one optional boolean argument from the current tool call payload with a safe default.
/// 从当前工具调用参数中读取一个可选布尔参数，并在缺失时返回安全默认值。
fn optional_bool_argument(args: &Value, key: &str, default: bool) -> Result<bool, (i64, String)> {
    match args.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| (-32602, format!("Parameter '{}' must be boolean", key))),
        None => Ok(default),
    }
}

/// English: Apply one runtime entry-registry delta to the MCP host tool registry.
/// 把一份运行时入口注册表差异应用到 MCP 宿主工具注册表。
fn apply_runtime_entry_registry_delta(
    inner: &mut ServerInner,
    delta: &RuntimeEntryRegistryDelta,
) {
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

/// English: Insert one LuaSkills-managed tool into the dynamic registry while rejecting host-reserved name collisions.
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

/// Render initialized project environments into user-facing Markdown.
/// 把已初始化项目环境渲染成面向用户的 Markdown 文本。
fn render_environment_list_markdown(environments: &[LuaProjectEnvironment]) -> String {
    if environments.is_empty() {
        return "# Vulcan Environment List\n\nNo explicit project environments are currently initialized.".to_string();
    }

    let mut lines = vec!["# Vulcan Environment List".to_string(), String::new()];
    for environment in environments {
        lines.push(format!("## `{}`", environment.environment_id));
        for root in &environment.skill_roots {
            lines.push(format!("- `{}` => `{}`", root.name, root.skills_dir.display()));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

/// Render one initialized project environment into user-facing Markdown.
/// 把单个已初始化项目环境渲染成面向用户的 Markdown 文本。
fn render_environment_detail_markdown(environment: &LuaProjectEnvironment) -> String {
    let mut lines = vec![
        "# Vulcan Environment Initialized".to_string(),
        String::new(),
        format!("- environment_id: `{}`", environment.environment_id),
        "- roots:".to_string(),
    ];
    for root in &environment.skill_roots {
        lines.push(format!("  - `{}` => `{}`", root.name, root.skills_dir.display()));
    }
    lines.join("\n")
}

/// English: Render one project-environment removal result into user-facing Markdown.
/// 把单个项目环境移除结果渲染成面向用户的 Markdown 文本。
fn render_environment_remove_markdown(
    environment_id: &str,
    environment: Option<&LuaProjectEnvironment>,
    record_removed: bool,
) -> String {
    let mut lines = vec![
        "# Vulcan Environment Removed".to_string(),
        String::new(),
        format!("- environment_id: `{}`", environment_id),
        format!("- persisted_record_removed: `{}`", record_removed),
    ];
    if let Some(environment) = environment {
        if let Some(primary_root) = environment.skill_roots.first() {
            lines.push(format!(
                "- retained_skills_dir: `{}`",
                primary_root.skills_dir.display()
            ));
        }
    } else {
        lines.push("- active_environment_removed: `false`".to_string());
    }
    lines.push("- note: the physical skills directory is retained by default.".to_string());
    lines.join("\n")
}

/// English: Render one inspected project environment and its effective skills into user-facing Markdown.
/// 把单个项目环境及其当前生效技能渲染成面向用户的 Markdown 文本。
fn render_environment_inspect_markdown(
    environment: &LuaProjectEnvironment,
    effective_skills: &[RuntimeSkillHelpDescriptor],
) -> String {
    let mut lines = vec![
        "# Vulcan Environment Inspect".to_string(),
        String::new(),
        format!("- environment_id: `{}`", environment.environment_id),
        "- roots:".to_string(),
    ];
    for root in &environment.skill_roots {
        lines.push(format!("  - `{}` => `{}`", root.name, root.skills_dir.display()));
    }
    lines.push(String::new());
    lines.push("## Effective Skills".to_string());
    if effective_skills.is_empty() {
        lines.push("No skills are currently active in this environment.".to_string());
    } else {
        for skill in effective_skills {
            lines.push(format!(
                "- `{}` => root `{}`, dir `{}`",
                skill.skill_id, skill.root_name, skill.skill_dir
            ));
        }
    }
    lines.join("\n")
}

/// English: Build one deterministic record filename for the given environment id.
/// 为给定环境标识生成确定性的记录文件名。
fn environment_record_file_name(environment_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    environment_id.hash(&mut hasher);
    let hash = hasher.finish();
    let safe_name: String = environment_id
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect();
    format!("{}-{:016x}.json", safe_name, hash)
}
