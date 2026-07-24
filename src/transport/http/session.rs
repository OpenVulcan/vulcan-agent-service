use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::transport::mcp::protocol::RequestContext;

/// Streamable HTTP session metadata
/// Streamable HTTP 会话元数据。
pub struct Session {
    /// Negotiated MCP protocol version for this session
    /// 当前会话协商后的 MCP 协议版本。
    pub protocol_version: String,
    /// Request-scoped client registration context
    /// 请求级客户端注册上下文。
    pub request_context: RequestContext,
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create a new streamable HTTP session
    /// 创建一个新的 Streamable HTTP 会话。
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a stateful session after initialize with a negotiated protocol version.
    /// 在 initialize 成功且已获得协商协议版本后创建状态化会话。
    ///
    /// Parameters: `request_context` is the initialized client context that must include `protocol_version`.
    /// 参数：`request_context` 是已初始化客户端上下文，必须包含 `protocol_version`。
    ///
    /// Returns: the new session id, or an error when the negotiated protocol version is missing or empty.
    /// 返回：新会话 ID；当协商协议版本缺失或为空时返回错误。
    pub async fn create(&self, mut request_context: RequestContext) -> Result<String, String> {
        let Some(protocol_version) = request_context.protocol_version.clone() else {
            return Err("streamable session requires negotiated protocol version".to_string());
        };
        if protocol_version.trim().is_empty() {
            return Err("streamable session requires negotiated protocol version".to_string());
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        request_context.session_id = Some(session_id.clone());
        self.sessions.lock().await.insert(
            session_id.clone(),
            Session {
                protocol_version,
                request_context,
            },
        );
        Ok(session_id)
    }

    /// Remove a session entirely
    /// 完全移除一个会话。
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    /// Check whether a session exists
    /// 检查会话是否存在。
    pub async fn exists(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }

    /// Read the negotiated protocol version for a session
    /// 读取会话协商后的协议版本。
    pub async fn protocol_version(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.protocol_version.clone())
    }

    /// Read the stored request context for a session
    /// 读取会话持有的请求上下文。
    pub async fn request_context(&self, session_id: &str) -> Option<RequestContext> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.request_context.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Streamable session creation should reject contexts without negotiated protocol versions.
    /// Streamable 会话创建应拒绝缺少协商协议版本的上下文。
    #[tokio::test]
    async fn session_manager_create_rejects_missing_protocol_version() {
        // Build one empty manager so the test isolates session creation validation.
        // 构造空管理器，使测试只聚焦会话创建校验。
        let manager = SessionManager::new();
        // Build a request context that simulates a caller skipping initialize negotiation.
        // 构造模拟调用方跳过 initialize 协商的请求上下文。
        let request_context = RequestContext::default();

        let error = manager
            .create(request_context)
            .await
            .expect_err("missing protocol version should be rejected");

        assert_eq!(
            error,
            "streamable session requires negotiated protocol version"
        );
    }

    /// Streamable session creation should reject empty negotiated protocol versions.
    /// Streamable 会话创建应拒绝空的协商协议版本。
    #[tokio::test]
    async fn session_manager_create_rejects_empty_protocol_version() {
        // Build one empty manager so no existing session state can affect validation.
        // 构造空管理器，避免既有会话状态影响校验。
        let manager = SessionManager::new();
        // Build a request context whose protocol version is present but empty.
        // 构造协议版本字段存在但为空的请求上下文。
        let request_context = RequestContext {
            protocol_version: Some(String::new()),
            ..RequestContext::default()
        };

        let error = manager
            .create(request_context)
            .await
            .expect_err("empty protocol version should be rejected");

        assert_eq!(
            error,
            "streamable session requires negotiated protocol version"
        );
    }

    /// Streamable session creation should persist the negotiated protocol version and generated session id.
    /// Streamable 会话创建应持久化协商协议版本与生成的会话 ID。
    #[tokio::test]
    async fn session_manager_create_persists_protocol_version_and_session_id() {
        // Build one empty manager to verify the first created streamable session.
        // 构造空管理器，用于验证第一个创建的 streamable 会话。
        let manager = SessionManager::new();
        // Build a request context matching the post-initialize production path.
        // 构造与生产 initialize 后路径一致的请求上下文。
        let request_context = RequestContext {
            protocol_version: Some("2025-06-18".to_string()),
            ..RequestContext::default()
        };

        let session_id = manager
            .create(request_context)
            .await
            .expect("valid protocol version should create a session");

        assert_eq!(
            manager.protocol_version(&session_id).await.as_deref(),
            Some("2025-06-18")
        );
        assert_eq!(
            manager
                .request_context(&session_id)
                .await
                .and_then(|context| context.session_id),
            Some(session_id)
        );
    }
}

/// Legacy SSE session: each SSE connection gets its own broadcast channel /
/// 旧版 SSE 会话：每个 SSE 连接各自拥有独立通道。
pub struct SseSession {
    pub tx: mpsc::Sender<Value>,
}

#[derive(Clone)]
pub struct SseSessionManager {
    sessions: Arc<Mutex<HashMap<String, SseSession>>>,
    /// Counter for generating session IDs
    /// 用于生成会话 ID 的计数器。
    counter: Arc<Mutex<u64>>,
}

impl SseSessionManager {
    /// Create a legacy SSE session manager
    /// 创建旧版 SSE 会话管理器。
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a new legacy SSE session
    /// 创建一个新的旧版 SSE 会话。
    pub async fn create(&self) -> (String, mpsc::Receiver<Value>) {
        let mut counter = self.counter.lock().await;
        *counter += 1;
        let session_id = format!("sse-{}", counter);
        let (tx, rx) = mpsc::channel::<Value>(256);
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), SseSession { tx });
        (session_id, rx)
    }

    /// Send a message into a legacy SSE session
    /// 向旧版 SSE 会话推送消息。
    pub async fn send(&self, session_id: &str, value: Value) -> Result<(), ()> {
        // Clone the sender before awaiting so a slow or closed receiver never holds the session map lock.
        // 在 await 前克隆发送器，避免慢速或已关闭的接收端长期占用会话表锁。
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions.get(session_id).map(|session| session.tx.clone())
        };
        // Missing session IDs are explicit delivery failures for legacy POST /message.
        // 缺失的会话 ID 对旧版 POST /message 来说是明确的投递失败。
        let Some(tx) = tx else {
            return Err(());
        };
        if tx.send(value).await.is_ok() {
            return Ok(());
        }
        self.remove(session_id).await;
        Err(())
    }

    /// Check whether a legacy SSE session exists.
    /// 检查旧版 SSE 会话是否存在。
    /// Parameters: `session_id` is the legacy SSE session identifier to look up.
    /// 参数：`session_id` 是要查询的旧版 SSE 会话标识。
    /// Returns true when the session is still registered.
    /// 当会话仍被注册时返回 true。
    pub async fn exists(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }

    /// Remove a legacy SSE session
    /// 移除旧版 SSE 会话。
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }
}
