use serde_json::Value;
use std::collections::BTreeMap;

use crate::config::reload_runtime_configs;
use crate::host_core::model::{RuntimeTextContent, RuntimeToolCallResult};
use crate::host_core::projections::{
    build_luaskill_tool_descriptor, render_help_detail_markdown, render_help_list_markdown,
    require_non_empty_grpc_field,
};
use crate::host_core::runtime::HostRuntime;
use crate::host_core::skill_tools::{
    infer_skill_install_source_type, parse_optional_skill_install_source_type,
    render_skill_url_install_not_implemented_result, require_skill_manager_skill_id,
    require_skill_manager_source,
};
use crate::host_core::state::{LuaSkillPackageDescriptor, LuaSkillToolDescriptor};
use crate::luaskills_adapter::{
    build_grpc_runtime_invocation_context, build_grpc_runtime_request_context,
    grpc_client_budget_snapshot_for_render,
};
use crate::model_provider::install_luaskills_model_callbacks;
use crate::support::temp_maintenance::ensure_runtime_temp_dir;
use crate::support::tool_result_format::{HostRenderOptions, render_tool_result_text};
use luaskills::{SkillInstallRequest, SkillInstallSourceType};

impl HostRuntime {
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
        request_id: Option<&str>,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
            request_id,
            Some(&tool_name_for_call),
            skill_name.as_deref(),
        )
        .map_err(|error| {
            (
                -32603,
                format!("build Lua invocation context failed: {error}"),
            )
        })?;
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
                )
                .map_err(|error| (-32603, format!("resolve client budget failed: {error}")))?;
                let spill_root = ensure_runtime_temp_dir()
                    .map_err(|error| {
                        (
                            -32603,
                            format!("resolve runtime spill dir failed: {}", error),
                        )
                    })?
                    .join("mcp")
                    .join("cache");
                let rendered = render_tool_result_text(
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
                )
                .map_err(|error| (-32603, format!("render Lua skill result failed: {error}")))?;
                Ok(RuntimeToolCallResult {
                    content: vec![RuntimeTextContent::text(&rendered)],
                    is_error: None,
                })
            }
            Err(error) => Ok(RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(&error)],
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
            .list_skill_help()
            .map_err(|error| (-32603, error))?;
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
        request_id: Option<&str>,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        let skill_id = require_non_empty_grpc_field(skill_id, "skill_id")?;
        let flow = require_non_empty_grpc_field(flow, "flow")?;
        let client_name = require_non_empty_grpc_field(client_name, "client_name")?;
        let engine = self.resolve_lua_engine_for_environment()?;
        let runtime_request_context =
            build_grpc_runtime_request_context(&client_name, client_version, request_id);
        let result = tokio::task::spawn_blocking(move || {
            let engine = engine
                .read()
                .map_err(|_| "Lua engine lock poisoned".to_string())?;
            engine.render_skill_help_detail(&skill_id, &flow, Some(&runtime_request_context))
        })
        .await
        .map_err(|error| (-32603, format!("vulcan-help-detail spawn error: {}", error)))?;

        match result {
            Ok(Some(detail)) => Ok(RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(&render_help_detail_markdown(
                    &detail,
                ))],
                is_error: None,
            }),
            Ok(None) => Ok(RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text("Skill help not found.")],
                is_error: Some(true),
            }),
            Err(error) => Ok(RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(&error)],
                is_error: Some(true),
            }),
        }
    }

    /// Dispatch one strict LuaSkills runtime-config JSON request on the blocking runtime pool.
    /// 在阻塞运行时线程池中分发一份严格的 LuaSkills runtime-config JSON 请求。
    /// Parameter `request_json` is the complete upstream request object encoded as JSON.
    /// 参数：`request_json` 是编码为 JSON 的完整上游请求对象。
    /// Returns the upstream stable JSON response envelope or a host execution error.
    /// 返回上游稳定 JSON 响应包络，或宿主执行错误。
    pub async fn dispatch_luaskill_runtime_config(
        &self,
        request_json: String,
    ) -> Result<String, (i64, String)> {
        let engine = self.resolve_lua_engine_for_environment()?;
        tokio::task::spawn_blocking(move || {
            let mut engine = engine
                .write()
                .map_err(|_| "Lua engine lock poisoned".to_string())?;
            Ok(engine.dispatch_runtime_config_tool_json(&request_json))
        })
        .await
        .map_err(|error| {
            (
                -32603,
                format!("runtime-config dispatcher spawn error: {}", error),
            )
        })?
        .map_err(|error| (-32603, error))
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
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
    pub async fn update_luaskill(
        &self,
        skill_id: String,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        let skill_id = require_skill_manager_skill_id(Some(skill_id.as_str()), "uninstall")?;
        self.execute_skill_uninstall(skill_id).await
    }

    /// Reload hot-reloadable runtime configs through a stable gRPC method.
    /// 通过稳定 gRPC 方法重载可热重载运行时配置。
    pub fn reload_luaskill_runtime_configs(&self) -> Result<String, (i64, String)> {
        // Stage and commit all hot-reloadable configs together so failures cannot expose mixed versions.
        // 将全部可热重载配置一起分阶段加载并提交，避免失败时暴露混合版本。
        let reports = reload_runtime_configs()
            .map_err(|error| (-32603, format!("reload runtime configs failed: {}", error)))?;
        install_luaskills_model_callbacks().map_err(|error| {
            (
                -32603,
                format!("reload model callbacks failed: {}", error.message),
            )
        })?;

        Ok(format!(
            "Runtime MCP configs reloaded successfully.\n- client_budgets: patterns={}, grpc_clients={}, source={}\n- tool_configs: tools={}, source={}\n- model_config: provider_enabled={}, embed={}, embed_api_key={}, embed_base_url={}, llm={}, llm_api_key={}, llm_base_url={}, source={}\n- config.yaml: not reloaded",
            reports.client_budget.client_count,
            reports.client_budget.grpc_client_count,
            reports
                .client_budget
                .source_path
                .as_deref()
                .unwrap_or("unavailable"),
            reports.tool_config.tool_count,
            reports
                .tool_config
                .source_path
                .as_deref()
                .unwrap_or("unavailable"),
            reports.model_config.provider_enabled,
            reports.model_config.embedding_enabled,
            reports.model_config.embedding_api_key_configured,
            reports.model_config.embedding_base_url_configured,
            reports.model_config.llm_enabled,
            reports.model_config.llm_api_key_configured,
            reports.model_config.llm_base_url_configured,
            reports
                .model_config
                .source_path
                .as_deref()
                .unwrap_or("unavailable")
        ))
    }
}
