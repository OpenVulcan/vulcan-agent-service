--[[
shared_length
中文：为 vulcan-codekit 提供统一的客户端字符预算规则，避免 detail/tree/rg 各自维护一套长度限制映射。
English: Provide a shared client-length budget policy for vulcan-codekit so detail/tree/rg do not each maintain their own mapping.
]]

local DEFAULT_AST_CLIENT_CHAR_LIMIT = 10000

--[[
中文：去除字符串首尾空白，保证客户端名称比较稳定。
English: Trim leading and trailing whitespace so client-name comparisons remain stable.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
中文：判断字符串是否以前缀开头，用于客户端名规则匹配。
English: Check whether a string starts with a prefix for client-name rule matching.
]]
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
中文：从当前请求上下文中提取客户端名称，用于决定字符预算。
English: Resolve the current client name from request context so the shared budget can be determined.
]]
local function resolve_current_client_name(vulcan_context)
    local context = type(vulcan_context) == "table" and vulcan_context or nil
    if not context then
        return nil
    end

    local client_info = context.client_info
    if type(client_info) ~= "table" and type(context.context) == "table" then
        client_info = context.context.client_info
    end
    if type(client_info) ~= "table" then
        return nil
    end

    local client_name = trim(client_info.name or "")
    if client_name == "" then
        return nil
    end
    return client_name
end

--[[
中文：根据客户端名称映射字符预算，统一所有 codekit 工具的长度限制口径。
English: Map a client name to the shared character budget used across all codekit tools.
]]
local function resolve_client_char_limit(client_name)
    local normalized_name = trim(client_name or ""):lower()
    if normalized_name == "" then
        return DEFAULT_AST_CLIENT_CHAR_LIMIT
    end
    if starts_with(normalized_name, "qwen-code-mcp-client") or normalized_name:find("qwen", 1, true) then
        return 25000
    end
    if normalized_name == "codex-mcp-client" then
        return 10000
    end
    if normalized_name:find("opencode", 1, true) or normalized_name:find("claude-code", 1, true) then
        return 50000
    end
    return DEFAULT_AST_CLIENT_CHAR_LIMIT
end

--[[
中文：直接从 `vulcan` 上下文初始化当前工具调用应使用的字符预算。
English: Initialize the character budget for the current tool call directly from the `vulcan` context.
]]
local function initialize_client_char_limit(vulcan_context)
    return resolve_client_char_limit(resolve_current_client_name(vulcan_context))
end

return {
    DEFAULT_AST_CLIENT_CHAR_LIMIT = DEFAULT_AST_CLIENT_CHAR_LIMIT,
    resolve_current_client_name = resolve_current_client_name,
    resolve_client_char_limit = resolve_client_char_limit,
    initialize_client_char_limit = initialize_client_char_limit,
}
