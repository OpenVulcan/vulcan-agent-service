--[[
vulcan-lua-help
Return the current runtime execution manual, supported libraries, and output rules.
返回当前运行时执行手册、支持库清单与输出规则。
]]

-- Trim one text value safely.
-- 安全裁剪一段文本的首尾空白。
local function trim(text)
    if type(text) ~= "string" then
        return ""
    end
    return text:match("^%s*(.-)%s*$") or ""
end

-- Split a text block into lines.
-- 把一段文本拆分成按行数组。
local function split_lines(text)
    local lines = {}
    if type(text) ~= "string" or text == "" then
        return lines
    end
    text = text:gsub("\r\n", "\n")
    for line in text:gmatch("([^\n]*)\n?") do
        if line == "" and #lines > 0 and lines[#lines] == "" then
            break
        end
        table.insert(lines, line)
    end
    return lines
end

-- Return whether one file path currently exists.
-- 判断某个文件路径当前是否存在。
local function file_exists(path)
    if type(path) ~= "string" or path == "" then
        return false
    end
    local ok, exists = pcall(vulcan.fs_exists, path)
    return ok and exists == true
end

-- Resolve the best available lua_packages list path.
-- 解析当前最合适的 lua_packages 列表文件路径。
local function resolve_packages_file()
    local candidates = {}
    if type(vulcan.resources_dir) == "string" and vulcan.resources_dir ~= "" then
        table.insert(candidates, vulcan.path_join(vulcan.resources_dir, "lua_packages.txt"))
    end
    local cwd = vulcan.cwd()
    if type(cwd) == "string" and cwd ~= "" then
        table.insert(candidates, vulcan.path_join(cwd, "scripts", "lua_packages.txt"))
    end

    for _, candidate in ipairs(candidates) do
        if file_exists(candidate) then
            return candidate
        end
    end
    return nil
end

-- Map installed rock names into the most common Lua require names.
-- 把安装包名称映射成最常见的 Lua require 名称。
local function package_module_hints(rock_name)
    local mapping = {
        ["lua-cjson"] = "cjson",
        ["luafilesystem"] = "lfs",
        ["luasocket"] = "socket / socket.http / ltn12",
        ["luasec"] = "ssl / ssl.https",
        ["lrexlib-pcre2"] = "rex_pcre2",
        ["luaossl"] = "openssl",
        ["lyaml"] = "yaml",
        ["lua-toml"] = "toml",
        ["serpent"] = "serpent",
        ["lua-zlib"] = "zlib",
    }
    return mapping[rock_name]
end

-- Parse `pkg` lines from the configured lua_packages list file.
-- 从 lua_packages 配置文件里解析 `pkg` 行。
local function parse_supported_packages(package_file)
    if not package_file then
        return {}, nil
    end

    local ok, content = pcall(vulcan.fs_read, package_file)
    if not ok or type(content) ~= "string" then
        return {}, package_file
    end

    local packages = {}
    for _, line in ipairs(split_lines(content)) do
        local cleaned = trim(line)
        if cleaned ~= "" and not cleaned:match("^#") then
            local rock_name, version = cleaned:match("^pkg%s+([%w%._%-]+)%s*([%w%._%-]*)$")
            if rock_name then
                table.insert(packages, {
                    rock = rock_name,
                    version = version ~= "" and version or nil,
                    module = package_module_hints(rock_name),
                })
            end
        end
    end

    table.sort(packages, function(left, right)
        return left.rock < right.rock
    end)
    return packages, package_file
end

-- Render one supported package entry as Markdown text.
-- 将单个支持库条目渲染为 Markdown 文本。
local function render_package_line(package_info)
    local line = "- `" .. package_info.rock .. "`"
    if package_info.module then
        line = line .. " -> `" .. package_info.module .. "`"
    end
    if package_info.version then
        line = line .. " (version: `" .. package_info.version .. "`)"
    end
    return line
end

