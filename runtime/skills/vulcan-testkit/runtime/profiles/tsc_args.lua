--[[
Shared TypeScript compiler argument policy for no-emit validation.
TypeScript 编译器 no-emit 验证的共享参数策略。
]]

-- Values in this set explicitly disable boolean noEmit behavior.
-- 此集合中的值会显式关闭 noEmit 布尔行为。
local FALSE_VALUES = {
    ["0"] = true,
    ["false"] = true,
    ["no"] = true,
    ["off"] = true,
}

--- Trim surrounding whitespace from one argument value.
--- 去除单个参数值的首尾空白。
--- @param value any Candidate argument value.
--- @return string
local function trim(value)
    return (tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--- Return the option name before any inline equals value.
--- 返回内联等号取值之前的选项名称。
--- @param value any Candidate argument value.
--- @return string
local function option_name(value)
    return trim(tostring(value or ""):match("^([^=]+)") or value):lower()
end

--- Return the inline value after an equals sign.
--- 返回等号之后的内联取值。
--- @param value any Candidate argument value.
--- @return string|nil
local function inline_value(value)
    return tostring(value or ""):match("^[^=]+=(.*)$")
end

--- Check whether a text value explicitly disables a boolean compiler option.
--- 检查文本值是否显式关闭某个布尔编译器选项。
--- @param value any Candidate boolean value.
--- @return boolean
local function is_false_value(value)
    return FALSE_VALUES[trim(value):lower()] == true
end

--- Find a noEmit argument form that explicitly disables noEmit.
--- 查找显式关闭 noEmit 的参数形式。
--- @param args table Argument array.
--- @param start_index number Start index.
--- @return string|nil
local function disabled_no_emit_arg(args, start_index)
    local index = start_index or 1
    while index <= #(args or {}) do
        local raw = tostring(args[index] or "")
        if option_name(raw) == "--noemit" then
            local inline = inline_value(raw)
            if inline ~= nil and is_false_value(inline) then
                return raw
            end
            local next_value = (args or {})[index + 1]
            if inline == nil and next_value ~= nil and is_false_value(next_value) then
                return raw .. " " .. tostring(next_value)
            end
        end
        index = index + 1
    end
    return nil
end

--- Check whether arguments include an enabled noEmit marker.
--- 检查参数是否包含启用状态的 noEmit 标记。
--- @param args table Argument array.
--- @param start_index number Start index.
--- @return boolean
local function has_enabled_no_emit(args, start_index)
    local index = start_index or 1
    while index <= #(args or {}) do
        local raw = tostring(args[index] or "")
        if option_name(raw) == "--noemit" then
            local inline = inline_value(raw)
            if inline == nil then
                return true
            end
            if trim(inline) ~= "" and not is_false_value(inline) then
                return true
            end
        end
        index = index + 1
    end
    return false
end

return {
    disabled_no_emit_arg = disabled_no_emit_arg,
    has_enabled_no_emit = has_enabled_no_emit,
}
