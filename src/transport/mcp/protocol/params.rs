//! MCP JSON-RPC params parsing helpers.
//! MCP JSON-RPC params 解析辅助函数。

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Deserialize required JSON-RPC params without inventing a default payload.
/// 不虚构默认载荷，反序列化必填 JSON-RPC params。
///
/// Type parameter: `T` is the target DTO type for the MCP method params.
/// 类型参数：`T` 是 MCP 方法 params 对应的目标 DTO 类型。
///
/// Parameters: `method_name` is used in diagnostics, and `params` is the raw optional JSON-RPC params value.
/// 参数：`method_name` 用于诊断信息，`params` 是原始可选 JSON-RPC params 值。
///
/// Returns: the parsed DTO, or a diagnostic string for missing or invalid params.
/// 返回：解析后的 DTO，或缺失/无效 params 的诊断字符串。
pub fn parse_required_params<T>(method_name: &str, params: Option<Value>) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let params = params.ok_or_else(|| format!("{method_name} params are required."))?;
    serde_json::from_value(params).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mcp::protocol::InitializeRequest;
    use serde_json::json;

    /// Required params parsing should reject an omitted JSON-RPC params field before deserialization.
    /// 必填 params 解析应在反序列化前拒绝省略的 JSON-RPC params 字段。
    #[test]
    fn parse_required_params_rejects_missing_params() {
        let error = parse_required_params::<InitializeRequest>("initialize", None)
            .expect_err("missing initialize params should be rejected explicitly");

        assert_eq!(error, "initialize params are required.");
    }

    /// Required params parsing should preserve serde diagnostics for explicit invalid payloads.
    /// 必填 params 解析应保留显式无效载荷的 serde 诊断。
    #[test]
    fn parse_required_params_preserves_invalid_payload_error() {
        let error = parse_required_params::<InitializeRequest>("initialize", Some(Value::Null))
            .expect_err("explicit null params should remain invalid");

        assert!(error.contains("invalid type: null"));
    }

    /// Required params parsing should deserialize a complete initialize payload.
    /// 必填 params 解析应反序列化完整 initialize 载荷。
    #[test]
    fn parse_required_params_accepts_complete_payload() {
        let request: InitializeRequest = parse_required_params(
            "initialize",
            Some(json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "unit-test",
                    "version": "1.0.0"
                }
            })),
        )
        .expect("complete initialize params should parse");

        assert_eq!(request.protocol_version, "2025-11-25");
        assert_eq!(
            request
                .client_info
                .as_ref()
                .map(|client| client.name.as_str()),
            Some("unit-test")
        );
    }
}
