use serde_json::Value;

use crate::host_core::HostRuntime;
use crate::host_core::projections::render_help_detail_markdown;
use crate::luaskills_adapter::{
    build_runtime_invocation_context, build_runtime_request_context,
    client_budget_snapshot_for_render,
};
use crate::support::temp_maintenance::ensure_runtime_temp_dir;
use crate::support::tool_result_format::{HostRenderOptions, render_tool_result_text};
use crate::transport::mcp::protocol::{
    RequestContext, TextContent, ToolCallRequest, ToolCallResult,
};

impl HostRuntime {
    /// Build the tools/call response value used by the MCP dispatcher.
    /// 构建 MCP dispatcher 使用的 tools/call 响应值。
    pub(crate) async fn call_mcp_tool_value(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let req: ToolCallRequest = serde_json::from_value(params.unwrap_or_default())
            .map_err(|error| (-32602, format!("Invalid tools/call params: {}", error)))?;
        let tool = self.resolve_mcp_tool(&req.name).await?;
        let args = req.arguments.unwrap_or_default();

        let result = match tool.name.as_str() {
            "vulcan-help-list" => ToolCallResult {
                content: vec![TextContent::text(&self.list_luaskill_help()?)],
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

            "reload_vulcan_mcp_configs" => ToolCallResult {
                content: vec![TextContent::text(&self.reload_luaskill_runtime_configs()?)],
                is_error: None,
            },

            "luaskill-config" => ToolCallResult {
                content: vec![TextContent::text(
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

        serde_json::to_value(result)
            .map_err(|error| (-32603, format!("Serialization error: {}", error)))
    }

    /// Call one dynamic LuaSkill tool and render it into an MCP tool result.
    /// 调用单个动态 LuaSkill 工具，并将其渲染为 MCP 工具结果。
    async fn call_dynamic_luaskill_tool_for_mcp(
        &self,
        tool_name: &str,
        args: Value,
        request_context: &RequestContext,
    ) -> Result<ToolCallResult, (i64, String)> {
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
                            template_resources_root: self.mcp_template_resources_root(),
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
}
