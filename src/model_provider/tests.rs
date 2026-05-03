use super::*;
use crate::config::model_config::{
    EmbeddingModelConfig, LlmModelConfig, OpenAiCompatibleModelConfig,
};
use crate::config::model_config::{initialize_model_config_runtime_root, preload_model_config};
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

/// Captured one-shot HTTP request received by the local mock model provider.
/// 本地模型供应商假服务收到的单次 HTTP 请求记录。
struct MockProviderRequest {
    /// Request path including the OpenAI-compatible API endpoint path.
    /// 请求路径，包含 OpenAI-compatible API 端点路径。
    path: String,
    /// Authorization header captured from the request.
    /// 从请求中捕获的 Authorization 请求头。
    authorization: Option<String>,
    /// JSON request body captured from the request.
    /// 从请求中捕获的 JSON 请求体。
    body: Value,
}

/// Return one shared mutex used to serialize process-wide LuaSkills model callback tests.
/// 返回一个共享互斥锁，用于串行化进程级 LuaSkills 模型回调测试。
fn callback_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Build one unique temporary directory path for a model-provider test case.
/// 为模型供应商测试用例构建唯一临时目录路径。
fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "vulcan-mcp-model-provider-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ))
}

/// Build one minimal LuaSkills engine for checking the Lua-facing model capability surface.
/// 构建一个最小 LuaSkills 引擎，用于检查 Lua 面向模型能力面。
fn make_lua_engine() -> LuaEngine {
    LuaEngine::new(LuaEngineOptions {
        host_options: LuaRuntimeHostOptions::default(),
        pool_config: LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
    })
    .expect("Lua engine should be created")
}

/// Restore model config discovery to the repository template and re-apply callback registration.
/// 将模型配置发现恢复到仓库模板，并重新应用回调注册。
fn restore_default_model_callbacks() {
    let _ = initialize_model_config_runtime_root(None);
    let _ = preload_model_config();
    install_luaskills_model_callbacks();
}

/// Start a one-shot local HTTP server that returns the provided OpenAI-compatible response body.
/// 启动一个单次使用的本地 HTTP 服务，并返回指定 OpenAI-compatible 响应体。
fn start_mock_provider_server(response_body: Value) -> (String, JoinHandle<MockProviderRequest>) {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("mock provider should bind a local port");
    let base_url = format!(
        "http://{}/v1",
        listener
            .local_addr()
            .expect("mock provider local address should resolve")
    );
    let response_text = response_body.to_string();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("mock provider should receive one request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("mock provider read timeout should set");
        let request_bytes = read_http_request_bytes(&mut stream);
        let captured_request = parse_mock_provider_request(&request_bytes);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_text.len(),
            response_text
        );
        stream
            .write_all(response.as_bytes())
            .expect("mock provider response should write");
        captured_request
    });
    (base_url, handle)
}

/// Read exactly one HTTP request from the mock provider stream.
/// 从假服务连接中读取完整的单个 HTTP 请求。
fn read_http_request_bytes(stream: &mut TcpStream) -> Vec<u8> {
    let mut request_bytes = Vec::new();
    loop {
        if let Some(expected_len) = expected_http_request_len(&request_bytes)
            && request_bytes.len() >= expected_len
        {
            request_bytes.truncate(expected_len);
            return request_bytes;
        }
        let mut chunk = [0_u8; 512];
        let read_len = stream
            .read(&mut chunk)
            .expect("mock provider request should read");
        if read_len == 0 {
            return request_bytes;
        }
        request_bytes.extend_from_slice(&chunk[..read_len]);
    }
}

/// Return the total expected request byte length once headers and Content-Length are available.
/// 在请求头与 Content-Length 可用后返回预期完整请求字节长度。
fn expected_http_request_len(request_bytes: &[u8]) -> Option<usize> {
    let header_end = http_header_end_index(request_bytes)?;
    let header_text = std::str::from_utf8(&request_bytes[..header_end]).ok()?;
    let content_length = header_text
        .lines()
        .find_map(|line| parse_content_length_header(line))
        .unwrap_or(0);
    Some(header_end + content_length)
}

