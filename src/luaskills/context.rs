use crate::config::client_budget::{
    ClientBudgetSnapshot, resolve_client_budget_snapshot, resolve_effective_client_match_name,
    resolve_grpc_client_budget_snapshot,
};
use crate::transport::mcp::protocol::RequestContext;
use luaskills::{LuaInvocationContext, RuntimeClientInfo, RuntimeRequestContext};
use serde_json::json;

/// Convert one MCP request context into the generic runtime request context expected by the LuaSkills library.
/// 把一份 MCP 请求上下文转换为 LuaSkills 库期望的通用运行时请求上下文。
pub fn build_runtime_request_context(request_context: &RequestContext) -> RuntimeRequestContext {
    let effective_client_name = resolve_effective_client_match_name(Some(request_context));
    let effective_client_version = request_context
        .client_info
        .as_ref()
        .map(|client_info| client_info.version.clone());
    let runtime_client_info =
        if effective_client_name.is_some() || effective_client_version.is_some() {
            Some(RuntimeClientInfo {
                kind: Some("mcp".to_string()),
                name: effective_client_name.clone(),
                version: effective_client_version,
            })
        } else {
            None
        };

    RuntimeRequestContext {
        request_id: None,
        client_name: effective_client_name,
        transport_name: request_context.transport.clone(),
        session_id: request_context.session_id.clone(),
        client_info: runtime_client_info,
        client_capabilities: request_context.client_capabilities.clone(),
    }
}

/// Build one host-injected runtime invocation context from MCP request context, client budgets, and tool config.
/// 基于 MCP 请求上下文、客户端预算与工具配置构造一份宿主注入式运行时调用上下文。
pub fn build_runtime_invocation_context(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> LuaInvocationContext {
    let client_budget = resolve_client_budget_snapshot(request_context, tool_name, skill_name);
    let runtime_request_context = request_context.map(build_runtime_request_context);
    LuaInvocationContext::new(
        runtime_request_context,
        serde_json::to_value(&client_budget).unwrap_or_else(|_| json!({})),
        client_budget.tool_config.clone(),
    )
}

/// Build one runtime request context from a trusted gRPC client identity.
/// 基于受信任的 gRPC 客户端身份构造运行时请求上下文。
pub fn build_grpc_runtime_request_context(
    client_name: &str,
    client_version: Option<&str>,
    request_id: Option<&str>,
) -> RuntimeRequestContext {
    let normalized_client_name = client_name.trim().to_string();
    let normalized_client_version = client_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let normalized_request_id = request_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let runtime_client_info =
        if normalized_client_name.is_empty() && normalized_client_version.is_none() {
            None
        } else {
            Some(RuntimeClientInfo {
                kind: Some("grpc".to_string()),
                name: if normalized_client_name.is_empty() {
                    None
                } else {
                    Some(normalized_client_name.clone())
                },
                version: normalized_client_version,
            })
        };

    RuntimeRequestContext {
        request_id: normalized_request_id,
        client_name: if normalized_client_name.is_empty() {
            None
        } else {
            Some(normalized_client_name.clone())
        },
        transport_name: Some("grpc_unary".to_string()),
        session_id: None,
        client_info: runtime_client_info,
        client_capabilities: json!({}),
    }
}

/// Build one Lua invocation context for gRPC without generic MCP client matching.
/// 为 gRPC 构造 Lua 调用上下文，不使用通用 MCP 客户端匹配逻辑。
pub fn build_grpc_runtime_invocation_context(
    client_name: &str,
    client_version: Option<&str>,
    request_id: Option<&str>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> LuaInvocationContext {
    let client_budget = resolve_grpc_client_budget_snapshot(client_name, tool_name, skill_name);
    let runtime_request_context = Some(build_grpc_runtime_request_context(
        client_name,
        client_version,
        request_id,
    ));
    LuaInvocationContext::new(
        runtime_request_context,
        serde_json::to_value(&client_budget).unwrap_or_else(|_| json!({})),
        client_budget.tool_config.clone(),
    )
}

/// Convert the host-side client budget snapshot into the exact spill-render input still used by the MCP host.
/// 把宿主侧客户端预算快照转换为 MCP 宿主当前仍在使用的溢出渲染输入。
pub fn client_budget_snapshot_for_render(
    request_context: Option<&RequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    resolve_client_budget_snapshot(request_context, tool_name, skill_name)
}

/// Resolve the render budget for a gRPC tool call by exact `client_name`.
/// 通过精确 `client_name` 解析 gRPC 工具调用的渲染预算。
pub fn grpc_client_budget_snapshot_for_render(
    client_name: &str,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> ClientBudgetSnapshot {
    resolve_grpc_client_budget_snapshot(client_name, tool_name, skill_name)
}
