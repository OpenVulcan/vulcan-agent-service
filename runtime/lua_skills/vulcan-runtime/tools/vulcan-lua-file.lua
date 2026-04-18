--[[
vulcan-lua-file
Execute one Lua file through the host-side luaexec bridge and return a Markdown string result.
通过宿主侧 luaexec 桥接执行指定 Lua 文件，并返回 Markdown 字符串结果。
]]

-- Execute one controlled runtime Lua file request through the host-side luaexec bridge.
-- 通过宿主侧 luaexec 桥接执行一次受控的运行时 Lua 文件请求。
return function(args)
    if type(args) ~= "table" then
        error("vulcan-lua-file expects a table input / vulcan-lua-file 需要 table 输入")
    end

    local luaexec = vulcan and vulcan.luaexec
    if type(luaexec) ~= "function" then
        error("vulcan.luaexec is not available / vulcan.luaexec 当前不可用")
    end

    local result = luaexec(args)
    if type(result) ~= "string" then
        return "# Runtime Execution Error\n\n## Status\nFAILED\n\n## Error\n```text\nvulcan.luaexec must return a string, got: "
            .. type(result)
            .. "\n```"
    end

    return result
end
