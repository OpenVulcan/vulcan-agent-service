use std::fmt::Write as _;

use crate::host_core::model::{RuntimeTextContent, RuntimeToolCallResult};
use crate::host_core::runtime::HostRuntime;
use crate::host_core::skill_tools::{
    SkillManagerAction, SkillManagerToolArguments, build_skill_manager_for_root,
    infer_skill_install_source_type, parse_skill_manager_tool_arguments,
    render_skill_apply_tool_result, render_skill_install_record,
    render_skill_uninstall_tool_result, render_skill_url_install_not_implemented_result,
    require_skill_manager_skill_id, require_skill_manager_source, select_skill_manager_user_root,
};
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    RuntimeSkillRoot, SkillInstallRequest, SkillInstallSourceType, SkillUninstallOptions,
};
use serde_json::Value;

impl HostRuntime {
    /// Execute the host-owned skill-manager tool from raw JSON arguments.
    /// 使用原始 JSON 参数执行宿主自有 skill-manager 工具。
    pub(crate) async fn execute_skill_manager_tool_args(
        &self,
        args: &Value,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        let request = parse_skill_manager_tool_arguments(args)?;
        self.execute_skill_manager_tool(request).await
    }

    /// Execute one parsed host-owned skill-manager request against the configured LuaSkills runtime.
    /// 针对已配置的 LuaSkills 运行时执行一次解析后的宿主自有 skill-manager 请求。
    async fn execute_skill_manager_tool(
        &self,
        request: SkillManagerToolArguments,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        match request.action {
            SkillManagerAction::List => {
                let rendered = self.render_skill_manager_list()?;
                Ok(RuntimeToolCallResult {
                    content: vec![RuntimeTextContent::text(&rendered)],
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
    pub(super) fn render_skill_manager_list(&self) -> Result<String, (i64, String)> {
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
    pub(super) async fn execute_skill_install(
        &self,
        request: SkillInstallRequest,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
    pub(super) async fn execute_skill_update(
        &self,
        request: SkillInstallRequest,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
    pub(super) async fn execute_skill_uninstall(
        &self,
        skill_id: String,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
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
}