/// Return the byte index immediately after the HTTP header terminator.
/// 返回 HTTP 请求头结束符之后的字节索引。
fn http_header_end_index(request_bytes: &[u8]) -> Option<usize> {
    request_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

/// Parse one Content-Length header line.
/// 解析单行 Content-Length 请求头。
fn parse_content_length_header(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    if name.trim().eq_ignore_ascii_case("content-length") {
        value.trim().parse::<usize>().ok()
    } else {
        None
    }
}

/// Parse one captured HTTP request into the compact mock-provider request struct.
/// 将捕获到的 HTTP 请求解析为紧凑的假服务请求结构。
fn parse_mock_provider_request(request_bytes: &[u8]) -> MockProviderRequest {
    let request_text =
        std::str::from_utf8(request_bytes).expect("mock provider request should be UTF-8");
    let (header_text, body_text) = request_text
        .split_once("\r\n\r\n")
        .expect("mock provider request should contain headers");
    let path = header_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("mock provider request path should exist")
        .to_string();
    let authorization = header_text.lines().find_map(parse_authorization_header);
    let body = serde_json::from_str::<Value>(body_text)
        .expect("mock provider request body should be JSON");
    MockProviderRequest {
        path,
        authorization,
        body,
    }
}

/// Parse one Authorization header line.
/// 解析单行 Authorization 请求头。
fn parse_authorization_header(line: &str) -> Option<String> {
    let (name, value) = line.split_once(':')?;
    if name.trim().eq_ignore_ascii_case("authorization") {
        Some(value.trim().to_string())
    } else {
        None
    }
}

/// Write an isolated model configuration file for one mock-provider integration test.
/// 为单个假服务集成测试写入隔离模型配置文件。
fn write_mock_model_config(root: &std::path::Path, base_url: &str, body: &str) {
    let config_path = root.join("configs").join("model_config.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("model config directory should be created");
    std::fs::write(&config_path, body.replace("${MOCK_BASE_URL}", base_url))
        .expect("model config should be written");
}

/// Build one provider fixture with both embedding and LLM enabled.
/// 构建同时启用向量与 LLM 的供应商测试夹具。
fn provider_fixture() -> OpenAiCompatibleModelConfig {
    OpenAiCompatibleModelConfig {
        enabled: true,
        embedding: EmbeddingModelConfig {
            enabled: true,
            base_url: Some("https://embedding.example.test/v1/".to_string()),
            api_key: Some("sk-embed".to_string()),
            model: Some("embed-small".to_string()),
            timeout_ms: Some(1000),
            request_overrides: json!({
                "model": "must-not-win",
                "input": "must-not-win",
                "encoding_format": "float"
            }),
        },
        llm: LlmModelConfig {
            enabled: true,
            base_url: Some("https://llm.example.test/v1/".to_string()),
            api_key: Some("sk-llm".to_string()),
            model: Some("llm-small".to_string()),
            temperature: Some(0.1),
            max_tokens: Some(300),
            timeout_ms: Some(1000),
            request_overrides: json!({
                "model": "must-not-win",
                "messages": [],
                "stream": true,
                "enable_thinking": false
            }),
        },
    }
}

/// Embedding request bodies should preserve host-owned model/input while accepting safe overrides.
/// 向量请求体应保留宿主管理的 model/input，同时接受安全覆盖字段。
#[test]
fn embedding_request_body_preserves_reserved_fields() {
    let body = build_embedding_request_body(&provider_fixture(), "hello")
        .expect("embedding body should build");

    assert_eq!(body["model"], "embed-small");
    assert_eq!(body["input"], "hello");
    assert_eq!(body["encoding_format"], "float");
}

/// LLM request bodies should stay non-streaming and reject reserved override fields.
/// LLM 请求体应保持非流式，并拒绝保留字段覆盖。
#[test]
fn llm_request_body_forces_non_streaming_and_reserved_fields() {
    let body =
        build_llm_request_body(&provider_fixture(), "sys", "user").expect("llm body should build");

    assert_eq!(body["model"], "llm-small");
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["content"], "user");
    assert_eq!(body["enable_thinking"], false);
}

/// OpenAI-compatible embedding responses should parse vector dimensions and usage.
/// OpenAI-compatible 向量响应应解析向量维度与用量信息。
#[test]
fn embedding_response_parses_vector_and_usage() {
    let response = json!({
        "data": [
            { "embedding": [0.25, -1.0, 2.5] }
        ],
        "usage": {
            "prompt_tokens": 7,
            "total_tokens": 7
        }
    });

    let parsed = parse_embedding_response(&response).expect("embedding response should parse");

    assert_eq!(parsed.dimensions, 3);
    assert_eq!(parsed.vector, vec![0.25, -1.0, 2.5]);
    assert_eq!(
        parsed.usage,
        Some(ModelUsage {
            input_tokens: Some(7),
            output_tokens: None,
            total_tokens: Some(7),
        })
    );
}

/// OpenAI-compatible LLM responses should parse assistant text and usage.
/// OpenAI-compatible LLM 响应应解析 assistant 文本与用量信息。
#[test]
fn llm_response_parses_assistant_text_and_usage() {
    let response = json!({
        "choices": [
            { "message": { "content": "done" } }
        ],
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 2,
            "total_tokens": 7
        }
    });

    let parsed = parse_llm_response(&response).expect("llm response should parse");

    assert_eq!(parsed.assistant, "done");
    assert_eq!(
        parsed.usage,
        Some(ModelUsage {
            input_tokens: Some(5),
            output_tokens: Some(2),
            total_tokens: Some(7),
        })
    );
}

