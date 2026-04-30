use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

use crate::protocol::{
    InitializeRequest, PROTOCOL_VERSION_LATEST, RequestContext, negotiate_version,
};
use crate::server::McpServer;

/// Stateful stdio session metadata for one MCP client attached over stdin/stdout.
/// 为通过标准输入输出连接的单个 MCP 客户端保存状态化 stdio 会话元数据。
#[derive(Clone, Default)]
struct StdioSessionState {
    /// Persisted request-scoped client context after initialize succeeds.
    /// initialize 成功后持久保存的请求级客户端上下文。
    request_context: Option<RequestContext>,
}

/// Run the MCP server over stdio using newline-delimited JSON-RPC messages.
/// 使用换行分隔的 JSON-RPC 消息通过 stdio 运行 MCP 服务。
pub async fn run_stdio(server: McpServer) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[MCP] Starting stdio transport on stdin/stdout ...");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);
    let mut session_state = StdioSessionState::default();

    loop {
        let Some(message) = read_stdio_message(&mut reader).await? else {
            break;
        };

        let response = handle_stdio_message(&server, &message, &mut session_state).await?;
        if let Some(response_message) = response {
            write_stdio_message(&mut writer, &response_message).await?;
        }
    }

    writer.flush().await?;
    Ok(())
}

/// Handle one stdio-delivered JSON-RPC message and update session state when initialize succeeds.
/// 处理一条通过 stdio 送达的 JSON-RPC 消息，并在 initialize 成功后更新会话状态。
async fn handle_stdio_message(
    server: &McpServer,
    message: &Value,
    session_state: &mut StdioSessionState,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    if is_initialize_request(message) {
        let Some(response) = server.handle_message(message).await else {
            return Err("initialize did not produce a JSON-RPC response.".into());
        };
        let Some(request_context) = build_initialize_request_context(message, &response)? else {
            return Ok(Some(response));
        };
        session_state.request_context = Some(request_context);
        return Ok(Some(response));
    }

    let Some(request_context) = session_state.request_context.clone() else {
        return Ok(build_stdio_preinitialize_response(message));
    };

    Ok(server
        .handle_message_with_context(message, request_context)
        .await)
}

/// Build the persisted request context for a stdio session from initialize request/response values.
/// 根据 initialize 的请求与响应值构建 stdio 会话持久化请求上下文。
fn build_initialize_request_context(
    message: &Value,
    response: &Value,
) -> Result<Option<RequestContext>, Box<dyn std::error::Error>> {
    if response.get("error").is_some() {
        return Ok(None);
    }

    let Some(protocol_version) = negotiated_protocol_from_initialize(response) else {
        return Err("initialize response did not include result.protocolVersion.".into());
    };

    let initialize_request: InitializeRequest = serde_json::from_value(
        message.get("params").cloned().unwrap_or_default(),
    )
    .map_err(|error| {
        format!(
            "initialize params could not be reconstructed after success: {}",
            error
        )
    })?;

    Ok(Some(RequestContext {
        transport: Some("stdio".to_string()),
        session_id: Some("stdio".to_string()),
        protocol_version: Some(protocol_version),
        client_info: initialize_request.client_info,
        client_match_name_override: None,
        exact_client_name: None,
        disable_client_match_overrides: false,
        client_capabilities: initialize_request.capabilities,
    }))
}

/// Return whether the JSON-RPC payload is an initialize request carrying an id.
/// 判断该 JSON-RPC 负载是否为携带 id 的 initialize 请求。
fn is_initialize_request(message: &Value) -> bool {
    message
        .get("method")
        .and_then(Value::as_str)
        .map(|method| method == "initialize")
        .unwrap_or(false)
        && message.get("id").is_some()
}

/// Build an early error response when the client sends requests before initialize over stdio.
/// 当客户端在 stdio 上 initialize 之前发送请求时，构造提前失败的错误响应。
fn build_stdio_preinitialize_response(message: &Value) -> Option<Value> {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").cloned();

    match (method, id) {
        (Some("initialize"), None) => Some(json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "error": {
                "code": -32600,
                "message": "initialize must be sent as a request with an id."
            }
        })),
        (Some(_), Some(id_value)) => Some(json!({
            "jsonrpc": "2.0",
            "id": id_value,
            "error": {
                "code": -32002,
                "message": "Missing MCP session state. Call initialize first."
            }
        })),
        _ => None,
    }
}

/// Extract the negotiated protocol version from a successful initialize response.
/// 从成功的 initialize 响应中提取协商后的协议版本。
fn negotiated_protocol_from_initialize(response: &Value) -> Option<String> {
    let protocol_version = response
        .get("result")
        .and_then(|result| result.get("protocolVersion"))
        .and_then(Value::as_str)?;

    if protocol_version == PROTOCOL_VERSION_LATEST || negotiate_version(protocol_version).is_some()
    {
        return Some(protocol_version.to_string());
    }

    None
}

/// Read one newline-delimited JSON-RPC message from stdio.
/// 从 stdio 中读取一条按换行分隔的 JSON-RPC 消息。
async fn read_stdio_message<R>(
    reader: &mut BufReader<R>,
) -> Result<Option<Value>, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin,
{
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        let message = serde_json::from_str::<Value>(trimmed)
            .map_err(|error| format!("Invalid stdio JSON-RPC line: {} ({})", trimmed, error))?;
        return Ok(Some(message));
    }
}

/// Write one newline-delimited JSON-RPC message to stdio.
/// 向 stdio 写出一条按换行分隔的 JSON-RPC 消息。
async fn write_stdio_message<W>(
    writer: &mut BufWriter<W>,
    message: &Value,
) -> Result<(), Box<dyn std::error::Error>>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_string(message)?;
    writer.write_all(body.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
