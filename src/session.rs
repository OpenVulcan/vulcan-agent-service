use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// A single MCP session. Each session has a channel for sending responses
/// back to the client (used for HTTP Streamable SSE streaming).
pub struct Session {
    /// Channel sender for pushing responses to the SSE stream
    pub tx: mpsc::Sender<Value>,
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new session and return its (session_id, rx) where rx is the receiver
    /// for responses that should be streamed to the client.
    pub async fn create(&self) -> (String, mpsc::Receiver<Value>) {
        let session_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel::<Value>(256);
        self.sessions.lock().await.insert(session_id.clone(), Session { tx });
        (session_id, rx)
    }

    /// Send a response value into a session's stream.
    pub async fn send(&self, session_id: &str, value: Value) -> Result<(), ()> {
        let sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            session.tx.send(value).await.map_err(|_| ())?;
            return Ok(());
        }
        Err(())
    }

    /// Remove a session (client called DELETE or connection dropped).
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    /// Check if a session exists.
    pub async fn exists(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }
}

// Legacy SSE session: each SSE connection gets its own broadcast channel
pub struct SseSession {
    pub tx: mpsc::Sender<Value>,
}

#[derive(Clone)]
pub struct SseSessionManager {
    sessions: Arc<Mutex<HashMap<String, SseSession>>>,
    /// Counter for generating session IDs
    counter: Arc<Mutex<u64>>,
}

impl SseSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a new SSE session and return (session_id, rx).
    pub async fn create(&self) -> (String, mpsc::Receiver<Value>) {
        let mut counter = self.counter.lock().await;
        *counter += 1;
        let session_id = format!("sse-{}", counter);
        let (tx, rx) = mpsc::channel::<Value>(256);
        self.sessions.lock().await.insert(session_id.clone(), SseSession { tx });
        (session_id, rx)
    }

    /// Get the POST message endpoint URL for a session.
    pub fn message_endpoint(&self, session_id: &str, base_url: &str) -> String {
        format!("{}/message?sessionId={}", base_url, session_id)
    }

    /// Send a message into an SSE session.
    pub async fn send(&self, session_id: &str, value: Value) -> Result<(), ()> {
        let sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            session.tx.send(value).await.map_err(|_| ())?;
            return Ok(());
        }
        Err(())
    }

    /// Remove an SSE session.
    pub async fn remove(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    /// Get session ID from query params (for legacy POST /message).
    pub fn session_id_from_query(query: &str) -> Option<String> {
        query
            .trim_start_matches('?')
            .split('&')
            .find(|p| p.starts_with("sessionId="))
            .map(|p| p.strip_prefix("sessionId=").unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
    }
}
