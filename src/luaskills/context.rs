use crate::config::client_budget::{
    ClientBudgetSnapshot, EffectiveBudgetScope, resolve_client_budget_snapshot,
    resolve_effective_client_match_name, resolve_grpc_client_budget_snapshot,
};
use crate::support::RuntimeRequestContext as HostRuntimeRequestContext;
use luaskills::{
    LuaInvocationContext, RuntimeClientInfo, RuntimeRequestContext as LuaRuntimeRequestContext,
};
use serde_json::{Map, Number, Value, json};

/// Convert one host request context into the generic runtime request context expected by the LuaSkills library.
/// 把一份宿主请求上下文转换为 LuaSkills 库期望的通用运行时请求上下文。
pub fn build_runtime_request_context(
    request_context: &HostRuntimeRequestContext,
) -> LuaRuntimeRequestContext {
    let effective_client_name = resolve_effective_client_match_name(Some(request_context));
    let effective_client_version = request_context
        .client_info
        .as_ref()
        .map(|client_info| client_info.version.clone());
    let runtime_client_info =
        if effective_client_name.is_some() || effective_client_version.is_some() {
            Some(RuntimeClientInfo {
                kind: request_context
                    .transport
                    .clone()
                    .or_else(|| Some("host".to_string())),
                name: effective_client_name.clone(),
                version: effective_client_version,
            })
        } else {
            None
        };

    LuaRuntimeRequestContext {
        request_id: None,
        client_name: effective_client_name,
        transport_name: request_context.transport.clone(),
        session_id: request_context.session_id.clone(),
        client_info: runtime_client_info,
        client_capabilities: request_context.client_capabilities.clone(),
    }
}

