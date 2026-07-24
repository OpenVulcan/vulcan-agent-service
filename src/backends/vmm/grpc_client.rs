use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::pb_vmm::{
    ApplyProfileInstructionRequest, ChatCompactRequest, DeleteMemoriesRequest,
    DeleteProjectRequest, DeleteUserRequest, EnsureProjectRequest, EnsureProjectResponse,
    GetProfileBundleRequest, GetProfileBundleResponse, GetProfileNodesRequest,
    GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse, HealthzResponse,
    ListProjectsResponse, ListUsersResponse, MigrateProjectRequest, MigrateProjectResponse,
    PostActionRequest, PostActionResponse, PreCheckRequest, PreCheckResponse,
    ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest, ResolveUserResponse,
    SearchMemoryEventsRequest, SearchMemoryEventsResponse, WriteMemoriesRequest,
    WriteMemoriesResponse, vmm_service_client::VmmServiceClient,
};

/// VulcanMemoryMesh gRPC client wrapper that serializes access to the underlying tonic client.
/// VulcanMemoryMesh gRPC 客户端包装器，负责串行化底层 tonic 客户端访问。
#[derive(Clone)]
pub struct VmmClient {
    /// Shared tonic VMM service client guarded by a mutex because tonic clients require mutable access for calls.
    /// 由互斥锁保护的共享 tonic VMM 服务客户端，因为 tonic 客户端发起调用时需要可变访问。
    client: Arc<Mutex<VmmServiceClient<Channel>>>,
}

impl VmmClient {
    /// Connect to the specified VMM gRPC service endpoint.
    /// 连接指定的 VMM gRPC 服务端点。
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = VmmServiceClient::connect(endpoint.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    /// Forward one VMM Healthz request and return the original protobuf response.
    /// 转发一条 VMM Healthz 请求，并返回原始 protobuf 响应。
    pub async fn forward_healthz(&self) -> Result<HealthzResponse, tonic::Status> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.healthz(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ListProjects request and return the original protobuf response.
    /// 转发一条 VMM ListProjects 请求，并返回原始 protobuf 响应。
    pub async fn forward_list_projects(&self) -> Result<ListProjectsResponse, tonic::Status> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_projects(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ResolveProject request and return the original protobuf response.
    /// 转发一条 VMM ResolveProject 请求，并返回原始 protobuf 响应。
    pub async fn forward_resolve_project(
        &self,
        request: ResolveProjectRequest,
    ) -> Result<ResolveProjectResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.resolve_project(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM EnsureProject request and return the original protobuf response.
    /// 转发一条 VMM EnsureProject 请求，并返回原始 protobuf 响应。
    pub async fn forward_ensure_project(
        &self,
        request: EnsureProjectRequest,
    ) -> Result<EnsureProjectResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.ensure_project(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM DeleteProject request and return the original protobuf response.
    /// 转发一条 VMM DeleteProject 请求，并返回原始 protobuf 响应。
    pub async fn forward_delete_project(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<crate::pb_vmm::DeleteProjectResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.delete_project(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM MigrateProject request and return the original protobuf response.
    /// 转发一条 VMM MigrateProject 请求，并返回原始 protobuf 响应。
    pub async fn forward_migrate_project(
        &self,
        request: MigrateProjectRequest,
    ) -> Result<MigrateProjectResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.migrate_project(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ResolveUser request and return the original protobuf response.
    /// 转发一条 VMM ResolveUser 请求，并返回原始 protobuf 响应。
    pub async fn forward_resolve_user(
        &self,
        request: ResolveUserRequest,
    ) -> Result<ResolveUserResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.resolve_user(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ListUsers request and return the original protobuf response.
    /// 转发一条 VMM ListUsers 请求，并返回原始 protobuf 响应。
    pub async fn forward_list_users(&self) -> Result<ListUsersResponse, tonic::Status> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_users(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM DeleteUser request and return the original protobuf response.
    /// 转发一条 VMM DeleteUser 请求，并返回原始 protobuf 响应。
    pub async fn forward_delete_user(
        &self,
        request: DeleteUserRequest,
    ) -> Result<crate::pb_vmm::DeleteUserResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.delete_user(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM GetProfileNodes request and return the original protobuf response.
    /// 转发一条 VMM GetProfileNodes 请求，并返回原始 protobuf 响应。
    pub async fn forward_get_profile_nodes(
        &self,
        request: GetProfileNodesRequest,
    ) -> Result<GetProfileNodesResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.get_profile_nodes(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM GetProfileBundle request and return the original protobuf response.
    /// 转发一条 VMM GetProfileBundle 请求，并返回原始 protobuf 响应。
    pub async fn forward_get_profile_bundle(
        &self,
        request: GetProfileBundleRequest,
    ) -> Result<GetProfileBundleResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.get_profile_bundle(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ApplyProfileInstruction request and return the original protobuf response.
    /// 转发一条 VMM ApplyProfileInstruction 请求，并返回原始 protobuf 响应。
    pub async fn forward_apply_profile_instruction(
        &self,
        request: ApplyProfileInstructionRequest,
    ) -> Result<crate::pb_vmm::ApplyProfileInstructionResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.apply_profile_instruction(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM SearchMemoryEvents request and return the original protobuf response.
    /// 转发一条 VMM SearchMemoryEvents 请求，并返回原始 protobuf 响应。
    pub async fn forward_search_memory_events(
        &self,
        request: SearchMemoryEventsRequest,
    ) -> Result<SearchMemoryEventsResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.search_memory_events(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM GetTurnDetails request and return the original protobuf response.
    /// 转发一条 VMM GetTurnDetails 请求，并返回原始 protobuf 响应。
    pub async fn forward_get_turn_details(
        &self,
        request: GetTurnDetailsRequest,
    ) -> Result<GetTurnDetailsResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.get_turn_details(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM WriteMemories request and return the original protobuf response.
    /// 转发一条 VMM WriteMemories 请求，并返回原始 protobuf 响应。
    pub async fn forward_write_memories(
        &self,
        request: WriteMemoriesRequest,
    ) -> Result<WriteMemoriesResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.write_memories(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM DeleteMemories request and return the original protobuf response.
    /// 转发一条 VMM DeleteMemories 请求，并返回原始 protobuf 响应。
    pub async fn forward_delete_memories(
        &self,
        request: DeleteMemoriesRequest,
    ) -> Result<crate::pb_vmm::DeleteMemoriesResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.delete_memories(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM ChatCompact request and return the original protobuf response.
    /// 转发一条 VMM ChatCompact 请求，并返回原始 protobuf 响应。
    pub async fn forward_chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<crate::pb_vmm::ChatCompactResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.chat_compact(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM PreCheck request and return the original protobuf response.
    /// 转发一条 VMM PreCheck 请求，并返回原始 protobuf 响应。
    pub async fn forward_pre_check(
        &self,
        request: PreCheckRequest,
    ) -> Result<PreCheckResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.pre_check(req).await?;
        Ok(resp.into_inner())
    }

    /// Forward one VMM PostAction request and return the original protobuf response.
    /// 转发一条 VMM PostAction 请求，并返回原始 protobuf 响应。
    pub async fn forward_post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, tonic::Status> {
        let req = tonic::Request::new(request);
        let mut client = self.client.lock().await;
        let resp = client.post_action(req).await?;
        Ok(resp.into_inner())
    }
}
