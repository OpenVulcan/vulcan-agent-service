use serde_json::{Value, json};

use crate::host_core::HostRuntime;
use crate::host_core::model::RuntimeToolCallRequest;
use crate::transport::mcp::mapping::{
    mcp_tool_call_result_value_from_runtime, mcp_tools_list_value_from_runtime,
    runtime_context_from_mcp,
};
use crate::transport::mcp::protocol::{RequestContext, ToolCallRequest, parse_required_params};
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
        if is_jsonrpc_response_message(message) {
            return None;
        }

        if let Some(id) = message.get("id").cloned() {
            // Reject malformed request objects before method routing so protocol errors are not reported as empty method names.
            // 在方法路由前拒绝畸形 request 对象，避免把协议错误报告为空方法名。
            let Some(method) = message.get("method").and_then(|value| value.as_str()) else {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32600,
                        "message": "Invalid JSON-RPC request: method must be a string."
                    }
                }));
            };
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
        let request: ToolCallRequest = parse_required_params("tools/call", params)
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

/// Return whether one JSON-RPC value is a client-originated response that must not be dispatched as a request.
/// 判断一个 JSON-RPC 值是否为客户端回传且不应按 request 分发的 response。
///
/// Parameters: `message` is the JSON value received from the transport boundary.
/// 参数：`message` 是传输边界收到的 JSON 值。
///
/// Returns: `true` when the value has an id plus result/error and no method.
/// 返回：当该值包含 id 与 result/error 且不包含 method 时返回 `true`。
fn is_jsonrpc_response_message(message: &Value) -> bool {
    message.get("method").is_none()
        && message.get("id").is_some()
        && (message.get("result").is_some() || message.get("error").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dispatcher should ignore client-originated JSON-RPC success responses instead of routing them as empty-method requests.
    /// Dispatcher 应忽略客户端回传的 JSON-RPC 成功响应，而不是将其路由为空方法 request。
    #[tokio::test]
    async fn dispatcher_ignores_jsonrpc_success_response_messages() {
        // Build a dispatcher with an empty host runtime because response messages must not touch runtime capabilities.
        // 使用空宿主运行时构造 dispatcher，因为 response 消息不应触达运行时能力。
        let dispatcher = McpDispatcher::new(HostRuntime::new());
        // Build a valid JSON-RPC success response as a client would send back to a server-originated request.
        // 构造客户端针对服务端请求回传的合法 JSON-RPC 成功响应。
        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "ok": true
                }
            }))
            .await;

        assert!(response.is_none());
    }

    /// Dispatcher should ignore client-originated JSON-RPC error responses instead of producing Method not found errors.
    /// Dispatcher 应忽略客户端回传的 JSON-RPC 错误响应，而不是生成 Method not found 错误。
    #[tokio::test]
    async fn dispatcher_ignores_jsonrpc_error_response_messages() {
        // Build a dispatcher with an empty host runtime because response messages must terminate at the protocol adapter.
        // 使用空宿主运行时构造 dispatcher，因为 response 消息应终止在协议适配层。
        let dispatcher = McpDispatcher::new(HostRuntime::new());
        // Build a valid JSON-RPC error response that carries id and error without a method.
        // 构造包含 id 与 error 且不包含 method 的合法 JSON-RPC 错误响应。
        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "error": {
                    "code": -32000,
                    "message": "client failure"
                }
            }))
            .await;

        assert!(response.is_none());
    }

    /// Dispatcher should omit JSON-RPC response items from batches while preserving real request responses.
    /// Dispatcher 应从 batch 中省略 JSON-RPC response 项，同时保留真实 request 的响应。
    #[tokio::test]
    async fn dispatcher_omits_jsonrpc_response_items_from_batches() {
        // Build a dispatcher with an empty host runtime because ping handling does not require configured tools.
        // 使用空宿主运行时构造 dispatcher，因为 ping 处理不需要已配置工具。
        let dispatcher = McpDispatcher::new(HostRuntime::new());
        // Build one mixed batch containing a client response and one ordinary ping request.
        // 构造同时包含客户端 response 与普通 ping request 的混合 batch。
        let response = dispatcher
            .handle_message(&serde_json::json!([
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "ok": true
                    }
                },
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "ping"
                }
            ]))
            .await
            .expect("batch with one request should produce one response");
        // Read the batch response array so the assertion verifies filtering rather than just presence.
        // 读取 batch 响应数组，确保断言验证的是过滤结果而不只是响应存在。
        let response_items = response
            .as_array()
            .expect("batch response should be an array");

        assert_eq!(response_items.len(), 1);
        assert_eq!(response_items[0]["id"], serde_json::json!(2));
        assert!(response_items[0].get("result").is_some());
    }

    /// Dispatcher should reject id-bearing request objects without method as invalid JSON-RPC requests.
    /// Dispatcher 应将缺少 method 的带 id request 对象拒绝为无效 JSON-RPC 请求。
    #[tokio::test]
    async fn dispatcher_rejects_request_without_method_as_invalid_request() {
        // Build a dispatcher with an empty host runtime because malformed requests must fail before runtime routing.
        // 使用空宿主运行时构造 dispatcher，因为畸形 request 必须在运行时路由前失败。
        let dispatcher = McpDispatcher::new(HostRuntime::new());
        // Build a malformed id-bearing request that is neither a response nor a routable request.
        // 构造一个既不是 response 也无法路由的畸形带 id request。
        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "params": {}
            }))
            .await
            .expect("invalid request should produce an error response");

        assert_eq!(response["id"], serde_json::json!(3));
        assert_eq!(response["error"]["code"], serde_json::json!(-32600));
        assert_eq!(
            response["error"]["message"],
            serde_json::json!("Invalid JSON-RPC request: method must be a string.")
        );
    }

    /// Dispatcher should reject non-string methods before unknown-method routing begins.
    /// Dispatcher 应在进入未知方法路由前拒绝非字符串 method。
    #[tokio::test]
    async fn dispatcher_rejects_non_string_method_as_invalid_request() {
        // Build a dispatcher with an empty host runtime because method type validation is protocol-layer behavior.
        // 使用空宿主运行时构造 dispatcher，因为 method 类型校验属于协议层行为。
        let dispatcher = McpDispatcher::new(HostRuntime::new());
        // Build a malformed request whose method exists but is not a JSON string.
        // 构造一个 method 存在但不是 JSON 字符串的畸形 request。
        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": 42
            }))
            .await
            .expect("invalid method type should produce an error response");

        assert_eq!(response["id"], serde_json::json!(4));
        assert_eq!(response["error"]["code"], serde_json::json!(-32600));
        assert_eq!(
            response["error"]["message"],
            serde_json::json!("Invalid JSON-RPC request: method must be a string.")
        );
    }

    /// Dispatcher should reject initialize requests that omit required params with an explicit invalid-params error.
    /// Dispatcher 应以明确 invalid-params 错误拒绝省略必填 params 的 initialize request。
    #[tokio::test]
    async fn dispatcher_rejects_initialize_without_params_as_invalid_params() {
        // Build a dispatcher with an empty host runtime because missing params must fail before runtime session mutation.
        // 使用空宿主运行时构造 dispatcher，因为缺失 params 必须在运行时会话变更前失败。
        let dispatcher = McpDispatcher::new(HostRuntime::new());

        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "initialize"
            }))
            .await
            .expect("invalid initialize params should produce an error response");

        assert_eq!(response["id"], serde_json::json!(5));
        assert_eq!(response["error"]["code"], serde_json::json!(-32602));
        assert_eq!(
            response["error"]["message"],
            serde_json::json!("Invalid initialize params: initialize params are required.")
        );
    }

    /// Dispatcher should reject tools/call requests that omit the required params object.
    /// Dispatcher 应拒绝省略必填 params 对象的 tools/call request。
    #[tokio::test]
    async fn dispatcher_rejects_tools_call_without_params_as_invalid_params() {
        // Build a dispatcher with an empty host runtime because missing params must fail before tool resolution.
        // 使用空宿主运行时构造 dispatcher，因为缺失 params 必须在工具解析前失败。
        let dispatcher = McpDispatcher::new(HostRuntime::new());

        let response = dispatcher
            .handle_message(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call"
            }))
            .await
            .expect("invalid tools/call params should produce an error response");

        assert_eq!(response["id"], serde_json::json!(6));
        assert_eq!(response["error"]["code"], serde_json::json!(-32602));
        assert_eq!(
            response["error"]["message"],
            serde_json::json!("Invalid tools/call params: tools/call params are required.")
        );
    }
}
