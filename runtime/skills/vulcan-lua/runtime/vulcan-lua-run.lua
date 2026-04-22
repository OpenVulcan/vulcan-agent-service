--[[
vulcan-lua-run
Run one controlled Lua task through the host-side runtime bridge by accepting either inline code or one Lua file.
通过宿主侧运行时桥接执行一项受控 Lua 任务，支持内联代码或单个 Lua 文件两种输入。
]]

-- Render one stable input validation error so callers can see which field is invalid.
-- 渲染统一的输入校验错误，便于调用方直接看到哪个字段不合法。
local function render_input_error(message)
    return "# Runtime Input Error\n\n## Status\nFAILED\n\n## Error\n```text\n" .. message .. "\n```"
end

-- Decide whether one value is a blank string after trimming whitespace-only content.
-- 判断一个值在去除纯空白内容后是否仍然属于空字符串。
local function is_blank_string(value)
    return type(value) ~= "string" or value:match("^%s*$") ~= nil
end

-- Validate one `vulcan-lua-run` input table and require exactly one execution source.
-- 校验 `vulcan-lua-run` 输入表，并强制要求只能提供一种执行来源。
local function validate_request(input)
    if input == nil then
        input = {}
    end
    if type(input) ~= "table" then
        return nil, "vulcan-lua-run expects a table input / vulcan-lua-run 需要 table 输入"
    end

    if input.task ~= nil and type(input.task) ~= "string" then
        return nil, "`task` must be a string when provided / 提供 `task` 时必须是字符串"
    end

    if input.code ~= nil and type(input.code) ~= "string" then
        return nil, "`code` must be a string when provided / 提供 `code` 时必须是字符串"
    end

    if input.file ~= nil and type(input.file) ~= "string" then
        return nil, "`file` must be a string when provided / 提供 `file` 时必须是字符串"
    end

    if input.args ~= nil and type(input.args) ~= "table" then
        return nil, "`args` must be an object table when provided / 提供 `args` 时必须是对象 table"
    end

    if input.timeout_ms ~= nil then
        if type(input.timeout_ms) ~= "number" then
            return nil, "`timeout_ms` must be a number when provided / 提供 `timeout_ms` 时必须是数字"
        end
        if input.timeout_ms <= 0 then
            return nil, "`timeout_ms` must be greater than 0 / `timeout_ms` 必须大于 0"
        end
    end

    local has_code = not is_blank_string(input.code)
    local has_file = not is_blank_string(input.file)
    if has_code == has_file then
        return nil,
            "vulcan-lua-run requires exactly one of `code` or `file` / vulcan-lua-run 必须且只能传入 `code` 或 `file` 其中之一"
    end

    return {
        task = is_blank_string(input.task) and nil or input.task,
        code = has_code and input.code or nil,
        file = has_file and input.file or nil,
        args = input.args or {},
        timeout_ms = input.timeout_ms,
    }, nil
end

-- Execute one controlled runtime Lua request through the host-side luaexec bridge.
-- 通过宿主侧 luaexec 桥接执行一次受控运行时 Lua 请求。
return function(args)
    local request, validation_error = validate_request(args)
    if validation_error ~= nil then
        return render_input_error(validation_error)
    end

    local luaexec = vulcan and vulcan.runtime and vulcan.runtime.lua and vulcan.runtime.lua.exec
    if type(luaexec) ~= "function" then
        return render_input_error(
            "vulcan.runtime.lua.exec is not available / vulcan.runtime.lua.exec 当前不可用"
        )
    end

    local result = luaexec(request)
    if type(result) ~= "string" then
        return "# Runtime Execution Error\n\n## Status\nFAILED\n\n## Error\n```text\nvulcan.runtime.lua.exec must return a string, got: "
            .. type(result)
            .. "\n```"
    end

    return result
end
