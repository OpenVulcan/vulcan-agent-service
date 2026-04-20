--[[
vulcan-lua-exec
Execute controlled runtime Lua code through the host-side runtime bridge and return a Markdown string result.
通过宿主侧运行时桥接执行受控的运行时 Lua 代码，并返回 Markdown 字符串结果。
]]

-- Execute one controlled runtime Lua request through the host-side luaexec bridge.
-- 通过宿主侧 luaexec 桥接执行一次受控的运行时 Lua 请求。
return function(args)
    if type(args) ~= "table" then
        error("vulcan-lua-exec expects a table input / vulcan-lua-exec 需要 table 输入")
    end

    local luaexec = vulcan and vulcan.runtime and vulcan.runtime.lua and vulcan.runtime.lua.exec
    if type(luaexec) ~= "function" then
        error("vulcan.runtime.lua.exec is not available / vulcan.runtime.lua.exec 当前不可用")
    end

    local result = luaexec(args)
    if type(result) ~= "string" then
        return "# Runtime Execution Error\n\n## Status\nFAILED\n\n## Error\n```text\nvulcan.runtime.lua.exec must return a string, got: "
            .. type(result)
            .. "\n```"
    end

    return result
end
