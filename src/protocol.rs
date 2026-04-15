use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================
// Protocol version constants
// ============================================================

/// Latest supported protocol version (primary)
pub const PROTOCOL_VERSION_LATEST: &str = "2025-11-25";
/// Compatible older versions
pub const PROTOCOL_VERSION_COMPATIBLE: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Negotiate protocol version: pick the highest version that both sides support.
/// The server supports: 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05
pub fn negotiate_version(client_version: &str) -> Option<&'static str> {
    if client_version == PROTOCOL_VERSION_LATEST {
        return Some(PROTOCOL_VERSION_LATEST);
    }
    for &v in PROTOCOL_VERSION_COMPATIBLE {
        if v == client_version {
            return Some(v);
        }
    }
    None
}

/// Check if a feature is available in the negotiated version.
pub fn has_feature(version: &str, feature: FeatureFlag) -> bool {
    match feature {
        // Features in 2024-11-05 baseline
        FeatureFlag::BasicTools
        | FeatureFlag::Resources
        | FeatureFlag::Prompts
        | FeatureFlag::Ping => true,

        // Features added in 2025-03-26
        FeatureFlag::Sampling
        | FeatureFlag::Roots
        | FeatureFlag::Completions
        | FeatureFlag::Elicitation
        | FeatureFlag::ProgressToken
        | FeatureFlag::Cancellation => version == "2025-03-26" || version == "2025-06-18" || version == "2025-11-25",

        // Features added in 2025-11-25
        FeatureFlag::Streaming
        | FeatureFlag::StructuredLogging
        | FeatureFlag::ToolAnnotations
        | FeatureFlag::AudioContent
        | FeatureFlag::EmbeddedResource => version == "2025-06-18" || version == "2025-11-25",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureFlag {
    // 2024-11-05 baseline
    BasicTools,
    Resources,
    Prompts,
    Ping,

    // 2025-03-26 additions
    Sampling,
    Roots,
    Completions,
    Elicitation,
    ProgressToken,
    Cancellation,

    // 2025-11-25 additions
    Streaming,
    StructuredLogging,
    ToolAnnotations,
    AudioContent,
    EmbeddedResource,
}

// ============================================================
// Server & Client info
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

// ============================================================
// Capabilities (server → client)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingCapability {
    // 2025-03-26+: server can request client-side LLM sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingCapability {
    // 2025-11-25
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionsCapability {
    // 2025-03-26
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<CompletionsCapability>,
}

// ============================================================
// Client capabilities (client → server)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientRootsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSamplingCapability {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientExperimentalCapabilities {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<ClientRootsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<ClientSamplingCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<ClientExperimentalCapabilities>,
}

// ============================================================
// Initialize
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<ClientInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Request-scoped context that can be exposed to Lua skills.
/// 暴露给 Lua 技能的请求级上下文信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Transport name for the current request, for example streamable_http or grpc_unary.
    /// 当前请求所属的传输类型，例如 streamable_http 或 grpc_unary。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Stateful session identifier when the request belongs to an initialized session.
    /// 若当前请求属于已初始化会话，则记录对应的状态化会话 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Negotiated MCP protocol version for the current client session.
    /// 当前客户端会话协商得到的 MCP 协议版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Client identity reported during initialize or registration.
    /// 客户端在 initialize 或注册阶段上报的客户端标识信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<ClientInfo>,
    /// Raw client capabilities payload preserved from initialize.
    /// 从 initialize 中保留的客户端能力原始负载。
    #[serde(default = "default_request_context_capabilities")]
    pub client_capabilities: Value,
}

fn default_request_context_capabilities() -> Value {
    Value::Object(serde_json::Map::new())
}

impl Default for RequestContext {
    fn default() -> Self {
        Self {
            transport: None,
            session_id: None,
            protocol_version: None,
            client_info: None,
            client_capabilities: default_request_context_capabilities(),
        }
    }
}

// ============================================================
// Annotations (2025-11-25)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

// ============================================================
// Content types
// ============================================================

/// Content block used in tool results, resource reads, and prompt messages.
/// 2025-11-25 adds image and audio content types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(TextContent),
    #[serde(skip_serializing, skip_deserializing)] // only sent by server in 2025-11-25
    Image(ImageContent),
    #[serde(skip_serializing, skip_deserializing)] // only sent by server in 2025-11-25
    Audio(AudioContent),
    Resource(EmbeddedResource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    #[serde(default = "text_type_default")]
    pub r#type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub data: String,       // base64-encoded
    pub mime_type: String,  // e.g. "image/png"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioContent {
    pub data: String,       // base64-encoded
    pub mime_type: String,  // e.g. "audio/wav"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub resource: ResourceContents,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

// For tool results that only emit text content (simpler path)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<TextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ============================================================
// Meta (progress tokens — 2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<Value>, // string or number
}

// ============================================================
// Tools
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Tool annotations (2025-11-25)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// If true, the tool does not modify the user's system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// If true, the tool should be considered destructive / irreversible
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// If true, the tool must be confirmed by the user before calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_confirmation_required: Option<bool>,
    /// Unique ID for the tool (used for streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: InputSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _meta: Option<Meta>, // progress_token (2025-03-26+)
}

// ============================================================
// Resources
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Size in bytes (2025-03-26+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContents {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Blob content for binary resources (2025-03-26+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReadResult {
    pub contents: Vec<ResourceContents>,
}

// Resource template (2025-03-26+)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplate {
    pub uri_template: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

// ============================================================
// Prompts
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    pub role: String,
    pub content: TextContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptGetResult {
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

// ============================================================
// Roots (2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsListResult {
    pub roots: Vec<Root>,
}

// ============================================================
// Sampling (2025-03-26+) — server requests client to call an LLM
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingMessage {
    pub role: String,
    pub content: TextContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingRequest {
    pub messages: Vec<SamplingMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingResult {
    pub role: String,
    pub content: TextContent,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

// ============================================================
// Completions (2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    pub ref_value: String,
    pub argument: ArgumentInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgumentInfo {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResult {
    pub completion: CompletionInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionInner {
    pub values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

// ============================================================
// Elicitation (2025-03-26+) — server asks client to get user input
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequest {
    pub message: String,
    pub requested_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResult {
    pub action: String, // "accept" | "decline"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

// ============================================================
// Cancellation (2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancellationNotification {
    pub request_id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ============================================================
// Logging (2025-11-25)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingMessage {
    pub level: String, // "debug" | "info" | "notice" | "warning" | "error" | "critical" | "alert" | "emergency"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    pub data: Value,
}

// ============================================================
// Progress notification (2025-03-26+)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotification {
    pub progress_token: Value,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================
// Helper constructors
// ============================================================

impl Tool {
    pub fn new(name: &str, description: &str, properties: Value, required: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: InputSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(required),
            },
            annotations: None,
        }
    }

    pub fn with_annotations(
        name: &str,
        description: &str,
        properties: Value,
        required: Vec<String>,
        annotations: ToolAnnotations,
    ) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: InputSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(required),
            },
            annotations: Some(annotations),
        }
    }
}

impl TextContent {
    pub fn text(text: &str) -> Self {
        Self {
            r#type: "text".to_string(),
            text: text.to_string(),
            annotations: None,
        }
    }
}

fn text_type_default() -> String {
    "text".to_string()
}

impl ResourceContents {
    pub fn text(uri: &str, text: &str, mime_type: Option<String>) -> Self {
        Self {
            uri: uri.to_string(),
            mime_type,
            text: Some(text.to_string()),
            blob: None,
        }
    }
}
