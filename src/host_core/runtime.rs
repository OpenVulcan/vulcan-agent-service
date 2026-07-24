use std::collections::HashMap;
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
    apply_runtime_entry_registry_delta, insert_skill_entry, lock_luaskills_lifecycle_callback,
};
use crate::host_core::skill_tools::select_skill_manager_user_root;
use crate::host_core::state::ServerInner;
use crate::luaskills_adapter::{build_luaskills_engine_options, install_luaskills_log_callback};
use crate::support::RuntimeRequestContext;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaVmPoolConfig, RuntimeEntryRegistryDelta,
    RuntimeSkillLifecycleCallback, RuntimeSkillLifecycleEvent, RuntimeSkillRoot, ToolCacheConfig,
    set_entry_registry_callback, set_skill_lifecycle_callback,
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
}

/// Lua runtime engine and effective skill-root chain used for dynamic tool execution.
/// 用于动态工具执行的 Lua 运行时引擎与生效技能根链。
type LuaRuntimeTarget = (Arc<StdRwLock<LuaEngine>>, Vec<RuntimeSkillRoot>);

/// Lua runtime target plus the host-forced USER root used by skill-manager mutations.
/// Lua 运行时目标以及 skill-manager 变更操作使用的宿主强制 USER 根。
type LuaRuntimeUserTarget = (
    Arc<StdRwLock<LuaEngine>>,
    Vec<RuntimeSkillRoot>,
    RuntimeSkillRoot,
);

impl HostRuntime {
    /// Build a host runtime with the default host-owned tools registered before shared runtime state is published.
    /// 构建一份宿主运行时，并在共享运行时状态发布前完成默认宿主工具注册。
    pub fn new() -> Self {
        // Assemble the registry while it is still uniquely owned, so construction never depends on an async mutex lock.
        // 在注册表仍被唯一拥有时完成组装，使构建过程不依赖异步互斥锁。
        let mut inner = ServerInner {
            host_tools: HashMap::new(),
            skill_tools: HashMap::new(),
            skill_entries: HashMap::new(),
            version: None,
            initialized: false,
            client_capabilities: RuntimeRequestContext::default().client_capabilities,
        };
        Self::insert_default_host_tools(&mut inner);
        Self {
            inner: Arc::new(Mutex::new(inner)),
            vmm: None,
            lua_engine: None,
            lua_engine_options: None,
            lua_skill_roots: None,
        }
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

    /// Configure Lua skills from the resolved runtime roots and register their host-facing tool surface.
    /// 从已解析的运行时根配置 Lua skills，并注册其面向宿主的工具面。
    /// Parameters `config`, `skill_roots`, `pool_config`, and `cache_config` define the effective LuaSkills engine, root chain, pool, and cache.
    /// 参数 `config`、`skill_roots`、`pool_config` 和 `cache_config` 定义生效 LuaSkills 引擎、根链、池与缓存。
    /// Returns the configured runtime after engine initialization, host help registration, dynamic entry registration, and callbacks setup.
    /// 在引擎初始化、宿主帮助工具注册、动态入口注册与回调设置完成后返回配置后的运行时。
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
        let entries = engine.list_entries()?;
        eprintln!("[MCP] {} Lua skills loaded", entries.len());
        let engine = Arc::new(StdRwLock::new(engine));
        self.lua_engine = Some(engine.clone());
        self.lua_engine_options = Some(engine_options);
        self.lua_skill_roots = Some(skill_roots.to_vec());

        {
            let inner = self.builder_inner_mut()?;
            Self::insert_runtime_config_tool(inner);
            Self::insert_lua_help_tools(inner);
            for entry in entries {
                insert_skill_entry(inner, entry);
            }
        }

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
        let _lifecycle_callback_guard = lock_luaskills_lifecycle_callback();
        set_skill_lifecycle_callback(Some(lifecycle_callback));

        Ok(self)
    }

    /// Resolve mutable runtime registry state during builder-only configuration.
    /// 在仅限构建期的配置过程中解析可变运行时注册表状态。
    /// Returns mutable registry state while `HostRuntime` owns the registry `Arc` uniquely, otherwise returns an error.
    /// 当 `HostRuntime` 唯一拥有注册表 `Arc` 时返回可变注册表状态，否则返回错误。
    fn builder_inner_mut(&mut self) -> Result<&mut ServerInner, Box<dyn std::error::Error>> {
        // Builder methods must mutate unpublished state directly; after cloning, callers must build a fresh runtime instead.
        // 构建方法必须直接修改尚未发布的状态；克隆后调用方必须重新构建运行时。
        let inner = Arc::get_mut(&mut self.inner).ok_or(
            "HostRuntime builder mutation requires a uniquely owned runtime; clone the runtime only after all builder configuration has completed.",
        )?;
        Ok(inner.get_mut())
    }

