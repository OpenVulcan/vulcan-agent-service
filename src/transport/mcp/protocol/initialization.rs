//! MCP initialization and request context DTOs.
//! MCP 初始化与请求上下文 DTO。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::capabilities::ServerCapabilities;

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
    /// Optional host-side override used to force the effective client match name for policy resolution.
    /// 宿主侧可选覆盖值，用于强制指定策略解析时使用的客户端匹配名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_match_name_override: Option<String>,
    /// Trusted exact client name used by controlled transports such as gRPC to bypass generic matching.
    /// gRPC 等受控传输使用的受信任精确客户端名称，用于绕过通用匹配。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_client_name: Option<String>,
    /// Whether generic client-name overrides from environment or headers must be ignored.
    /// 是否必须忽略来自环境变量或请求头的通用客户端名称覆盖。
    #[serde(default)]
    pub disable_client_match_overrides: bool,
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
            client_match_name_override: None,
            exact_client_name: None,
            disable_client_match_overrides: false,
            client_capabilities: default_request_context_capabilities(),
        }
    }
}
