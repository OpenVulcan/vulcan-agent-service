use super::*;

#[tonic::async_trait]
impl LuaSkillsService for McpServiceImpl {
    async fn list_skills(
        &self,
        request: Request<LuaSkillListSkillsRequest>,
    ) -> Result<Response<LuaSkillListSkillsResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let skills = self
            .runtime
            .list_luaskill_packages()
            .await
            .map_err(mcp_error_to_status)?
            .iter()
            .map(lua_skill_package_to_pb)
            .collect();
        Ok(Response::new(LuaSkillListSkillsResponse { skills }))
    }

    async fn list_tools(
        &self,
        request: Request<LuaSkillListToolsRequest>,
    ) -> Result<Response<LuaSkillListToolsResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let projection = normalize_luaskill_projection(req.projection.as_ref());
        let tools = self
            .runtime
            .list_luaskill_tools()
            .await
            .map_err(mcp_error_to_status)?
            .iter()
            .map(|descriptor| {
                let projected_tool = project_runtime_tool_descriptor(
                    &descriptor.tool,
                    &LuaSkillToolProjectionOptions {
                        hide_managed_luaskill_sid: projection.supports_managed_luaskill_sid,
                    },
                );
                let mut projected = descriptor.clone();
                projected.tool = projected_tool;
                lua_skill_tool_to_pb(&projected)
            })
            .collect();
        Ok(Response::new(LuaSkillListToolsResponse { tools }))
    }

    async fn get_tool(
        &self,
        request: Request<LuaSkillGetToolRequest>,
    ) -> Result<Response<LuaSkillGetToolResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let projection = normalize_luaskill_projection(req.projection.as_ref());
        let mut tool = self
            .runtime
            .get_luaskill_tool(&req.tool_name)
            .await
            .map_err(mcp_error_to_status)?;
        tool.tool = project_runtime_tool_descriptor(
            &tool.tool,
            &LuaSkillToolProjectionOptions {
                hide_managed_luaskill_sid: projection.supports_managed_luaskill_sid,
            },
        );
        Ok(Response::new(LuaSkillGetToolResponse {
            tool: Some(lua_skill_tool_to_pb(&tool)),
        }))
    }

    async fn call_tool(
        &self,
        request: Request<LuaSkillCallToolRequest>,
    ) -> Result<Response<LuaSkillCallToolResponse>, Status> {
        let req = request.into_inner();
        let context = require_luaskill_context(req.context.as_ref())?;
        let projection = normalize_luaskill_projection(req.projection.as_ref());
        let arguments = parse_json_arguments(&req.arguments_json)?;
        let arguments = if projection.supports_managed_luaskill_sid {
            let tool = self
                .runtime
                .get_luaskill_tool(&req.tool_name)
                .await
                .map_err(mcp_error_to_status)?;
            inject_managed_luaskill_sid_argument(
                &tool.tool,
                arguments,
                &context.client_name,
                projection.session_id.as_deref(),
            )
            .map_err(mcp_error_to_status)?
        } else {
            arguments
        };
        let result = self
            .runtime
            .call_luaskill_tool(
                &req.tool_name,
                arguments,
                &context.client_name,
                optional_str(&context.client_version),
                optional_str(&context.request_id),
            )
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_call_response(&result)))
    }

    async fn list_help(
        &self,
        request: Request<LuaSkillListHelpRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .list_luaskill_help()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn get_help(
        &self,
        request: Request<LuaSkillGetHelpRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .get_luaskill_help(
                &req.skill_id,
                &req.flow,
                &context.client_name,
                optional_str(&context.client_version),
                optional_str(&context.request_id),
            )
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn list_skill_config(
        &self,
        request: Request<LuaSkillConfigListRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let skill_id = optional_string(req.skill_id);
        let text = self
            .runtime
            .list_luaskill_config(skill_id)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn get_skill_config(
        &self,
        request: Request<LuaSkillConfigGetRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .get_luaskill_config(req.skill_id, req.key)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn set_skill_config(
        &self,
        request: Request<LuaSkillConfigSetRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .set_luaskill_config(req.skill_id, req.key, req.value)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn delete_skill_config(
        &self,
        request: Request<LuaSkillConfigDeleteRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .delete_luaskill_config(req.skill_id, req.key)
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn list_installed_skills(
        &self,
        request: Request<LuaSkillListInstalledSkillsRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .list_installed_luaskills()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }

    async fn install_skill(
        &self,
        request: Request<LuaSkillInstallRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .install_luaskill(req.source, optional_string(req.source_type))
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn update_skill(
        &self,
        request: Request<LuaSkillUpdateRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .update_luaskill(req.skill_id)
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn uninstall_skill(
        &self,
        request: Request<LuaSkillUninstallRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let result = self
            .runtime
            .uninstall_luaskill(req.skill_id)
            .await
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(tool_call_result_to_text_response(&result)))
    }

    async fn reload_runtime_configs(
        &self,
        request: Request<LuaSkillReloadRuntimeConfigsRequest>,
    ) -> Result<Response<LuaSkillTextResponse>, Status> {
        let req = request.into_inner();
        let _context = require_luaskill_context(req.context.as_ref())?;
        let text = self
            .runtime
            .reload_luaskill_runtime_configs()
            .map_err(mcp_error_to_status)?;
        Ok(Response::new(text_response(text)))
    }
}