/// Build one host-injected runtime invocation context from host request context, client budgets, and tool config.
/// 基于宿主请求上下文、客户端预算与工具配置构造一份宿主注入式运行时调用上下文。
/// Parameters: `request_context` is the optional host request context for client matching.
/// 参数：`request_context` 是用于客户端匹配的可选宿主请求上下文。
/// Parameters: `tool_name` is the optional runtime tool name being invoked.
/// 参数：`tool_name` 是当前调用的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the Lua invocation context or a cached configuration error.
/// 返回 Lua 调用上下文或缓存的配置错误。
pub fn build_runtime_invocation_context(
    request_context: Option<&HostRuntimeRequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<LuaInvocationContext, String> {
    let client_budget = resolve_client_budget_snapshot(request_context, tool_name, skill_name)?;
    let runtime_request_context = request_context.map(build_runtime_request_context);
    Ok(LuaInvocationContext::new(
        runtime_request_context,
        client_budget_snapshot_value(&client_budget),
        client_budget.tool_config.clone(),
    ))
}

/// Build one runtime request context from a trusted gRPC client identity.
/// 基于受信任的 gRPC 客户端身份构造运行时请求上下文。
pub fn build_grpc_runtime_request_context(
    client_name: &str,
    client_version: Option<&str>,
    request_id: Option<&str>,
) -> LuaRuntimeRequestContext {
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

    LuaRuntimeRequestContext {
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
/// Parameters: `client_name` is the trusted gRPC client identity.
/// 参数：`client_name` 是受信任的 gRPC 客户端身份。
/// Parameters: `client_version` is the optional trusted gRPC client version.
/// 参数：`client_version` 是可选的受信任 gRPC 客户端版本。
/// Parameters: `request_id` is the optional request identifier forwarded to LuaSkills.
/// 参数：`request_id` 是转发给 LuaSkills 的可选请求标识。
/// Parameters: `tool_name` is the optional runtime tool name being invoked.
/// 参数：`tool_name` 是当前调用的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the Lua invocation context or a cached configuration error.
/// 返回 Lua 调用上下文或缓存的配置错误。
pub fn build_grpc_runtime_invocation_context(
    client_name: &str,
    client_version: Option<&str>,
    request_id: Option<&str>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<LuaInvocationContext, String> {
    let client_budget = resolve_grpc_client_budget_snapshot(client_name, tool_name, skill_name)?;
    let runtime_request_context = Some(build_grpc_runtime_request_context(
        client_name,
        client_version,
        request_id,
    ));
    Ok(LuaInvocationContext::new(
        runtime_request_context,
        client_budget_snapshot_value(&client_budget),
        client_budget.tool_config.clone(),
    ))
}

/// Convert one resolved client-budget snapshot into the LuaSkills context JSON object.
/// 将一份已解析的客户端预算快照转换为 LuaSkills 上下文 JSON 对象。
fn client_budget_snapshot_value(client_budget: &ClientBudgetSnapshot) -> Value {
    // Build the injected object from the snapshot's concrete fields so failures cannot be hidden as an empty budget object.
    // 基于快照的确定字段构造注入对象，避免把失败隐藏成空预算对象。
    let mut budget = Map::new();
    budget.insert(
        "client_name".to_string(),
        optional_string_value(&client_budget.client_name),
    );
    budget.insert(
        "tool_name".to_string(),
        optional_string_value(&client_budget.tool_name),
    );
    budget.insert(
        "skill_name".to_string(),
        optional_string_value(&client_budget.skill_name),
    );
    budget.insert(
        "matched_client_pattern".to_string(),
        optional_string_value(&client_budget.matched_client_pattern),
    );
    budget.insert(
        "tool_result".to_string(),
        budget_scope_value(&client_budget.tool_result),
    );
    budget.insert(
        "file_read".to_string(),
        budget_scope_value(&client_budget.file_read),
    );
    budget.insert("tool_config".to_string(), client_budget.tool_config.clone());
    Value::Object(budget)
}

/// Convert one optional string snapshot field into its JSON representation.
/// 将一个可选字符串快照字段转换为对应 JSON 表示。
fn optional_string_value(value: &Option<String>) -> Value {
    match value {
        Some(value) => Value::String(value.clone()),
        None => Value::Null,
    }
}

/// Convert one effective budget scope into its JSON representation.
/// 将一个最终预算场景转换为对应 JSON 表示。
fn budget_scope_value(scope: &EffectiveBudgetScope) -> Value {
    let mut rendered = Map::new();
    rendered.insert(
        "bytes".to_string(),
        Value::Number(Number::from(scope.bytes)),
    );
    rendered.insert(
        "lines".to_string(),
        Value::Number(Number::from(scope.lines)),
    );
    Value::Object(rendered)
}

/// Convert the host-side client budget snapshot into the exact spill-render input used by runtime rendering.
/// 把宿主侧客户端预算快照转换为运行时渲染使用的精确溢出渲染输入。
/// Parameters: `request_context` is the optional host request context for client matching.
/// 参数：`request_context` 是用于客户端匹配的可选宿主请求上下文。
/// Parameters: `tool_name` is the optional runtime tool name being rendered.
/// 参数：`tool_name` 是当前渲染的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the resolved budget snapshot or a cached configuration error.
/// 返回解析后的预算快照或缓存的配置错误。
pub fn client_budget_snapshot_for_render(
    request_context: Option<&HostRuntimeRequestContext>,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<ClientBudgetSnapshot, String> {
    resolve_client_budget_snapshot(request_context, tool_name, skill_name)
}

/// Resolve the render budget for a gRPC tool call by exact `client_name`.
/// 通过精确 `client_name` 解析 gRPC 工具调用的渲染预算。
/// Parameters: `client_name` is the trusted gRPC client identity.
/// 参数：`client_name` 是受信任的 gRPC 客户端身份。
/// Parameters: `tool_name` is the optional runtime tool name being rendered.
/// 参数：`tool_name` 是当前渲染的可选运行时工具名称。
/// Parameters: `skill_name` is the optional owning skill name for tool-config lookup.
/// 参数：`skill_name` 是用于工具配置查找的可选所属 skill 名称。
/// Returns the resolved budget snapshot or a cached configuration error.
/// 返回解析后的预算快照或缓存的配置错误。
pub fn grpc_client_budget_snapshot_for_render(
    client_name: &str,
    tool_name: Option<&str>,
    skill_name: Option<&str>,
) -> Result<ClientBudgetSnapshot, String> {
    resolve_grpc_client_budget_snapshot(client_name, tool_name, skill_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build runtime invocation context with a structured client-budget object for MCP calls.
    /// 验证 MCP 调用构造的运行时调用上下文包含结构化客户端预算对象。
    #[test]
    fn build_runtime_invocation_context_injects_structured_client_budget() {
        // Define one MCP request context with an exact client identity.
        // 定义一个带精确客户端身份的 MCP 请求上下文。
        let request_context = HostRuntimeRequestContext {
            exact_client_name: Some("codex".to_string()),
            transport: Some("stdio".to_string()),
            ..HostRuntimeRequestContext::default()
        };

        // Build the Lua invocation context through the same entrypoint used by dynamic tools.
        // 通过动态工具使用的同一入口构造 Lua 调用上下文。
        let invocation_context =
            build_runtime_invocation_context(Some(&request_context), Some("tool"), Some("skill"))
                .expect("runtime invocation context should build");

        assert_eq!(
            invocation_context.client_budget["client_name"].as_str(),
            Some("codex")
        );
        assert_eq!(
            invocation_context.client_budget["tool_name"].as_str(),
            Some("tool")
        );
        assert_eq!(
            invocation_context.client_budget["skill_name"].as_str(),
            Some("skill")
        );
        assert!(
            invocation_context.client_budget["tool_result"]["bytes"]
                .as_u64()
                .is_some()
        );
        assert!(
            invocation_context.client_budget["file_read"]["lines"]
                .as_i64()
                .is_some()
        );
        assert!(invocation_context.client_budget["tool_config"].is_object());
    }

    /// Build runtime invocation context with a structured client-budget object for gRPC calls.
    /// 验证 gRPC 调用构造的运行时调用上下文包含结构化客户端预算对象。
    #[test]
    fn build_grpc_runtime_invocation_context_injects_structured_client_budget() {
        // Build the gRPC-specific Lua invocation context from trusted client identity.
        // 基于受信任客户端身份构造 gRPC 专用 Lua 调用上下文。
        let invocation_context = build_grpc_runtime_invocation_context(
            "GrpcClient",
            Some("1.0.0"),
            Some("request-1"),
            Some("tool"),
            Some("skill"),
        )
        .expect("gRPC runtime invocation context should build");

        assert_eq!(
            invocation_context.client_budget["client_name"].as_str(),
            Some("GrpcClient")
        );
        assert_eq!(
            invocation_context.client_budget["tool_name"].as_str(),
            Some("tool")
        );
        assert_eq!(
            invocation_context.client_budget["skill_name"].as_str(),
            Some("skill")
        );
        assert!(
            invocation_context.client_budget["tool_result"]["bytes"]
                .as_u64()
                .is_some()
        );
        assert!(
            invocation_context.client_budget["file_read"]["lines"]
                .as_i64()
                .is_some()
        );
        assert!(invocation_context.client_budget["tool_config"].is_object());
    }
}
