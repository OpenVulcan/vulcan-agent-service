use serde_json::{Value, json};

use crate::host_core::HostRuntime;
use crate::host_core::model::RuntimeToolCallRequest;
use crate::transport::mcp::mapping::{
    mcp_tool_call_result_value_from_runtime, mcp_tools_list_value_from_runtime,
    runtime_context_from_mcp,
};
use crate::transport::mcp::protocol::{RequestContext, ToolCallRequest};
use crate::transport::mcp::views;

/// MCP JSON-RPC adapter that owns the protocol-facing dispatch entrypoints for transports.
/// 拥有面向传输层协议分发入口的 MCP JSON-RPC 适配器。
#[derive(Clone)]
pub struct McpDispatcher {
    /// Host runtime used by the dispatcher while MCP handlers are migrated out incrementally.
    /// 在 MCP handler 逐步迁出期间由 dispatcher 使用的宿主运行时。
    runtime: HostRuntime,
}

impl McpDispatcher {
    /// Build a new MCP dispatcher around one host runtime instance.
    /// 围绕单个宿主运行时实例构建新的 MCP dispatcher。
    pub fn new(runtime: HostRuntime) -> Self {
        Self { runtime }
    }

    /// Handle one JSON-RPC message without a pre-established request context.
    /// 处理一条尚未建立请求上下文的 JSON-RPC 消息。
    pub async fn handle_message(&self, message: &Value) -> Option<Value> {
        self.handle_message_with_context(message, RequestContext::default())
            .await
    }

    /// Handle one JSON-RPC message with a transport-provided request context.
    /// 使用传输层提供的请求上下文处理一条 JSON-RPC 消息。
    pub async fn handle_message_with_context(
        &self,
        message: &Value,
        request_context: RequestContext,
    ) -> Option<Value> {
        // Batch requests reuse the same transport-level context for every contained JSON-RPC item.
        // 批量请求会为其中每个 JSON-RPC 项复用同一份传输层上下文。
        if let Some(batch) = message.as_array() {
            let mut responses = Vec::new();
            for item in batch {
                if let Some(response) = self.handle_single(item, request_context.clone()).await {
                    responses.push(response);
                }
            }
            if !responses.is_empty() {
                return Some(Value::Array(responses));
            }
            return None;
        }

        self.handle_single(message, request_context).await
    }

    /// Build a protocol-level parse error response for malformed JSON-RPC input.
    /// 为格式错误的 JSON-RPC 输入构建协议级解析错误响应。
    #[allow(dead_code)]
    pub fn parse_error(message: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": message
            }
        })
    }

    /// Handle one non-batch JSON-RPC request, notification, or invalid object.
    /// 处理单条非批量 JSON-RPC request、notification 或无效对象。
    async fn handle_single(
        &self,
        message: &Value,
        request_context: RequestContext,
    ) -> Option<Value> {
        if let Some(id) = message.get("id").cloned() {
            let method = message
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let params = message.get("params").cloned();
            let result = self.handle_request(method, params, request_context).await;
            match result {
                Ok(value) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": value
                })),
                Err((code, message)) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": code,
                        "message": message
                    }
                })),
            }
        } else if let Some(method) = message.get("method").and_then(|value| value.as_str()) {
            let params = message.get("params").cloned();
            self.handle_notification(method, params, request_context)
                .await;
            None
        } else {
            None
        }
    }

    /// Route one JSON-RPC request method into the host runtime capability surface.
    /// 将单个 JSON-RPC request 方法路由到宿主运行时能力面。
    async fn handle_request(
        &self,
        method: &str,
        params: Option<Value>,
        request_context: RequestContext,
    ) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => self.handle_initialize(params),
            "ping" => Ok(json!({})),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(params, &request_context).await,
            _ => {
                eprintln!("[MCP] Unknown method: {}", method);
                Err((-32601, format!("Method not found: {}", method)))
            }
        }
    }

    /// Route one JSON-RPC notification method into transport-visible side effects.
    /// 将单个 JSON-RPC notification 方法路由到传输可见副作用。
    async fn handle_notification(
        &self,
        method: &str,
        _params: Option<Value>,
        _request_context: RequestContext,
    ) {
        match method {
            "notifications/initialized" => {
                views::mark_initialized(&self.runtime).await;
                eprintln!("[MCP] Client initialized");
            }
            _ => {
                eprintln!("[MCP] Unknown notification: {}", method);
            }
        }
    }

    /// Handle initialize through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 initialize。
    fn handle_initialize(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
        views::initialize_value(&self.runtime, params)
    }

    /// Handle tools/list through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 tools/list。
    fn handle_tools_list(&self) -> Result<Value, (i64, String)> {
        mcp_tools_list_value_from_runtime(self.runtime.list_runtime_tools()?)
    }

    /// Handle tools/call through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 tools/call。
    async fn handle_tools_call(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        let request: ToolCallRequest = serde_json::from_value(params.unwrap_or_default())
            .map_err(|error| (-32602, format!("Invalid tools/call params: {}", error)))?;
        let runtime_request = RuntimeToolCallRequest {
            name: request.name,
            arguments: request.arguments,
        };
        let runtime_context = runtime_context_from_mcp(request_context);
        let result = self
            .runtime
            .call_runtime_tool(runtime_request, &runtime_context)
            .await?;
        mcp_tool_call_result_value_from_runtime(result)
    }
}
