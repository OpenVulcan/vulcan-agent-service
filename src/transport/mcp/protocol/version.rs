//! MCP protocol version negotiation helpers.
//! MCP 协议版本协商辅助类型与函数。

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
