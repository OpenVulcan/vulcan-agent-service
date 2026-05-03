use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Transport-neutral client identity captured by host runtime adapters.
/// 宿主运行时适配器捕获的传输无关客户端身份。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeClientInfo {
    /// Client display or product name reported by the caller.
    /// 调用方上报的客户端显示名或产品名。
    pub name: String,
    /// Client version reported by the caller.
    /// 调用方上报的客户端版本。
    pub version: String,
}

/// Transport-neutral request context shared by config, host core, and protocol adapters.
/// config、host core 与协议适配器共享的传输无关请求上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequestContext {
    /// Transport name for the current request, for example streamable_http or grpc_unary.
    /// 当前请求所属的传输类型，例如 streamable_http 或 grpc_unary。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Stateful session identifier when the request belongs to an initialized session.
    /// 若当前请求属于已初始化会话，则记录对应的状态化会话 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Negotiated protocol version for the current client session when a protocol has one.
    /// 当前客户端会话在协议存在版本协商时得到的协议版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Client identity reported by the transport.
    /// 传输层上报的客户端身份。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<RuntimeClientInfo>,
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
    /// Raw client capabilities payload preserved from the adapter.
    /// 从适配器保留的客户端能力原始负载。
    #[serde(default = "default_runtime_capabilities")]
    pub client_capabilities: Value,
}

impl Default for RuntimeRequestContext {
    /// Build an empty request context for adapters that have not completed a session handshake.
    /// 为尚未完成会话握手的适配器构建空请求上下文。
    fn default() -> Self {
        Self {
            transport: None,
            session_id: None,
            protocol_version: None,
            client_info: None,
            client_match_name_override: None,
            exact_client_name: None,
            disable_client_match_overrides: false,
            client_capabilities: default_runtime_capabilities(),
        }
    }
}

/// Build the default empty capability object used by runtime request contexts.
/// 构建运行时请求上下文使用的默认空能力对象。
fn default_runtime_capabilities() -> Value {
    Value::Object(serde_json::Map::new())
}
