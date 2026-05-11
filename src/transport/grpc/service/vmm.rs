use super::*;

#[tonic::async_trait]
impl VmmService for McpServiceImpl {
    async fn healthz(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::HealthzResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_healthz().await?))
    }

    async fn list_projects(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::ListProjectsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_list_projects().await?))
    }

    async fn resolve_project(
        &self,
        request: Request<vmm_pb::ResolveProjectRequest>,
    ) -> Result<Response<vmm_pb::ResolveProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_resolve_project(request.into_inner()).await?,
        ))
    }

    async fn ensure_project(
        &self,
        request: Request<vmm_pb::EnsureProjectRequest>,
    ) -> Result<Response<vmm_pb::EnsureProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_ensure_project(request.into_inner()).await?,
        ))
    }

    async fn delete_project(
        &self,
        request: Request<vmm_pb::DeleteProjectRequest>,
    ) -> Result<Response<vmm_pb::DeleteProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_delete_project(request.into_inner()).await?,
        ))
    }

    async fn migrate_project(
        &self,
        request: Request<vmm_pb::MigrateProjectRequest>,
    ) -> Result<Response<vmm_pb::MigrateProjectResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_migrate_project(request.into_inner()).await?,
        ))
    }

    async fn resolve_user(
        &self,
        request: Request<vmm_pb::ResolveUserRequest>,
    ) -> Result<Response<vmm_pb::ResolveUserResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_resolve_user(request.into_inner()).await?,
        ))
    }

    async fn list_users(
        &self,
        _request: Request<()>,
    ) -> Result<Response<vmm_pb::ListUsersResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(client.forward_list_users().await?))
    }

    async fn delete_user(
        &self,
        request: Request<vmm_pb::DeleteUserRequest>,
    ) -> Result<Response<vmm_pb::DeleteUserResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_delete_user(request.into_inner()).await?,
        ))
    }

    async fn get_profile_nodes(
        &self,
        request: Request<vmm_pb::GetProfileNodesRequest>,
    ) -> Result<Response<vmm_pb::GetProfileNodesResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_profile_nodes(request.into_inner())
                .await?,
        ))
    }

    async fn get_profile_bundle(
        &self,
        request: Request<vmm_pb::GetProfileBundleRequest>,
    ) -> Result<Response<vmm_pb::GetProfileBundleResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_profile_bundle(request.into_inner())
                .await?,
        ))
    }

    async fn apply_profile_instruction(
        &self,
        request: Request<vmm_pb::ApplyProfileInstructionRequest>,
    ) -> Result<Response<vmm_pb::ApplyProfileInstructionResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_apply_profile_instruction(request.into_inner())
                .await?,
        ))
    }

    async fn search_memory_events(
        &self,
        request: Request<vmm_pb::SearchMemoryEventsRequest>,
    ) -> Result<Response<vmm_pb::SearchMemoryEventsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_search_memory_events(request.into_inner())
                .await?,
        ))
    }

    async fn get_turn_details(
        &self,
        request: Request<vmm_pb::GetTurnDetailsRequest>,
    ) -> Result<Response<vmm_pb::GetTurnDetailsResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client
                .forward_get_turn_details(request.into_inner())
                .await?,
        ))
    }

    async fn write_memories(
        &self,
        request: Request<vmm_pb::WriteMemoriesRequest>,
    ) -> Result<Response<vmm_pb::WriteMemoriesResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_write_memories(request.into_inner()).await?,
        ))
    }

    async fn chat_compact(
        &self,
        request: Request<vmm_pb::ChatCompactRequest>,
    ) -> Result<Response<vmm_pb::ChatCompactResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_chat_compact(request.into_inner()).await?,
        ))
    }

    async fn pre_check(
        &self,
        request: Request<vmm_pb::PreCheckRequest>,
    ) -> Result<Response<vmm_pb::PreCheckResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_pre_check(request.into_inner()).await?,
        ))
    }

    async fn post_action(
        &self,
        request: Request<vmm_pb::PostActionRequest>,
    ) -> Result<Response<vmm_pb::PostActionResponse>, Status> {
        let client = self.require_vmm_backend()?;
        Ok(Response::new(
            client.forward_post_action(request.into_inner()).await?,
        ))
    }
}
