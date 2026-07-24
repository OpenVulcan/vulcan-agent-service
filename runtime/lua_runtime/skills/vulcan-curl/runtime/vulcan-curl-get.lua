--[[
vulcan-curl-get
Execute one simple GET request through lua-curl using AI-friendly structured input.
通过 lua-curl 使用面向 AI 的结构化输入执行一次简单 GET 请求。
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

-- Run the simple GET tool with a protected boundary so output stays stable.
-- 使用保护边界执行简单 GET 工具，保证输出保持稳定。
return function(args)
    local helpers = load_shared_http()

    local ok, result = xpcall(function()
        local base_dir = tostring((args and args.cwd) or helpers.resolve_runtime_cwd())
        local spec = helpers.build_get_spec(args or {}, base_dir)
        return helpers.execute_parsed_request(spec, base_dir, tonumber(args and args.timeout_ms) or helpers.default_timeout_ms())
    end, function(error_text)
        return debug.traceback(tostring(error_text), 2)
    end)

    if ok then
        return result
    end

    return helpers.render_error_markdown(result)
end
