use serde_json::{Value, json};

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

/// Convert an optional protocol arguments field into the runtime JSON object contract.
/// 将协议层可选 arguments 字段转换为运行时 JSON 对象契约。
///
/// Parameters: `arguments` is the optional tool arguments payload parsed from the transport request.
/// 参数：`arguments` 是从传输层请求解析出的可选工具参数载荷。
///
/// Returns: the caller-supplied JSON value, or an empty object when the field was omitted.
/// 返回：调用方提供的 JSON 值；当字段被省略时返回空对象。
fn runtime_tool_arguments_or_empty_object(arguments: Option<Value>) -> Value {
    arguments.unwrap_or_else(|| json!({}))
}

impl HostRuntime {
    /// Invoke one runtime tool and return a transport-neutral tool-call result.
    /// 调用单个运行时工具并返回传输无关的工具调用结果。
    pub(crate) async fn call_runtime_tool(
        &self,
        req: RuntimeToolCallRequest,
        request_context: &RuntimeRequestContext,
    ) -> Result<RuntimeToolCallResult, (i64, String)> {
        let tool = self.resolve_tool_descriptor(&req.name).await?;
        let args = runtime_tool_arguments_or_empty_object(req.arguments);

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

            "runtime-config" => {
                // Serialize the already-decoded MCP argument object back to the canonical upstream JSON boundary.
                // 把 MCP 已解码参数对象重新序列化到上游标准 JSON 边界。
                let request_json = serde_json::to_string(&args).map_err(|error| {
                    (
                        -32603,
                        format!("failed to serialize runtime-config request: {}", error),
                    )
                })?;
                let response_json = self.dispatch_luaskill_runtime_config(request_json).await?;
                let response: luaskills::RuntimeSkillConfigToolResponse =
                    serde_json::from_str(&response_json).map_err(|error| {
                        (
                            -32603,
                            format!(
                                "failed to decode runtime-config dispatcher response: {}",
                                error
                            ),
                        )
                    })?;
                RuntimeToolCallResult {
                    content: vec![RuntimeTextContent::text(&response_json)],
                    is_error: (!response.ok).then_some(true),
                }
            }

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
                        template_resources_root: self.tool_result_template_resources_root(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Missing transport arguments should become the empty object expected by runtime tools.
    /// 缺省的传输层 arguments 应转换为运行时工具期望的空对象。
    #[test]
    fn runtime_tool_arguments_or_empty_object_defaults_missing_arguments_to_object() {
        let arguments = runtime_tool_arguments_or_empty_object(None);

        assert_eq!(arguments, json!({}));
    }

    /// Explicit caller JSON should pass through unchanged so invalid payloads remain visible to tool parsers.
    /// 显式调用方 JSON 应原样透传，确保无效载荷仍能被工具解析器看见。
    #[test]
    fn runtime_tool_arguments_or_empty_object_preserves_explicit_payloads() {
        let explicit_null = runtime_tool_arguments_or_empty_object(Some(Value::Null));
        let explicit_object =
            runtime_tool_arguments_or_empty_object(Some(json!({ "action": "list" })));

        assert_eq!(explicit_null, Value::Null);
        assert_eq!(explicit_object, json!({ "action": "list" }));
    }
}
