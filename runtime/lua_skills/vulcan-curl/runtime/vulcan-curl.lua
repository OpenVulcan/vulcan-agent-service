--[[
vulcan-curl
Execute one HTTP request through lua-curl using curl-style argument parsing and return one Markdown string.
通过 lua-curl 按 curl 风格参数解析执行一次 HTTP 请求，并返回单个 Markdown 字符串。
]]

-- Load the shared HTTP helper module from the current tool directory.
-- 从当前工具目录加载共享 HTTP 辅助模块。
local function load_shared_http()
    local entry_dir = tostring(vulcan.context.entry_dir or ".")
    local helper_path = vulcan.path.join(entry_dir, "shared_http.lua")
    local chunk, load_error = loadfile(helper_path)
    if not chunk then
        error("Failed to load shared_http.lua: " .. tostring(load_error))
    end

    local ok, helpers = pcall(chunk)
    if not ok or type(helpers) ~= "table" then
        error("shared_http.lua did not return a helper table: " .. tostring(helpers))
    end

    return helpers
end

-- Run the curl tool with a protected boundary so user-facing output stays stable.
-- 使用保护边界执行 curl 工具，保证面向用户的输出保持稳定。
return function(args)
    local helpers = load_shared_http()

    local ok, result = xpcall(function()
        return helpers.execute_curl_args_request(args)
    end, function(error_text)
        return debug.traceback(tostring(error_text), 2)
    end)

    if ok then
        return result
    end

    return helpers.render_error_markdown(result)
end
