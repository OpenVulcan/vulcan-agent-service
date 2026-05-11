use super::*;

#[tonic::async_trait]
impl HostAdapterService for McpServiceImpl {
    /// Return one normalized host adapter descriptor and capability profile.
    /// 返回一个归一化宿主适配器描述与能力画像。
    async fn get_host_adapter_profile(
        &self,
        request: Request<HostAdapterProfileRequest>,
    ) -> Result<Response<HostAdapterProfileResponse>, Status> {
        let req = request.into_inner();
        let adapter = self
            .runtime
            .describe_host_adapter(optional_str(&req.host_kind));
        let adapter_json = serialize_grpc_json(&adapter, "adapter_json")?;
        let profile_json = serialize_grpc_json(&adapter.profile, "profile_json")?;

        Ok(Response::new(HostAdapterProfileResponse {
            adapter_json,
            profile_json,
            host_kind: adapter.host_kind.as_str().to_string(),
            display_name: adapter.profile.display_name,
            refresh_mode: tool_refresh_mode_to_grpc(adapter.refresh_mode).to_string(),
            identity_mode: identity_mode_to_grpc(adapter.identity_mode).to_string(),
            is_error: false,
            message: String::new(),
            vmm_enabled: self.runtime.is_vmm_backend_enabled(),
            vmm_status: self.runtime.vmm_backend_status_message().to_string(),
        }))
    }

    /// Normalize one host adapter runtime context.
    /// 归一化一份宿主适配器运行时上下文。
    async fn build_host_adapter_runtime(
        &self,
        request: Request<HostAdapterRuntimeRequest>,
    ) -> Result<Response<HostAdapterRuntimeResponse>, Status> {
        let req = request.into_inner();
        let runtime = self
            .runtime
            .build_host_adapter_runtime(HostAdapterRuntimeInput {
                host_kind: optional_string(req.host_kind),
                adapter_host_kind: optional_string(req.adapter_host_kind),
                session_id: optional_string(req.session_id),
                workmem_id: optional_string(req.workmem_id),
                turn_id: optional_string(req.turn_id),
                workspace: optional_string(req.workspace),
                user_message: optional_string(req.user_message),
                conversation_id: optional_string(req.conversation_id),
                root_session_id: optional_string(req.root_session_id),
            });
        let runtime_json = serialize_grpc_json(&runtime, "runtime_json")?;

        Ok(Response::new(HostAdapterRuntimeResponse {
            runtime_json,
            host_kind: runtime.context.host_kind.as_str().to_string(),
            session_id: runtime.context.session_id.unwrap_or_default(),
            workmem_id: runtime.context.workmem_id.unwrap_or_default(),
            workmem_source: workmem_source_to_grpc(runtime.context.workmem_source).to_string(),
            identity_ready: runtime.identity_ready,
            degraded_reasons: runtime.degraded_reasons,
            is_error: false,
            message: String::new(),
            vmm_enabled: self.runtime.is_vmm_backend_enabled(),
            vmm_status: self.runtime.vmm_backend_status_message().to_string(),
        }))
    }

