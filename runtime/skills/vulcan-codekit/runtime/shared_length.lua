--[[
shared_length
为 vulcan-codekit 提供统一的 MCP 预算读取逻辑。
宿主 Rust 已经完成预算解析与预估，这里只直接消费最终的 `tool_result/file_read -> bytes/lines` 结果。
Provide unified MCP budget access for vulcan-codekit.
The Rust host has already resolved and estimated the final budgets, so Lua only consumes the final
`tool_result/file_read -> bytes/lines` values directly.
]]

--[[
去除字符串首尾空白，确保宿主传入的 scope 名称与工具配置字段读取稳定。
Trim leading and trailing whitespace so host-provided scope names and tool-config fields are read consistently.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
从传入对象中解析真正的运行时上下文表。
Resolve the actual runtime context table from the incoming object.
]]
local function resolve_runtime_context(vulcan_context)
    local root = type(vulcan_context) == "table" and vulcan_context or nil
    if not root then
        return nil
    end

    if type(root.context) == "table" then
        return root.context
    end

    return root
end

--[[
从 `vulcan` 上下文中读取宿主已经解析完成的客户端预算对象。
当前统一语法下，预算对象直接暴露 `tool_result` 与 `file_read` 两个 scope。
当前统一语法下，预算对象直接暴露 `tool_result` 与 `file_read` 两个 scope。
Read the client-budget object already resolved by the host from the `vulcan` context.
Under the unified syntax, the budget object directly exposes the `tool_result` and `file_read` scopes.
]]
local function resolve_client_budget(vulcan_context)
    local context = resolve_runtime_context(vulcan_context)
    if not context then
        return nil
    end
    return type(context.client_budget) == "table" and context.client_budget or nil
end

--[[
从 `vulcan` 上下文中读取宿主注入的当前工具配置；若未命中工具配置，则返回 nil。
Read the current tool config injected by the host from `vulcan`; return nil when no tool-specific config matched.
]]
local function resolve_tool_config(vulcan_context)
    local context = resolve_runtime_context(vulcan_context)
    if not context then
        return nil
    end
    return type(context.tool_config) == "table" and context.tool_config or nil
end

--[[
根据工具配置选择当前应使用的预算 scope。
当前宿主统一提供：
- tool_result：MCP 工具结果返回预算
- file_read：客户端文件读取预算
若未配置 scope，则默认使用 tool_result。
Select the budget scope currently required by the tool configuration.
The host now exposes two unified scopes:
- tool_result: final MCP tool-result budget
- file_read: client-side file-read budget
When no scope is configured, `tool_result` is used by default.
]]
local function resolve_client_budget_scope(vulcan_context)
    local budget = resolve_client_budget(vulcan_context)
    if type(budget) ~= "table" then
        return nil, {
            error = "missing_client_budget",
            message = "vulcan.context.client_budget is required and must be injected by the MCP host",
        }
    end
    local tool_config = resolve_tool_config(vulcan_context)
    local scope_name = "tool_result"

    if type(tool_config) == "table" and type(tool_config.budget_scope) == "string" then
        local configured_scope = trim(tool_config.budget_scope)
        if configured_scope == "file_read" or configured_scope == "tool_result" then
            scope_name = configured_scope
        end
    end

    local scoped_budget = budget[scope_name]
    if type(scoped_budget) ~= "table" then
        return nil, {
            error = "missing_budget_scope",
            message = string.format("vulcan.context.client_budget.%s is required", scope_name),
            scope = scope_name,
        }
    end

    local bytes_limit = tonumber(scoped_budget.bytes)
    if not bytes_limit or bytes_limit <= 0 then
        return nil, {
            error = "invalid_budget_bytes",
            message = string.format("vulcan.context.client_budget.%s.bytes must be a positive number", scope_name),
            scope = scope_name,
            actual_value = scoped_budget.bytes,
        }
    end

    local lines_limit = tonumber(scoped_budget.lines)
    if lines_limit == nil then
        return nil, {
            error = "invalid_budget_lines",
            message = string.format("vulcan.context.client_budget.%s.lines must be provided", scope_name),
            scope = scope_name,
            actual_value = scoped_budget.lines,
        }
    end

    return {
        bytes = math.floor(bytes_limit),
        lines = math.floor(lines_limit),
    }, nil
end

--[[
在单次工具调用开始时初始化当前 codekit 使用的完整预算对象，包含 bytes 与 lines。
Initialize the full budget object used by the current codekit call, including bytes and lines.
]]
local function initialize_client_budget(vulcan_context)
    return resolve_client_budget_scope(vulcan_context)
end

return {
    resolve_client_budget = resolve_client_budget,
    resolve_tool_config = resolve_tool_config,
    resolve_client_budget_scope = resolve_client_budget_scope,
    initialize_client_budget = initialize_client_budget,
}
