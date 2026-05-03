use crate::config::client_budget::CLIENT_MATCH_NAME_OVERRIDE_HEADER;
use crate::transport::http::helpers::{
    client_match_name_override_header_value, merge_header_client_match_name_override,
};
use crate::transport::mcp::protocol::RequestContext;
use axum::http::{HeaderMap, HeaderValue};

/// Header parsing should read the exact override value and ignore absent headers.
/// 请求头解析应读取明确覆盖值，并在缺失时返回空结果。
#[test]
fn client_match_name_override_header_value_reads_override_when_present() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CLIENT_MATCH_NAME_OVERRIDE_HEADER,
        HeaderValue::from_static("qoder"),
    );
    assert_eq!(
        client_match_name_override_header_value(&headers).as_deref(),
        Some("qoder")
    );

    let empty_headers = HeaderMap::new();
    assert_eq!(
        client_match_name_override_header_value(&empty_headers),
        None
    );
}

/// Request-level header overrides should replace any stored session override for the current request.
/// 请求级请求头覆盖值应替换当前请求使用的已保存会话覆盖值。
#[test]
fn merge_header_client_match_name_override_replaces_stored_override() {
    let request_context = RequestContext {
        client_match_name_override: Some("mcphost".to_string()),
        ..RequestContext::default()
    };

    let merged =
        merge_header_client_match_name_override(request_context, Some("qoder".to_string()));
    assert_eq!(merged.client_match_name_override.as_deref(), Some("qoder"));
}