    /// Compare two tool registry snapshots and return restart guidance.
    /// 对比两份 tool 注册表快照并返回重启提示。
    async fn diff_tool_registry(
        &self,
        request: Request<HostAdapterDiffToolRegistryRequest>,
    ) -> Result<Response<HostAdapterDiffToolRegistryResponse>, Status> {
        let req = request.into_inner();
        let previous =
            parse_tool_registry_snapshot(&req.previous_snapshot_json, "previous_snapshot_json")?;
        let next = parse_tool_registry_snapshot(&req.next_snapshot_json, "next_snapshot_json")?;
        let refresh_mode = parse_optional_refresh_mode(&req.refresh_mode)?;
        let diff = diff_tool_registry_snapshots(
            &previous,
            &next,
            &ToolRegistryDiffOptions {
                refresh_mode,
                dynamic_tool_refresh_supported: req
                    .has_dynamic_tool_refresh_supported
                    .then_some(req.dynamic_tool_refresh_supported),
                host_restart_required: req.host_restart_required,
            },
        )
        .map_err(Status::invalid_argument)?;
        let diff_json = serialize_grpc_json(&diff, "diff_json")?;

        Ok(Response::new(HostAdapterDiffToolRegistryResponse {
            diff_json,
            changed_tool_ids: diff.changed_tool_ids,
            added_tool_ids: grpc_tool_ids(&diff.added),
            removed_tool_ids: grpc_tool_ids(&diff.removed),
            updated_tool_ids: grpc_tool_ids(&diff.updated),
            restart_required: diff.restart_required,
            summary: diff.summary,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Build model-facing and user-facing refresh guidance from tool snapshots.
    /// 根据 tool 快照构建面向模型与用户的刷新提示。
    async fn build_tool_refresh_notice(
        &self,
        request: Request<HostAdapterToolRefreshNoticeRequest>,
    ) -> Result<Response<HostAdapterToolRefreshNoticeResponse>, Status> {
        let req = request.into_inner();
        let previous =
            parse_tool_registry_snapshot(&req.previous_snapshot_json, "previous_snapshot_json")?;
        let next = parse_tool_registry_snapshot(&req.next_snapshot_json, "next_snapshot_json")?;
        let adapter = self
            .runtime
            .describe_host_adapter(optional_str(&req.host_kind));
        let notice = self
            .runtime
            .build_tool_refresh_notice_for_adapter(previous, next, Some(adapter.host_kind.as_str()))
            .map_err(Status::invalid_argument)?;
        let notice_json = serialize_grpc_json(&notice, "notice_json")?;

        Ok(Response::new(HostAdapterToolRefreshNoticeResponse {
            notice_json,
            changed: notice.changed,
            refresh_mode: tool_refresh_mode_to_grpc(notice.refresh_mode).to_string(),
            severity: notice_severity_to_grpc(notice.severity).to_string(),
            restart_required: notice.restart_required,
            changed_tool_ids: notice.changed_tool_ids,
            model_message: notice.model_message,
            user_message: notice.user_message,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Return whether the host runtime has an enabled VMM backend.
    /// 返回当前宿主运行时是否启用了 VMM 后端。
    async fn get_vmm_status(
        &self,
        request: Request<HostAdapterVmmStatusRequest>,
    ) -> Result<Response<HostAdapterVmmStatusResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        Ok(Response::new(HostAdapterVmmStatusResponse {
            vmm_enabled,
            vmm_status,
            is_error: false,
            message: String::new(),
        }))
    }

    /// Return stable VMM memory tool metadata for host plugin registration.
    /// 返回宿主插件注册工具时使用的稳定 VMM 记忆工具元信息。
    async fn list_vmm_memory_tools(
        &self,
        request: Request<HostAdapterListVmmMemoryToolsRequest>,
    ) -> Result<Response<HostAdapterListVmmMemoryToolsResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        // Do not expose VMM tools when the VMM backend is not enabled.
        // 当 VMM 后端未启用时，不向宿主暴露任何 VMM 工具。
        if !vmm_enabled {
            return Ok(Response::new(HostAdapterListVmmMemoryToolsResponse {
                tools: Vec::new(),
                is_error: false,
                message: vmm_status.clone(),
                vmm_enabled,
                vmm_status,
            }));
        }

        // Tool descriptors are available once the VMM backend is enabled; actual
        // health errors are still surfaced by each VMM relay call.
        // VMM 后端启用后即可暴露工具描述；
        // 具体健康错误仍由每次 VMM 中转调用自行返回。
        let tools = vmm_memory_tool_descriptors()
            .into_iter()
            .map(|descriptor| HostAdapterToolDescriptor {
                name: descriptor.name,
                description: descriptor.description,
                input_schema_json: descriptor.input_schema_json,
                annotations_json: descriptor.annotations_json,
                source: descriptor.source,
            })
            .collect();

        Ok(Response::new(HostAdapterListVmmMemoryToolsResponse {
            tools,
            is_error: false,
            message: String::new(),
            vmm_enabled,
            vmm_status,
        }))
    }

    /// Return stable VMM binding/admin tool metadata for host plugin registration.
    /// 返回宿主插件注册绑定与管理工具时使用的稳定 VMM 元信息。
    async fn list_vmm_binding_tools(
        &self,
        request: Request<HostAdapterListVmmBindingToolsRequest>,
    ) -> Result<Response<HostAdapterListVmmBindingToolsResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        // Binding/admin descriptors stay visible even before VMM is healthy so hosts can
        // keep one stable manifest and inspect setup guidance through the same tool ids.
        // 绑定与管理工具描述即使在 VMM 尚未健康时也保持可见，
        // 这样宿主仍能维持稳定 manifest，并通过同一组工具标识查看初始化指导。
        let tools = vmm_binding_tool_descriptors()
            .into_iter()
            .map(|descriptor| HostAdapterToolDescriptor {
                name: descriptor.name,
                description: descriptor.description,
                input_schema_json: descriptor.input_schema_json,
                annotations_json: descriptor.annotations_json,
                source: descriptor.source,
            })
            .collect();

        Ok(Response::new(HostAdapterListVmmBindingToolsResponse {
            tools,
            is_error: false,
            message: String::new(),
            vmm_enabled,
            vmm_status,
        }))
    }

    /// Return stable VMM profile-adjust tool metadata for host plugin registration.
    /// 返回宿主插件注册画像调整工具时使用的稳定 VMM 元信息。
    async fn list_vmm_profile_tools(
        &self,
        request: Request<HostAdapterListVmmProfileToolsRequest>,
    ) -> Result<Response<HostAdapterListVmmProfileToolsResponse>, Status> {
        let _req = request.into_inner();
        let vmm_enabled = self.runtime.is_vmm_backend_enabled();
        let vmm_status = self.runtime.vmm_backend_status_message().to_string();

        // Profile-adjust descriptors depend on the VMM backend because they update reviewed
        // durable profile state rather than merely helping hosts inspect local bindings.
        // 画像调整描述依赖 VMM 后端，因为它们会更新经评审的长期画像状态，
        // 而不是像本地绑定查看那样只帮助宿主检查自身配置。
        if !vmm_enabled {
            return Ok(Response::new(HostAdapterListVmmProfileToolsResponse {
                tools: Vec::new(),
                is_error: false,
                message: vmm_status.clone(),
                vmm_enabled,
                vmm_status,
            }));
        }

        let tools = vmm_profile_tool_descriptors()
            .into_iter()
            .map(|descriptor| HostAdapterToolDescriptor {
                name: descriptor.name,
                description: descriptor.description,
                input_schema_json: descriptor.input_schema_json,
                annotations_json: descriptor.annotations_json,
                source: descriptor.source,
            })
            .collect();

        Ok(Response::new(HostAdapterListVmmProfileToolsResponse {
            tools,
            is_error: false,
            message: String::new(),
            vmm_enabled,
            vmm_status,
        }))
    }
}
