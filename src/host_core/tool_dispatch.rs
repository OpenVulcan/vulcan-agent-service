use serde_json::Value;

use crate::host_core::HostRuntime;
use crate::host_core::model::{RuntimeTextContent, RuntimeToolCallRequest, RuntimeToolCallResult};
use crate::host_core::projections::render_help_detail_markdown;
use crate::luaskills_adapter::{
    build_runtime_invocation_context, build_runtime_request_context,
    client_budget_snapshot_for_render,
};
use crate::support::RuntimeRequestContext;
use crate::support::temp_maintenance::ensure_runtime_temp_dir;
use crate::support::tool_result_format::{HostRenderOptions, render_tool_result_text};

impl HostRuntime {
    /// Invoke one runtime tool and return a transport-neutral tool-call result.
    /// 调用单个运行时工具并返回传输无关的工具调用结果。
    pub(crate) async fn call_runtime_tool(
        &self,
        req: RuntimeToolCallRequest,
        request_context: &RuntimeRequestContext,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        let tool = self.resolve_tool_descriptor(&req.name).await?;
        let args = req.arguments.unwrap_or_default();

        let result = match tool.name.as_str() {
            "vulcan-help-list" => RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(&self.list_luaskill_help()?)],
                is_error: None,
            },

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
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| (-32602, "Missing required parameter: flow".to_string()))?;
                let engine_clone = engine.clone();
                let runtime_request_context = build_runtime_request_context(request_context);
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
                    Ok(Some(detail)) => RuntimeToolCallResult {
                        content: vec![RuntimeTextContent::text(&render_help_detail_markdown(
                            &detail,
                        ))],
                        is_error: None,
                    },
                    Ok(None) => RuntimeToolCallResult {
                        content: vec![RuntimeTextContent::text("Skill help not found.")],
                        is_error: Some(true),
                    },
                    Err(error) => RuntimeToolCallResult {
                        content: vec![RuntimeTextContent::text(&error)],
                        is_error: Some(true),
                    },
                }
            }

            "reload_vulcan_mcp_configs" => RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(
                    &self.reload_luaskill_runtime_configs()?,
                )],
                is_error: None,
            },

            "luaskill-config" => RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(
                    &self.execute_luaskill_config_tool_text(&args)?,
                )],
                is_error: None,
            },

            "skill-manager" => self.execute_skill_manager_tool_args(&args).await?,

            _ => {
                self.call_dynamic_luaskill_tool_for_mcp(&tool.name, args, request_context)
                    .await?
            }
        };

        Ok(result)
    }

    /// Call one dynamic LuaSkill tool and render it into an MCP tool result.
    /// 调用单个动态 LuaSkill 工具，并将其渲染为 MCP 工具结果。
    async fn call_dynamic_luaskill_tool_for_mcp(
        &self,
        tool_name: &str,
        args: Value,
        request_context: &RuntimeRequestContext,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        if !self.has_lua_engine() {
            return Err((-32603, format!("Tool not implemented: {}", tool_name)));
        }

        let (target_engine, target_skill_roots) = self.resolve_lua_runtime_target()?;
        let is_skill = target_engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .is_skill(tool_name);
        if !is_skill {
            return Err((-32603, format!("Tool not implemented: {}", tool_name)));
        }

        let engine_clone = target_engine.clone();
        let tool_name = tool_name.to_string();
        let invocation_tool_name = tool_name.clone();
        let skill_name = target_engine
            .read()
            .map_err(|_| (-32603, "Lua engine lock poisoned.".to_string()))?
            .skill_name_for_tool(&tool_name);
        let args_clone = args.clone();
        let request_context = request_context.clone();
        let budget_request_context = request_context.clone();
        let invocation_context = build_runtime_invocation_context(
            Some(&request_context),
            Some(&invocation_tool_name),
            skill_name.as_deref(),
        );
        let result = tokio::task::spawn_blocking(move || {
            let engine = engine_clone
                .read()
                .map_err(|_| "Lua engine lock poisoned".to_string())?;
            engine.call_skill(
                &invocation_tool_name,
                &args_clone,
                Some(&invocation_context),
            )
        })
        .await
        .map_err(|error| (-32603, format!("Lua skill spawn error: {}", error)))?;

        match result {
            Ok(value) => {
                let client_budget = client_budget_snapshot_for_render(
                    Some(&budget_request_context),
                    Some(&tool_name),
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
                Ok(RuntimeToolCallResult {
                    content: vec![RuntimeTextContent::text(&render_tool_result_text(
                        &value,
                        skill_name.as_deref(),
                        Some(&client_budget),
                        &HostRenderOptions {
                            spill_root: Some(spill_root),
                            template_skill_roots: target_skill_roots
                                .iter()
                                .map(|root| root.skills_dir.clone())
                                .collect(),
                            template_resources_root: self.tool_result_template_resources_root(),
                        },
                    ))],
                    is_error: None,
                })
            }
            Err(error) => Ok(RuntimeToolCallResult {
                content: vec![RuntimeTextContent::text(&error)],
                is_error: Some(true),
            }),
        }
    }
}
