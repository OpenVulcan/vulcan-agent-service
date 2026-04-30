use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::pb_vmm::{
    ApplyProfileInstructionRequest, ChatCompactRequest, DeleteProjectRequest, DeleteUserRequest,
    EnsureProjectRequest, GetProfileBundleRequest, GetProfileNodesRequest, GetTurnDetailsRequest,
    MigrateProjectRequest, PostActionRequest, PostActionTimelineItem, PreCheckRequest,
    ResolveProjectRequest, ResolveUserRequest, SearchMemoryEventsRequest, WriteMemoriesRequest,
    WriteMemoryItem, vmm_service_client::VmmServiceClient,
};

/// VulcanMemoryMesh gRPC client wrapper that serializes access to the underlying tonic client.
/// VulcanMemoryMesh gRPC 客户端包装器，负责串行化底层 tonic 客户端访问。
#[derive(Clone)]
pub struct VmmClient {
    client: Arc<Mutex<VmmServiceClient<Channel>>>,
    /// Current VMM service endpoint, used for logging and diagnostics only.
    /// 当前 VMM 服务端点，仅用于日志与诊断展示。
    pub endpoint: String,
}

impl VmmClient {
    /// Connect to the specified VMM gRPC service endpoint.
    /// 连接指定的 VMM gRPC 服务端点。
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = VmmServiceClient::connect(endpoint.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            endpoint: endpoint.to_string(),
        })
    }

    /// Execute the VMM health check and return a brief status string.
    /// 执行 VMM 健康检查并返回简要状态字符串。
    pub async fn healthz(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.healthz(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "status={}, trace_id={}",
            inner.status, inner.trace_id
        ))
    }

    /// List project paths registered in VMM.
    /// 列出 VMM 中已登记的项目路径。
    pub async fn list_projects(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_projects(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let projects: Vec<String> = inner
            .projects
            .iter()
            .map(|p| p.display_path.clone())
            .collect();
        Ok(format!(
            "projects={}, trace_id={}",
            projects.join(", "),
            inner.trace_id
        ))
    }

    /// Resolve a project reference and return a summarized project description.
    /// 解析项目引用并返回项目信息摘要。
    pub async fn resolve_project(&self, project_ref: &str) -> Result<String, String> {
        let req = tonic::Request::new(ResolveProjectRequest {
            project_ref: project_ref.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client
            .resolve_project(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let path = inner
            .project
            .as_ref()
            .map(|p| p.display_path.clone())
            .unwrap_or_default();
        Ok(format!(
            "message={}, project={}, trace_id={}",
            inner.message, path, inner.trace_id
        ))
    }

    /// Ensure a project exists and create it if confirmation is granted.
    /// 确保项目存在，必要时按确认参数创建项目。
    pub async fn ensure_project(
        &self,
        project_path: &str,
        confirm_create: bool,
    ) -> Result<String, String> {
        let req = tonic::Request::new(EnsureProjectRequest {
            project_path: project_path.to_string(),
            confirm_create,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .ensure_project(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let path = inner
            .project
            .as_ref()
            .map(|p| p.display_path.clone())
            .unwrap_or_default();
        Ok(format!(
            "message={}, exists={}, project={}, trace_id={}",
            inner.message, inner.exists, path, inner.trace_id
        ))
    }

    /// Delete the specified project and its derived data.
    /// 删除指定项目及其派生数据。
    pub async fn delete_project(
        &self,
        project_path: &str,
        confirm_delete: bool,
    ) -> Result<String, String> {
        let req = tonic::Request::new(DeleteProjectRequest {
            project_path: project_path.to_string(),
            confirm_delete,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .delete_project(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "message={}, needs_confirm={}, deleted_sessions={}, deleted_messages={}, deleted_memories={}, deleted_vector={}, trace_id={}",
            inner.message,
            inner.needs_confirm,
            inner.deleted_sessions,
            inner.deleted_messages,
            inner.deleted_memories,
            inner.deleted_vector_rows,
            inner.trace_id
        ))
    }

    /// Migrate project data to a new path.
    /// 迁移项目数据到新路径。
    pub async fn migrate_project(
        &self,
        source: &str,
        target: &str,
        confirm: bool,
    ) -> Result<String, String> {
        let req = tonic::Request::new(MigrateProjectRequest {
            source_project_path: source.to_string(),
            target_project_path: target.to_string(),
            confirm_migrate: confirm,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .migrate_project(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "message={}, needs_confirm={}, migrated_sessions={}, migrated_messages={}, migrated_memories={}, rebuilt_vector={}, trace_id={}",
            inner.message,
            inner.needs_confirm,
            inner.migrated_sessions,
            inner.migrated_messages,
            inner.migrated_memories,
            inner.rebuilt_vector_rows,
            inner.trace_id
        ))
    }

    /// Resolve a user reference and optionally create the user.
    /// 解析用户引用并可选创建用户。
    pub async fn resolve_user(
        &self,
        user_ref: &str,
        confirm_create: bool,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ResolveUserRequest {
            user_ref: user_ref.to_string(),
            confirm_create,
        });
        let mut client = self.client.lock().await;
        let resp = client.resolve_user(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let user_info = inner
            .user
            .as_ref()
            .map(|u| format!("{}({})", u.user_name, u.user_id))
            .unwrap_or_default();
        Ok(format!(
            "message={}, user={}, created={}, exists={}, trace_id={}",
            inner.message, user_info, inner.created, inner.exists, inner.trace_id
        ))
    }

    /// List all user summaries.
    /// 列出所有用户摘要。
    pub async fn list_users(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_users(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let users: Vec<String> = inner
            .users
            .iter()
            .map(|u| format!("{}({})", u.user_name, u.user_id))
            .collect();
        Ok(format!(
            "users={}, trace_id={}",
            users.join(", "),
            inner.trace_id
        ))
    }

    /// Delete a user and its related data.
    /// 删除用户及其相关数据。
    pub async fn delete_user(
        &self,
        user_ref: &str,
        confirmation_code: &str,
    ) -> Result<String, String> {
        let req = tonic::Request::new(DeleteUserRequest {
            user_ref: user_ref.to_string(),
            confirmation_code: confirmation_code.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client.delete_user(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let user_info = inner
            .user
            .as_ref()
            .map(|u| format!("{}({})", u.user_name, u.user_id))
            .unwrap_or_default();
        Ok(format!(
            "message={}, requires_confirmation={}, user={}, deleted_sessions={}, deleted_messages={}, deleted_memories={}, deleted_vector={}, trace_id={}",
            inner.message,
            inner.requires_confirmation,
            user_info,
            inner.deleted_sessions,
            inner.deleted_messages,
            inner.deleted_memories,
            inner.deleted_vector_rows,
            inner.trace_id
        ))
    }

    /// Fetch profile nodes.
    /// 读取画像节点。
    pub async fn get_profile_nodes(
        &self,
        target: i32,
        user_id: u64,
        project_id: u64,
        limit: u32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(GetProfileNodesRequest {
            target,
            user_id,
            project_id,
            limit,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .get_profile_nodes(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "node_count={}, trace_id={}",
            inner.nodes.len(),
            inner.trace_id
        ))
    }

    /// Fetch the aggregated profile bundle text.
    /// 读取画像聚合文本。
    pub async fn get_profile_bundle(
        &self,
        user_id: u64,
        project_id: u64,
        mode: i32,
        include_exp: Option<bool>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(GetProfileBundleRequest {
            user_id,
            project_id,
            mode,
            include_explanation: include_exp,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .get_profile_bundle(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "combined_text_len={}, trace_id={}",
            inner.combined_text.len(),
            inner.trace_id
        ))
    }

    /// Apply a profile instruction.
    /// 写入画像指令。
    pub async fn apply_profile_instruction(
        &self,
        target: i32,
        user_id: u64,
        project_id: u64,
        instruction: &str,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ApplyProfileInstructionRequest {
            target,
            user_id,
            project_id,
            instruction: instruction.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client
            .apply_profile_instruction(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "instruction_id={}, accepted_nodes={}, retired_nodes={}, trace_id={}",
            inner.instruction_id,
            inner.accepted_nodes.len(),
            inner.retired_nodes.len(),
            inner.trace_id
        ))
    }

    /// Execute memory-event search.
    /// 执行记忆事件检索。
    pub async fn search_memory_events(
        &self,
        user_id: u64,
        project_id: u64,
        queries: Vec<String>,
        top_k: u32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(SearchMemoryEventsRequest {
            user_id,
            project_id,
            queries,
            top_k,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .search_memory_events(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let total: usize = inner.results.iter().map(|r| r.hits.len()).sum();
        Ok(format!(
            "total_hits={}, query_groups={}, trace_id={}",
            total,
            inner.results.len(),
            inner.trace_id
        ))
    }

    /// Load conversation details by turn ids.
    /// 按 turn_id 批量读取对话详情。
    pub async fn get_turn_details(&self, turn_ids: Vec<u64>) -> Result<String, String> {
        let req = tonic::Request::new(GetTurnDetailsRequest { turn_ids });
        let mut client = self.client.lock().await;
        let resp = client
            .get_turn_details(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "turns_loaded={}, trace_id={}",
            inner.turns.len(),
            inner.trace_id
        ))
    }

    /// Write structured memories.
    /// 写入结构化记忆。
    pub async fn write_memories(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        items: Vec<WriteMemoryItem>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(WriteMemoriesRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            items,
        });
        let mut client = self.client.lock().await;
        let resp = client
            .write_memories(req)
            .await
            .map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let deduped: usize = inner.items.iter().filter(|i| i.deduped).count();
        Ok(format!(
            "written={}, deduped={}, trace_id={}",
            inner.items.len(),
            deduped,
            inner.trace_id
        ))
    }

    /// Trigger conversation compaction.
    /// 触发对话压缩。
    pub async fn chat_compact(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ChatCompactRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
        });
        let mut client = self.client.lock().await;
        let resp = client.chat_compact(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "accepted={}, updated={}, compacted_turn_id={}, trace_id={}",
            inner.accepted, inner.updated, inner.compacted_turn_id, inner.trace_id
        ))
    }

    /// Execute the PreCheck flow.
    /// 执行 PreCheck。
    pub async fn pre_check(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        user_content: &str,
        recall_mode: i32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(PreCheckRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            user_content: user_content.to_string(),
            recall_mode,
        });
        let mut client = self.client.lock().await;
        let resp = client.pre_check(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "should_inject={}, context_items={}, degraded={}, trace_id={}",
            inner.should_inject,
            inner.context_items.len(),
            inner.degraded,
            inner.trace_id
        ))
    }

    /// Execute the PostAction flow.
    /// 执行 PostAction。
    pub async fn post_action(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        user_content: &str,
        assistant_content: &str,
        timeline: Vec<PostActionTimelineItem>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(PostActionRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            user_content: user_content.to_string(),
            assistant_content: assistant_content.to_string(),
            timeline,
        });
        let mut client = self.client.lock().await;
        let resp = client.post_action(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!(
            "accepted={}, trace_id={}",
            inner.accepted, inner.trace_id
        ))
    }
}
