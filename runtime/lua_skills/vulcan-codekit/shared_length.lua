--[[
shared_length
中文：为 vulcan-codekit 提供统一的客户端预算读取逻辑。预算由宿主 Rust 统一解析，这里只消费最终结果。
English: Provide unified client-budget access for vulcan-codekit. Budgets are resolved by the Rust host, and Lua only consumes the final result.
]]

local DEFAULT_AST_CLIENT_CHAR_LIMIT = 10000

--[[
中文：去除字符串首尾空白，确保宿主传入的调试字段与名称字段在 Lua 侧读取稳定。
English: Trim leading and trailing whitespace so host-provided debug and name fields remain stable when read from Lua.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
中文：从 `vulcan` 上下文中读取宿主已经解析完成的客户端预算对象。
English: Read the client-budget object that has already been resolved by the host and injected into `vulcan`.
]]
local function resolve_client_budget(vulcan_context)
    local context = type(vulcan_context) == "table" and vulcan_context or nil
    if not context then
        return nil
    end

    local budget = context.client_budget
    if type(budget) == "table" then
        return budget
    end

    if type(context.context) == "table" and type(context.context.client_budget) == "table" then
        return context.context.client_budget
    end

    return nil
end

--[[
中文：从 `vulcan` 上下文中读取宿主注入的当前工具配置；若未命中工具配置，则返回 nil。
English: Read the current tool config injected by the host from `vulcan`; return nil when no tool-specific config matched.
]]
local function resolve_tool_config(vulcan_context)
    local context = type(vulcan_context) == "table" and vulcan_context or nil
    if not context then
        return nil
    end

    local tool_config = context.tool_config
    if type(tool_config) == "table" then
        return tool_config
    end

    return nil
end

--[[
中文：解析 codekit 当前应使用的安全内联字节预算；若宿主未提供有效值，则回退到保守默认值。
English: Resolve the safe inline byte budget codekit should use right now; fall back to a conservative default when the host does not provide a valid value.
]]
local function resolve_client_char_limit(vulcan_context)
    local budget = resolve_client_budget(vulcan_context)
    local tool_config = resolve_tool_config(vulcan_context)
    local scope_name = "tool_output"
    if type(tool_config) == "table" and type(tool_config.budget_scope) == "string" then
        local configured_scope = trim(tool_config.budget_scope)
        if configured_scope ~= "" then
            scope_name = configured_scope
        end
    end

    if type(budget) == "table" then
        local budgets = budget.budgets
        if type(budgets) == "table" then
            local scoped_budget = budgets[scope_name]
            if type(scoped_budget) == "table" then
                local bytes_limit = tonumber(scoped_budget.bytes)
                if bytes_limit and bytes_limit > 0 then
                    return math.floor(bytes_limit)
                end
            end
        end
    end

    return DEFAULT_AST_CLIENT_CHAR_LIMIT
end

--[[
中文：在单次工具调用开始时初始化当前 codekit 使用的预算；本质上是对宿主预算的轻量包装。
English: Initialize the budget used by the current codekit call; this is effectively a thin wrapper over the host-provided budget.
]]
local function initialize_client_char_limit(vulcan_context)
    return resolve_client_char_limit(vulcan_context)
end

return {
    DEFAULT_AST_CLIENT_CHAR_LIMIT = DEFAULT_AST_CLIENT_CHAR_LIMIT,
    resolve_client_budget = resolve_client_budget,
    resolve_tool_config = resolve_tool_config,
    resolve_client_char_limit = resolve_client_char_limit,
    initialize_client_char_limit = initialize_client_char_limit,
}