    /// Insert host-owned tools that are always available on every HostRuntime.
    /// 插入每个 HostRuntime 始终可用的宿主自有工具。
    /// Parameter `inner` is the mutable runtime registry assembled during builder-only configuration.
    /// 参数 `inner` 是仅限构建期配置阶段组装的可变运行时注册表。
    fn insert_default_host_tools(inner: &mut ServerInner) {
        // --- reload_vulcan_mcp_configs: hot reload runtime client budget / tool config files ---
        // --- reload_vulcan_mcp_configs：热重载运行时客户端预算与工具配置文件 ---
        inner.host_tools.insert(
            "reload_vulcan_mcp_configs".to_string(),
            reload_runtime_configs_tool(),
        );

        // --- skill-manager: host-owned LuaSkills install/update/uninstall/list management ---
        // --- skill-manager：宿主自有的 LuaSkills 安装、更新、卸载与列表管理 ---
        inner
            .host_tools
            .insert("skill-manager".to_string(), skill_manager_tool());
    }

    /// Insert the host-owned runtime-config tool after the LuaSkills engine is configured.
    /// 在 LuaSkills 引擎完成配置后插入宿主自有的 runtime-config 工具。
    /// Parameter `inner` is the mutable runtime registry assembled during builder-only configuration.
    /// 参数 `inner` 是仅限构建期配置阶段组装的可变运行时注册表。
    fn insert_runtime_config_tool(inner: &mut ServerInner) {
        // The canonical name matches LuaSkills so every transport exposes one contract and one authorization boundary.
        // 标准名称与 LuaSkills 保持一致，使所有传输层只暴露一份契约和一个授权边界。
        inner
            .host_tools
            .insert("runtime-config".to_string(), runtime_config_tool());
    }

    /// Register host-wrapped Lua help tools on a uniquely owned builder runtime.
    /// 在唯一拥有的构建期运行时上注册宿主包装的 Lua help 工具。
    /// Returns success while the runtime is still uniquely owned, or an error after it has been cloned or shared.
    /// 当运行时仍被唯一拥有时返回成功；运行时已被克隆或共享后返回错误。
    #[cfg(test)]
    fn register_lua_help_tools(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let inner = self.builder_inner_mut()?;
        Self::insert_lua_help_tools(inner);
        Ok(())
    }

    /// Insert host-wrapped Lua help tools after the Lua engine has been configured successfully.
    /// 在 Lua 引擎成功完成配置后插入宿主包装的 Lua help 工具。
    /// Parameter `inner` is the mutable runtime registry assembled during builder-only configuration.
    /// 参数 `inner` 是仅限构建期配置阶段组装的可变运行时注册表。
    fn insert_lua_help_tools(inner: &mut ServerInner) {
        // --- vulcan-help-list: list strict LuaSkills help trees for host-side help wrappers ---
        // --- vulcan-help-list：为宿主侧 help 包装器列出严格 LuaSkills help 树 ---
        inner
            .host_tools
            .insert("vulcan-help-list".to_string(), lua_help_list_tool());

        // --- vulcan-help-detail: read one strict LuaSkills help flow and render it for MCP ---
        // --- vulcan-help-detail：读取单个严格 LuaSkills help 流并渲染给 MCP ---
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

    /// Resolve the configured VMM backend client for gRPC relay calls.
    /// 为 gRPC 中转调用解析已配置的 VMM 后端客户端。
    pub(crate) fn resolve_vmm_backend(&self) -> Result<VmmClient, (i64, String)> {
        self.vmm.clone().ok_or_else(|| {
            (
                -32603,
                "VMM backend is not configured. Configure vulcan-agent-service with vmm_enable=true and a VMM endpoint.".to_string(),
            )
        })
    }

    /// Return whether VMM-dependent host features may run.
    /// 返回依赖 VMM 的宿主功能是否可以运行。
    pub(crate) fn is_vmm_backend_enabled(&self) -> bool {
        self.vmm.is_some()
    }

    /// Return a stable human-readable VMM backend status string.
    /// 返回稳定的人类可读 VMM 后端状态文本。
    pub(crate) fn vmm_backend_status_message(&self) -> &'static str {
        if self.is_vmm_backend_enabled() {
            "VMM backend is enabled."
        } else {
            "VMM backend is not configured. Configure vulcan-agent-service with vmm_enable=true and a VMM endpoint."
        }
    }

    /// Resolve the default Lua engine together with the effective default skill-root chain.
    /// 解析默认 Lua 引擎及其对应的默认技能根目录链。
    pub(crate) fn resolve_lua_runtime_target(&self) -> Result<LuaRuntimeTarget, (i64, String)> {
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
    ) -> Result<LuaRuntimeUserTarget, (i64, String)> {
        let (engine, roots) = self.resolve_lua_runtime_target()?;
        let target_root = select_skill_manager_user_root(&roots)?;
        Ok((engine, roots, target_root))
    }
}

#[cfg(test)]
mod tests;