-- Build the help document returned to callers.
-- 构建返回给调用方的帮助文档。
local function build_help_markdown()
    local packages_file = resolve_packages_file()
    local packages, source_file = parse_supported_packages(packages_file)

    local lines = {
        "# Vulcan Lua Runtime Help",
        "",
        "## When To Use",
        "- Use `vulcan-lua-exec` when you need quick ad-hoc Lua execution, short loops, one-off file generation, lightweight data conversion, regex handling, network probing, or simple command orchestration.",
        "- Use `vulcan-lua-file` when you already have a Lua script file and want it executed with the working directory automatically switched to that file's directory.",
        "- Before calling either execution tool, read this help first so you know the currently supported runtime APIs, installed Lua libraries, return rules, and disabled capabilities.",
        "",
        "## Tool Reference",
        "### `vulcan-lua-exec`",
        "- Best for short ad-hoc Lua code, one-off loops, lightweight data conversion, temporary file generation, quick network probing, or shell command orchestration.",
        "- Input shape: `{ task?, code, args?, timeout_ms? }`",
        "- `task` is an optional human-readable summary shown in the result header.",
        "- `code` is required and contains the Lua source to run inside the isolated runtime VM.",
        "- `args` is optional and becomes the local variable `args` inside the executed code.",
        "- `timeout_ms` is optional; if omitted, the default timeout is `60000` milliseconds.",
        "- Result is always one Markdown string. `print(...)` output is captured, tables are rendered as pretty JSON text, and multiple return values are rendered item by item.",
        "",
        "### `vulcan-lua-file`",
        "- Best when the logic is easier to maintain in an existing `.lua` file, especially if the script uses file-relative paths or has multi-step flow.",
        "- Input shape: `{ task?, file, args?, timeout_ms? }`",
        "- `file` is required and points to the Lua script file to execute.",
        "- During execution, the working directory automatically switches to the target file directory, and `vulcan.entry_file` / `vulcan.entry_dir` point to that file context.",
        "- `args` and `timeout_ms` behave the same as in `vulcan-lua-exec`.",
        "- Result is also always one Markdown string with the same capture and rendering rules.",
        "",
        "## Runtime APIs Available Inside The Execution Environment",
        "- `print(...)`",
        "- `vulcan.fs_list(dir)`",
        "- `vulcan.fs_read(path)`",
        "- `vulcan.fs_write(path, content)`",
        "- `vulcan.fs_exists(path)`",
        "- `vulcan.fs_is_dir(path)`",
        "- `vulcan.path_join(...)`",
        "- `vulcan.cwd()`",
        "- `vulcan.temp_dir`",
        "- `vulcan.exec(spec)`",
        "- `vulcan.osinfo()`",
        "- `vulcan.json_encode(value)`",
        "- `vulcan.json_decode(text)`",
        "- `vulcan.call(name, args)`",
        "",
        "## Intentionally Disabled Inside The Execution Environment",
        "- `vulcan.luaexec` is disabled inside `vulcan-lua-exec` and `vulcan-lua-file`, so runtime recursion is not allowed.",
        "- `vulcan.log` is not registered in the isolated runtime VM.",
        "- `vulcan.cache_put/get/delete` are not registered in the isolated runtime VM.",
        "",
        "## Tool Call Rules Inside The Execution Environment",
        "- `vulcan.call(name, args)` may call other Vulcan MCP tools and returns their normal string result.",
        "- This is mainly a compatibility and composition feature. It is available, but it is not the recommended primary path for normal runtime tasks.",
        "- Internal tool calls run under the restricted simulated client `luaexec_call`, which currently defaults to `tool_result.bytes=10000` and `tool_result.lines=-1`.",
        "- Example: `local text = vulcan.call(\"codekit-markdown-menu\", { path = \"D:/work/docs\", recursive = false })`",
        "- You must not call the current tool that launched the active `luaexec` request.",
        "- You must not call `vulcan-lua-exec` or `vulcan-lua-file` from inside `vulcan-lua-exec` or `vulcan-lua-file`.",
        "",
        "## Output Rules",
        "- Execution tools always return one Markdown string.",
        "- `print(...)` output is captured and rendered under `Printed Output`; it does not go to the host console.",
        "- `return table` becomes pretty-printed JSON text.",
        "- Multiple return values are rendered item by item.",
        "- If there is no `return`, the rendered result shows `null`.",
        "- Long output uses truncation only; it does not page.",
        "",
        "## Timeout Rules",
        "- `timeout_ms` is optional on both execution tools.",
        "- If omitted, the default timeout is `60000` milliseconds.",
        "- Timeouts are enforced by the host, and a timeout returns a normal Markdown error result.",
        "",
        "## Supported Lua Packages",
    }

    if source_file then
        table.insert(lines, "- Source file: `" .. source_file .. "`")
    else
        table.insert(lines, "- Source file: not found")
    end

    if #packages == 0 then
        table.insert(lines, "- No package list could be loaded at runtime.")
    else
        for _, package_info in ipairs(packages) do
            table.insert(lines, render_package_line(package_info))
        end
    end

    table.insert(lines, "")
    table.insert(lines, "## Input Reminder")
    table.insert(lines, "- `vulcan-lua-exec` expects `{ task?, code, args?, timeout_ms? }` and is preferred for short inline logic.")
    table.insert(lines, "- `vulcan-lua-file` expects `{ task?, file, args?, timeout_ms? }` and is preferred for reusable or file-relative scripts.")

    return table.concat(lines, "\n")
end

-- Return the runtime help document as a plain Markdown string.
-- 以纯 Markdown 字符串返回运行时帮助文档。
return function()
    return build_help_markdown()
end
