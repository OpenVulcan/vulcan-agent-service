//! MCP protocol version negotiation helpers.
//! MCP 协议版本协商辅助类型与函数。

// ============================================================
// Protocol version constants
// ============================================================

/// Latest supported protocol version.
/// 最新支持的协议版本。
pub const PROTOCOL_VERSION_LATEST: &str = "2025-11-25";

/// Compatible older protocol versions in descending preference order.
/// 按优先级降序排列的兼容旧协议版本。
pub const PROTOCOL_VERSION_COMPATIBLE: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Negotiate protocol version: pick the highest version that both sides support.
/// 协商协议版本：选择客户端与服务端共同支持的最高版本。
pub fn negotiate_version(client_version: &str) -> Option<&'static str> {
    if client_version == PROTOCOL_VERSION_LATEST {
        return Some(PROTOCOL_VERSION_LATEST);
    }
    PROTOCOL_VERSION_COMPATIBLE
        .iter()
        .copied()
        .find(|version| *version == client_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protocol negotiation should accept the latest version, compatible versions, and reject unknown versions.
    /// 协议协商应接受最新版本与兼容版本，并拒绝未知版本。
    #[test]
    fn negotiate_version_accepts_latest_compatible_and_rejects_unknown() {
        assert_eq!(
            negotiate_version(PROTOCOL_VERSION_LATEST),
            Some("2025-11-25")
        );
        assert_eq!(negotiate_version("2025-06-18"), Some("2025-06-18"));
        assert_eq!(negotiate_version("1900-01-01"), None);
    }
}
