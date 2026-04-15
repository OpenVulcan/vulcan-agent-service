use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::protocol::RequestContext;

/// Streamable HTTP session metadata / Streamable HTTP 会话元数据。
pub struct Session {
    /// Negotiated MCP protocol version for this session / 当前会话协商后的 MCP 协议版本。
    pub protocol_version: String,
    /// Request-scoped client registration context / 请求级客户端注册上下文。
    pub request_context: RequestContext,
    /// Optional sender bound to the active GET /mcp SSE stream / 绑定到当前 GET /mcp SSE 流的可选发送器。
    pub tx: Option<mpsc::Sender<Value>>,
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create a new streamable HTTP session / 创建一个新的 Streamable HTTP 会话。
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a stateful session after initialize / 在 initialize 成功后创建状态化会话。
    pub async fn create(&self, mut request_context: RequestContext) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let protocol_version = request_context.protocol_version.clone().unwrap_or_default();
        request_context.session_id = Some(session_id.clone());
        self.sessions.lock().await.insert(
            session_id.clone(),
            Session {
                protocol_version,
                request_context,
                tx: None,
            },
        );
        session_id
    }

    /// Attach a single active SSE stream to the session / 为会话附加一个唯一活动 SSE 流。
    pub async fn attach_stream(&self, session_id: &str) -> Option<mpsc::Receiver<Value>> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id)?;
        let (tx, rx) = mpsc::channel::<Value>(256);
        session.tx = Some(tx);
        Some(rx)
    }

    /// Detach the active SSE stream from the session / 从会话上卸载当前活动 SSE 流。
    pub async fn detach_stream(&self, session_id: &str) {
        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            session.tx = None;
        }
    }

    /// Send a server-originated message into the active stream / 向当前活动流推送服务端消息。
    pub async fn send(&self, session_id: &str, value: Value) -> Result<(), ()> {
        let sender = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .and_then(|session| session.tx.clone())
        };

        let Some(tx) = sender else {
            return Err(());
        };

        if tx.send(value).await.is_ok() {
            return Ok(());
        }

        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            session.tx = None;
        }
        Err(())
    }

    /// Remove a session entirely / 完全移除一个会话。
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    /// Check whether a session exists / 检查会话是否存在。
    pub async fn exists(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }

    /// Read the negotiated protocol version for a session / 读取会话协商后的协议版本。
    pub async fn protocol_version(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.protocol_version.clone())
    }

    /// Read the stored request context for a session / 读取会话持有的请求上下文。
    pub async fn request_context(&self, session_id: &str) -> Option<RequestContext> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.request_context.clone())
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
    /// Counter for generating session IDs / 用于生成会话 ID 的计数器。
    counter: Arc<Mutex<u64>>,
}

impl SseSessionManager {
    /// Create a legacy SSE session manager / 创建旧版 SSE 会话管理器。
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a new legacy SSE session / 创建一个新的旧版 SSE 会话。
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

    /// Get the POST message endpoint URL for a session / 获取会话对应的 POST 消息端点。
    pub fn message_endpoint(&self, session_id: &str, base_url: &str) -> String {
        format!("{}/message?sessionId={}", base_url, session_id)
    }

    /// Send a message into a legacy SSE session / 向旧版 SSE 会话推送消息。
    pub async fn send(&self, session_id: &str, value: Value) -> Result<(), ()> {
        let sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            session.tx.send(value).await.map_err(|_| ())?;
            return Ok(());
        }
        Err(())
    }

    /// Remove a legacy SSE session / 移除旧版 SSE 会话。
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    /// Get session ID from query params (for legacy POST /message) /
    /// 从查询参数中提取旧版 POST /message 使用的 sessionId。
    pub fn session_id_from_query(query: &str) -> Option<String> {
        query
            .trim_start_matches('?')
            .split('&')
            .find(|p| p.starts_with("sessionId="))
            .map(|p| p.strip_prefix("sessionId=").unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
    }
}