/// Provider HTTP errors should preserve sanitized provider message, code, and status.
/// 供应商 HTTP 错误应保留脱敏后的供应商消息、错误码与状态码。
#[test]
fn provider_error_preserves_sanitized_provider_fields() {
    let error = provider_error_from_http_status(
        StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"bad sk-secret","code":"bad_request"}}"#,
        "sk-secret",
    );

    assert_eq!(error.code, ModelErrorCode::ProviderError);
    assert_eq!(error.provider_message.as_deref(), Some("bad ***"));
    assert_eq!(error.provider_code.as_deref(), Some("bad_request"));
    assert_eq!(error.provider_status, Some(400));
}

/// Model errors should serialize into the stable Lua-facing error object shape.
/// 模型错误应序列化为稳定的 Lua 面向错误对象形态。
#[test]
fn model_error_serializes_stable_lua_error_shape() {
    let value = json!({
        "ok": false,
        "error": ModelError::new(ModelErrorCode::ModelUnavailable, "missing")
    });

    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "model_unavailable");
    assert_eq!(value["error"]["message"], "missing");
}

/// Endpoint joining should trim duplicate slashes while keeping the configured API base path.
/// 端点拼接应去除重复斜杠，同时保留配置中的 API 基础路径。
#[test]
fn endpoint_url_preserves_base_path() {
    let url = openai_endpoint_url("https://example.test/compatible-mode/v1/", "/embeddings")
        .expect("url should build");

    assert_eq!(url, "https://example.test/compatible-mode/v1/embeddings");
}

/// Embedding calls should send a real OpenAI-compatible HTTP request and parse the mock provider response.
/// 向量调用应发送真实 OpenAI-compatible HTTP 请求，并解析假供应商响应。
#[test]
fn model_embed_posts_to_openai_compatible_endpoint() {
    let _guard = callback_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = unique_test_dir("mock-embed");
    let (base_url, handle) = start_mock_provider_server(json!({
        "data": [
            { "embedding": [0.5, 0.25, -0.75] }
        ],
        "usage": {
            "prompt_tokens": 9,
            "total_tokens": 9
        }
    }));
    write_mock_model_config(
        &root,
        &base_url,
        r#"
openai_compatible:
  enabled: true
  embedding:
    enabled: true
    base_url: "${MOCK_BASE_URL}"
    api_key: "sk-embed-local"
    model: "embed-small"
    timeout_ms: 5000
    request_overrides:
      encoding_format: "float"
"#,
    );
    initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
    preload_model_config().expect("model config should preload");

    let response = model_embed("hello", None).expect("embedding call should succeed");
    let request = handle.join().expect("mock provider should finish");

    assert_eq!(response.vector, vec![0.5, 0.25, -0.75]);
    assert_eq!(response.dimensions, 3);
    assert_eq!(
        response.usage,
        Some(ModelUsage {
            input_tokens: Some(9),
            output_tokens: None,
            total_tokens: Some(9),
        })
    );
    assert_eq!(request.path, "/v1/embeddings");
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer sk-embed-local")
    );
    assert_eq!(request.body["model"], "embed-small");
    assert_eq!(request.body["input"], "hello");
    assert_eq!(request.body["encoding_format"], "float");

    restore_default_model_callbacks();
    let _ = std::fs::remove_dir_all(root);
}

