use serde_json::{Value, json};

use crate::host_core::McpServer;
use crate::transport::mcp::protocol::{CancellationNotification, RequestContext};

/// MCP JSON-RPC adapter that owns the protocol-facing dispatch entrypoints for transports.
/// 拥有面向传输层协议分发入口的 MCP JSON-RPC 适配器。
#[derive(Clone)]
pub struct McpDispatcher {
    /// Host runtime used by the dispatcher while MCP handlers are migrated out incrementally.
    /// 在 MCP handler 逐步迁出期间由 dispatcher 使用的宿主运行时。
    runtime: McpServer,
}

impl McpDispatcher {
    /// Build a new MCP dispatcher around one host runtime instance.
    /// 围绕单个宿主运行时实例构建新的 MCP dispatcher。
    pub fn new(runtime: McpServer) -> Self {
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
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resources_read(params, &request_context),
            "resources/templates/list" => self.handle_resource_templates_list(),
            "prompts/list" => self.handle_prompts_list(),
            "prompts/get" => self.handle_prompts_get(params, &request_context),
            "completion/complete" => self.handle_completion(params),
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
        params: Option<Value>,
        _request_context: RequestContext,
    ) {
        match method {
            "notifications/initialized" => {
                self.runtime.mark_mcp_initialized().await;
                eprintln!("[MCP] Client initialized");
            }
            "notifications/cancelled" => {
                if let Some(params) = params {
                    let cancel: Result<CancellationNotification, _> =
                        serde_json::from_value(params);
                    if let Ok(cancel) = cancel {
                        eprintln!(
                            "[MCP] Request cancelled: {:?}, reason: {:?}",
                            cancel.request_id, cancel.reason
                        );
                    }
                }
            }
            "notifications/roots/list_changed" => {
                eprintln!("[MCP] Roots list changed notification received");
            }
            _ => {
                eprintln!("[MCP] Unknown notification: {}", method);
            }
        }
    }

    /// Handle initialize through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 initialize。
    fn handle_initialize(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
        self.runtime.initialize_mcp_client_value(params)
    }

    /// Handle tools/list through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 tools/list。
    fn handle_tools_list(&self) -> Result<Value, (i64, String)> {
        self.runtime.list_mcp_tools_value()
    }

    /// Handle tools/call through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 tools/call。
    async fn handle_tools_call(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        self.runtime
            .call_mcp_tool_value(params, request_context)
            .await
    }

    /// Handle resources/list through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 resources/list。
    fn handle_resources_list(&self) -> Result<Value, (i64, String)> {
        self.runtime.list_mcp_resources_value()
    }

    /// Handle resources/read through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 resources/read。
    fn handle_resources_read(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        self.runtime
            .read_mcp_resource_value(params, request_context)
    }

    /// Handle resources/templates/list through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 resources/templates/list。
    fn handle_resource_templates_list(&self) -> Result<Value, (i64, String)> {
        self.runtime.list_mcp_resource_templates_value()
    }

    /// Handle prompts/list through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 prompts/list。
    fn handle_prompts_list(&self) -> Result<Value, (i64, String)> {
        self.runtime.list_mcp_prompts_value()
    }

    /// Handle prompts/get through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 prompts/get。
    fn handle_prompts_get(
        &self,
        params: Option<Value>,
        request_context: &RequestContext,
    ) -> Result<Value, (i64, String)> {
        self.runtime.get_mcp_prompt_value(params, request_context)
    }

    /// Handle completion/complete through the MCP adapter boundary.
    /// 通过 MCP 适配边界处理 completion/complete。
    fn handle_completion(&self, params: Option<Value>) -> Result<Value, (i64, String)> {
        self.runtime.complete_mcp_argument_value(params)
    }
}
