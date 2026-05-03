use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::Mutex;

use crate::backends::vmm::grpc_client::VmmClient;
use crate::config::Config;
use crate::host_core::host_tools::{
    lua_help_detail_tool, lua_help_list_tool, reload_runtime_configs_tool, runtime_config_tool,
    skill_manager_tool,
};
use crate::host_core::lifecycle::{
    apply_runtime_entry_registry_delta, insert_skill_entry, luaskills_lifecycle_callback_lock,
};
use crate::host_core::skill_tools::select_skill_manager_user_root;
use crate::host_core::state::ServerInner;
use crate::luaskills_adapter::{build_luaskills_engine_options, install_luaskills_log_callback};
use crate::transport::mcp::protocol::*;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaVmPoolConfig, RuntimeEntryRegistryDelta,
    RuntimeSkillLifecycleCallback, RuntimeSkillLifecycleEvent, RuntimeSkillRoot, ToolCacheConfig,
    runtime_config_store::SkillConfigStore, set_entry_registry_callback,
    set_skill_lifecycle_callback,
};

// ============================================================
// Shared host runtime state
// 共享宿主运行时状态
// ============================================================

/// Host runtime core that owns tool registries, LuaSkills runtime state, and backend clients.
/// 拥有工具注册表、LuaSkills 运行时状态与后端客户端的宿主运行时核心。
#[derive(Clone)]
pub struct HostRuntime {
    pub(super) inner: Arc<Mutex<ServerInner>>,
    // gRPC clients stored outside the mutex — they are Clone and do not
    // require exclusive access, so extracting them for tool calls no longer
    // blocks on other concurrent operations (tools/list, initialize, etc).
    #[allow(dead_code)] // reserved for VMM forwarding mode
    pub(super) vmm: Option<VmmClient>,
    pub(super) lua_engine: Option<Arc<StdRwLock<LuaEngine>>>,
    pub(super) lua_engine_options: Option<LuaEngineOptions>,
    pub(super) lua_skill_roots: Option<Vec<RuntimeSkillRoot>>,
    pub(super) runtime_skill_config_file_path: Option<PathBuf>,
}

impl HostRuntime {
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
        let _lifecycle_callback_guard = luaskills_lifecycle_callback_lock()
            .lock()
            .expect("LuaSkills lifecycle callback lock should not be poisoned");
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
            reload_runtime_configs_tool(),
        );

        // --- skill-manager: host-owned LuaSkills install/update/uninstall/list management ---
        inner
            .host_tools
            .insert("skill-manager".to_string(), skill_manager_tool());
    }

    /// Register the host-owned luaskill-config tool after one effective unified config file path becomes available.
    /// 在生效的统一配置文件路径可用后注册宿主自有的 luaskill-config 工具。
    fn register_runtime_config_tool(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- luaskill-config: inspect or mutate the host-managed unified runtime skill config ---
        inner
            .host_tools
            .insert("luaskill-config".to_string(), runtime_config_tool());
    }

    /// Register the host-wrapped Lua help tools only after the Lua engine has been configured successfully.
    /// 仅在 Lua 引擎成功完成配置后再注册宿主包装的 Lua help 工具。
    fn register_lua_help_tools(&mut self) {
        let mut inner = self.inner.try_lock().unwrap();

        // --- vulcan-help-list: list strict LuaSkills help trees for host-side help wrappers ---
        inner
            .host_tools
            .insert("vulcan-help-list".to_string(), lua_help_list_tool());

        // --- vulcan-help-detail: read one strict LuaSkills help flow and render it for MCP ---
        inner
            .host_tools
            .insert("vulcan-help-detail".to_string(), lua_help_detail_tool());
    }

    /// Resolve the default Lua engine used by the MCP host product surface.
    /// 解析 MCP 宿主产品面固定使用的默认 Lua 引擎。
    pub(crate) fn resolve_lua_engine_for_environment(
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
    pub(crate) fn resolve_lua_runtime_target(
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
    pub(super) fn resolve_lua_runtime_user_target(
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
    pub(super) fn resolve_runtime_skill_config_store(
        &self,
    ) -> Result<SkillConfigStore, (i64, String)> {
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
}

#[cfg(test)]
mod tests;