/// LLM calls should send a non-streaming OpenAI-compatible chat request and parse the mock provider response.
/// LLM 调用应发送非流式 OpenAI-compatible chat 请求，并解析假供应商响应。
#[test]
fn model_llm_posts_non_streaming_chat_completion() {
    let _guard = callback_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = unique_test_dir("mock-llm");
    let (base_url, handle) = start_mock_provider_server(json!({
        "choices": [
            { "message": { "content": "mock assistant" } }
        ],
        "usage": {
            "prompt_tokens": 11,
            "completion_tokens": 4,
            "total_tokens": 15
        }
    }));
    write_mock_model_config(
        &root,
        &base_url,
        r#"
openai_compatible:
  enabled: true
  llm:
    enabled: true
    base_url: "${MOCK_BASE_URL}"
    api_key: "sk-llm-local"
    model: "llm-small"
    temperature: 0.1
    max_tokens: 64
    timeout_ms: 5000
    request_overrides:
      stream: true
      enable_thinking: false
"#,
    );
    initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
    preload_model_config().expect("model config should preload");

    let response =
        model_llm("system prompt", "user prompt", None).expect("LLM call should succeed");
    let request = handle.join().expect("mock provider should finish");

    assert_eq!(response.assistant, "mock assistant");
    assert_eq!(
        response.usage,
        Some(ModelUsage {
            input_tokens: Some(11),
            output_tokens: Some(4),
            total_tokens: Some(15),
        })
    );
    assert_eq!(request.path, "/v1/chat/completions");
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer sk-llm-local")
    );
    assert_eq!(request.body["model"], "llm-small");
    assert_eq!(request.body["stream"], false);
    assert_eq!(request.body["messages"][0]["role"], "system");
    assert_eq!(request.body["messages"][0]["content"], "system prompt");
    assert_eq!(request.body["messages"][1]["role"], "user");
    assert_eq!(request.body["messages"][1]["content"], "user prompt");
    assert_eq!(request.body["enable_thinking"], false);

    restore_default_model_callbacks();
    let _ = std::fs::remove_dir_all(root);
}

/// LuaSkills model callbacks should be registered only when the host model config enables the capability.
/// 只有宿主模型配置启用对应能力时，才应注册 LuaSkills 模型回调。
#[test]
fn install_callbacks_registers_enabled_capabilities_for_lua_status() {
    let _guard = callback_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = unique_test_dir("enabled-callbacks");
    let config_path = root.join("configs").join("model_config.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("config dir should be created");
    std::fs::write(
        &config_path,
        r#"
openai_compatible:
  enabled: true
  embedding:
    enabled: true
    base_url: "http://127.0.0.1:9/v1"
    api_key: "sk-test"
    model: "embed-small"
  llm:
    enabled: false
    model: ""
"#,
    )
    .expect("model config should be written");

    initialize_model_config_runtime_root(Some(&root)).expect("runtime root should set");
    preload_model_config().expect("model config should preload");
    install_luaskills_model_callbacks();

    let engine = make_lua_engine();
    let result = engine
        .run_lua(
            r#"
local status = vulcan.models.status()
return {
  status_ok = status.ok,
  embed = status.capabilities.embed,
  llm = status.capabilities.llm,
  has_embed = vulcan.models.has("embed"),
  has_llm = vulcan.models.has("llm"),
}
"#,
            &json!({}),
            None,
        )
        .expect("Lua model status should run");

    assert_eq!(result["status_ok"], true);
    assert_eq!(result["embed"], true);
    assert_eq!(result["llm"], false);
    assert_eq!(result["has_embed"], true);
    assert_eq!(result["has_llm"], false);

    restore_default_model_callbacks();
    let _ = std::fs::remove_dir_all(root);
}
