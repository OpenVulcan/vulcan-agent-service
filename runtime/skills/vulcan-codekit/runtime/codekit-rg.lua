--[[
codekit-rg
先基于 ripgrep 做文本命中，再结合 codekit-ast-detail 的结构能力，仅输出与命中行直接相关的 AST 结构。
Perform ripgrep text matching first, then reuse codekit-ast-detail structural analysis to return only AST structures directly related to the matched lines.
]]

-- 工具常量 / Tool constants for rg execution and response shaping.
local RG_TIMEOUT_MS = 30000
local MAX_MATCH_LINES_PER_SYMBOL = 12
local LFS_MODULE = nil
local SHARED_LENGTH_HELPERS = nil

-- 缓存的 codekit-ast-detail 助手集合 / Cached codekit-ast-detail helper bundle extracted from the existing skill entry.
local AST_RUNTIME_HELPERS = nil

-- 基础字符串工具 / Basic string helpers shared by validation, parsing, and rendering.
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

local function split_lines(content)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    local lines = {}
    if normalized == "" then
        return lines
    end
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end
    return lines
end

--[[
浅拷贝数组，避免在树渲染或结果拼装时直接改写原始列表。
Create a shallow array copy so tree rendering and result assembly do not mutate the original list in place.

参数 / Parameters:
- items(table|nil): 待复制的数组 / Array to clone.

返回 / Returns:
- table: 浅拷贝后的新数组 / Shallow-copied array.
]]
local function clone_array(items)
    local copied = {}
    for _, item in ipairs(items or {}) do
        table.insert(copied, item)
    end
    return copied
end

--[[
统一格式化结构范围，输出 `Lx-y` 或 `Lx` 形式，便于结果直接定位到代码区间。
Format structural ranges into `Lx-y` or `Lx` so the output can be used as an immediate line anchor.

参数 / Parameters:
- start_line(number): 起始行号 / 1-based start line.
- end_line(number): 结束行号 / 1-based end line.

返回 / Returns:
- string: 规范化后的行号范围文本 / Normalized line-span text.
]]
local function format_line_span(start_line, end_line)
    local normalized_start = tonumber(start_line) or 0
    local normalized_end = tonumber(end_line) or normalized_start
    if normalized_end < normalized_start then
        normalized_end = normalized_start
    end
    if normalized_start <= 0 then
        return "L?"
    end
    if normalized_start == normalized_end then
        return string.format("L%d", normalized_start)
    end
    return string.format("L%d-%d", normalized_start, normalized_end)
end

--[[
获取宿主注入的当前 skill 目录。
Resolve the current skill directory injected by the host.

参数 / Parameters:
- 无 / None.

返回 / Returns:
- string: 当前 skill 目录 / Current skill directory.
]]
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

local function get_entry_file()
    return tostring(vulcan.context.entry_file or vulcan.path.join(get_entry_dir(), "codekit-rg.lua"))
end

--[[
解析当前运行时可用的宿主进程执行函数，仅接受正式节点 `vulcan.process.exec`。
Resolve the host-side process execution function and accept only the formal node `vulcan.process.exec`.

返回 / Returns:
- function|nil: 可调用的宿主执行函数；若宿主未注入则返回 nil。
  Callable host execution function, or nil when the host did not inject one.
]]
local function get_host_exec_function()
    if type(vulcan) ~= "table" then
        return nil
    end
    if type(vulcan.process) == "table" and type(vulcan.process.exec) == "function" then
        return vulcan.process.exec
    end
    return nil
end

--[[
Return the normalized platform key used by LuaSkills dependency installation.
返回 LuaSkills 依赖安装使用的标准平台键。
]]
local function current_platform_key()
    local os_info = vulcan.os.info() or {}
    local architecture = trim((os_info.arch or os_info.architecture or "")):lower()
    local os_name = trim((os_info.os or "")):lower()

    if os_name == "windows" then
        if architecture == "arm64" or architecture == "aarch64" then
            return "windows-arm64"
        end
        return "windows-x64"
    end

    if os_name == "macos" or os_name == "darwin" or os_name == "osx" then
        if architecture == "arm64" or architecture == "aarch64" then
            return "macos-arm64"
        end
        return "macos-x64"
    end

    if architecture == "arm64" or architecture == "aarch64" then
        return "linux-arm64"
    end
    return "linux-x64"
end

--[[
Return the host-injected tool dependency root for the current skill.
返回宿主为当前 skill 注入的工具依赖根目录。
]]
local function get_tool_dependency_root()
    return trim(vulcan and vulcan.deps and vulcan.deps.tools_path or "")
end

--[[
Build one tool binary path from the injected dependency root, dependency name, version, and executable name.
基于注入的依赖根目录、依赖名、版本号与程序名构造工具二进制路径。
]]
local function build_tool_binary_path(dependency_name, version, executable_name)
    local tools_root = get_tool_dependency_root()
    if tools_root == "" then
        return ""
    end
    return vulcan.path.join(
        tools_root,
        tostring(dependency_name or ""),
        tostring(version or ""),
        current_platform_key(),
        "bin",
        tostring(executable_name or "")
    )
end

--[[
懒加载共享预算模块，让 rg/detail/tree 复用同一套 MCP 输出/读取预算映射。
Lazily load the shared budget module so rg/detail/tree reuse the same MCP output/read budget mapping.
]]
local function load_shared_length_helpers()
    if SHARED_LENGTH_HELPERS then
        return SHARED_LENGTH_HELPERS, nil
    end

    local helper_path = vulcan.path.join(get_entry_dir(), "shared_length.lua")
    local chunk, load_error = loadfile(helper_path)
    if not chunk then
        return nil, {
            error = "shared_length_load_failed",
            message = tostring(load_error),
            path = helper_path,
        }
    end

    local ok, helpers = pcall(chunk)
    if not ok or type(helpers) ~= "table" then
        return nil, {
            error = "shared_length_invalid",
            message = ok and "shared_length.lua did not return a table" or tostring(helpers),
            path = helper_path,
        }
    end

    SHARED_LENGTH_HELPERS = helpers
    return SHARED_LENGTH_HELPERS, nil
end

--[[
在单次工具调用开始时初始化当前客户端的 RG 预算。
Initialize the current RG budget at the start of each tool call.
]]
local function initialize_rg_client_budget()
    local helpers, helper_error = load_shared_length_helpers()
    if helper_error then
        return nil, helper_error
    end
    return helpers.initialize_client_budget(vulcan)
end

--[[
通过 `debug.getupvalue` 从现有 `codekit-ast-detail` 入口中提取内部助手函数，避免复制一整套 AST 解析实现。
Extract internal helper functions from the existing `codekit-ast-detail` entry with `debug.getupvalue` to avoid duplicating the full AST parsing pipeline.

参数 / Parameters:
- fn(function): 待检查 upvalue 的函数 / Function whose upvalues will be inspected.
- name(string): 目标 upvalue 名称 / Target upvalue name.

返回 / Returns:
- any|nil: 命中的 upvalue 值；未找到时返回 nil。
  Matched upvalue value, or nil when not found.
]]
local function extract_upvalue_by_name(fn, name)
    local index = 1
    while true do
        local upvalue_name, upvalue_value = debug.getupvalue(fn, index)
        if not upvalue_name then
            return nil
        end
        if upvalue_name == name then
            return upvalue_value
        end
        index = index + 1
    end
end

--[[
懒加载 `codekit-ast-detail` 内部助手，确保 `codekit-rg` 与现有 AST 规则、文件收集和结构归一化逻辑保持一致。
Lazily load internal `codekit-ast-detail` helpers so `codekit-rg` stays aligned with the existing AST rules, file collection logic, and symbol normalization flow.

参数 / Parameters:
- 无 / None.

返回 / Returns:
- table|nil: 提取成功后的助手函数集合 / Extracted helper bundle on success.
- table|nil: 加载失败时的结构化错误对象 / Structured error object on failure.
]]
local function load_ast_runtime_helpers()
    if AST_RUNTIME_HELPERS then
        return AST_RUNTIME_HELPERS, nil
    end

    local ast_entry_path = vulcan.path.join(get_entry_dir(), "codekit-ast-detail.lua")
    local chunk, load_error = loadfile(ast_entry_path)
    if not chunk then
        return nil, {
            error = "codekit_ast_entry_load_failed",
            message = tostring(load_error),
            path = ast_entry_path,
        }
    end

    local ok, ast_entry = pcall(chunk)
    if not ok or type(ast_entry) ~= "function" then
        return nil, {
            error = "codekit_ast_entry_invalid",
            message = ok and "codekit-ast-detail entry did not return a function" or tostring(ast_entry),
            path = ast_entry_path,
        }
    end

    local helpers = {
        validate_extension_argument = extract_upvalue_by_name(ast_entry, "validate_extension_argument"),
        validate_noignore_argument = extract_upvalue_by_name(ast_entry, "validate_noignore_argument"),
        collect_files = extract_upvalue_by_name(ast_entry, "collect_files"),
        find_binary = extract_upvalue_by_name(ast_entry, "find_binary"),
        run_language_scan = extract_upvalue_by_name(ast_entry, "run_language_scan"),
        normalize_symbol = extract_upvalue_by_name(ast_entry, "normalize_symbol"),
        deduplicate_symbols = extract_upvalue_by_name(ast_entry, "deduplicate_symbols"),
        build_symbol_tree = extract_upvalue_by_name(ast_entry, "build_symbol_tree"),
        get_file_line_count = extract_upvalue_by_name(ast_entry, "get_file_line_count"),
    }

    for helper_name, helper_value in pairs(helpers) do
        if type(helper_value) ~= "function" then
            return nil, {
                error = "codekit_ast_helper_missing",
                message = "required helper missing from codekit-ast-detail runtime",
                helper = helper_name,
                path = ast_entry_path,
            }
        end
    end

    AST_RUNTIME_HELPERS = helpers
    return AST_RUNTIME_HELPERS, nil
end

--[[
校验目录参数，要求非空字符串且必须是已存在目录。
Validate the directory argument. It must be a non-empty string pointing to an existing directory.

参数 / Parameters:
- value(any): 用户传入的目录参数 / User-provided directory argument.

返回 / Returns:
- string|nil: 规范化后的目录路径 / Normalized directory path.
- table|nil: 参数非法时返回结构化错误对象 / Structured error object when invalid.
]]
local function validate_directory_argument(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_dir_argument",
            message = "dir must be a non-empty string",
            actual_type = type(value),
        }
    end

    local normalized = trim(value)
    if not vulcan.fs.exists(normalized) then
        return nil, {
            error = "dir_not_found",
            message = "dir does not exist",
            dir = normalized,
        }
    end
    if not vulcan.fs.is_dir(normalized) then
        return nil, {
            error = "dir_must_be_directory",
            message = "dir must point to an existing directory",
            dir = normalized,
        }
    end
    return normalized, nil
end

--[[
校验 ripgrep 正则参数，要求非空字符串。
Validate the ripgrep regex argument. It must be a non-empty string.

参数 / Parameters:
- value(any): 用户传入的 rg 正则 / User-provided rg regular expression.

返回 / Returns:
- string|nil: 规范化后的 rg 正则 / Normalized rg pattern.
- table|nil: 参数非法时返回结构化错误对象 / Structured error object when invalid.
]]
local function validate_rg_pattern_argument(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_rg_pattern_argument",
            message = "rg_pattern must be a non-empty string",
            actual_type = type(value),
        }
    end
    return trim(value), nil
end

--[[
显式拒绝 `export_md_path` 参数，避免调用方误以为 `codekit-rg` 仍支持导出到指定目录。
Explicitly reject the `export_md_path` argument so callers do not assume `codekit-rg` still supports exporting to a chosen path.
]]
local function validate_export_md_absence(value)
    if value ~= nil then
        return {
            error = "export_md_path_not_supported",
            message = "codekit-rg no longer supports the export_md_path argument",
        }
    end
    return nil
end

local function get_lfs_module()
    if LFS_MODULE ~= nil then
        return LFS_MODULE
    end
    local ok, module = pcall(require, "lfs")
    if ok then
        LFS_MODULE = module
    else
        LFS_MODULE = false
    end
    return LFS_MODULE
end

local function get_parent_directory(path)
    local normalized = tostring(path or ""):gsub("[\\/]+$", "")
    local parent = normalized:match("^(.*)[\\/][^\\/]+$")
    if not parent or parent == normalized then
        return nil
    end
    return parent
end

local function append_path_segment(current, segment)
    if current == "" then
        return segment
    end
    if current == "/" then
        return "/" .. segment
    end
    if current:sub(-1) == "/" then
        return current .. segment
    end
    return current .. "/" .. segment
end

--[[
在 LuaFileSystem 不可用时，回退到宿主 `vulcan.process.exec` 递归创建目录，保证大结果落盘与 Markdown 导出仍可执行。
Fall back to host-side `vulcan.process.exec` recursive directory creation when LuaFileSystem is unavailable so large-result spilling and Markdown export still work.
]]
local function ensure_directory_via_exec(directory_path)
    local host_exec = get_host_exec_function()
    if type(host_exec) ~= "function" then
        return false, {
            error = "directory_creation_failed",
            message = "neither LuaFileSystem nor host process exec is available for directory creation",
            path = directory_path,
        }
    end

    local os_info = vulcan.os.info()
    local request
    if os_info and os_info.os == "windows" then
        request = {
            program = "powershell.exe",
            args = {
                "-NoProfile",
                "-Command",
                string.format("New-Item -ItemType Directory -Force -Path '%s' | Out-Null", tostring(directory_path):gsub("'", "''")),
            },
            timeout_ms = 10000,
        }
    else
        request = {
            program = "mkdir",
            args = { "-p", directory_path },
            timeout_ms = 10000,
        }
    end

    local ok, result = pcall(host_exec, request)
    if not ok or type(result) ~= "table" then
        return false, {
            error = "directory_creation_failed",
            message = ok and "unexpected exec result" or tostring(result),
            path = directory_path,
        }
    end
    if result.error or result.timed_out or result.success == false then
        return false, {
            error = "directory_creation_failed",
            message = trim(result.error or result.stderr or "mkdir failed"),
            path = directory_path,
        }
    end
    if vulcan.fs.exists(directory_path) and vulcan.fs.is_dir(directory_path) then
        return true, nil
    end
    return false, {
        error = "directory_creation_failed",
        message = "directory was not created",
        path = directory_path,
    }
end

local function ensure_directory(directory_path)
    local normalized = trim(directory_path or "")
    if normalized == "" then
        return false, {
            error = "directory_creation_failed",
            message = "directory path is empty",
        }
    end
    if vulcan.fs.exists(normalized) then
        if vulcan.fs.is_dir(normalized) then
            return true, nil
        end
        return false, {
            error = "directory_creation_failed",
            message = "target path already exists as a file",
            path = normalized,
        }
    end

    local lfs = get_lfs_module()
    if not lfs then
        return ensure_directory_via_exec(normalized)
    end

    local path_text = tostring(normalized):gsub("\\", "/")
    local prefix = ""
    if path_text:match("^%a:/") then
        prefix = path_text:sub(1, 3)
        path_text = path_text:sub(4)
    elseif starts_with(path_text, "/") then
        prefix = "/"
        path_text = path_text:sub(2)
    end

    local current = prefix
    for segment in path_text:gmatch("[^/]+") do
        current = append_path_segment(current, segment)
        if not vulcan.fs.exists(current) then
            local ok, mkdir_error = lfs.mkdir(current)
            if not ok then
                return false, {
                    error = "directory_creation_failed",
                    message = tostring(mkdir_error or "mkdir failed"),
                    path = current,
                }
            end
        elseif not vulcan.fs.is_dir(current) then
            return false, {
                error = "directory_creation_failed",
                message = "path exists but is not a directory",
                path = current,
            }
        end
    end
    return true, nil
end

local function shallow_copy_object(source)
    local copied = {}
    for key, value in pairs(source or {}) do
        copied[key] = value
    end
    return copied
end

--[[
Locate the `rg` executable from the host-injected dependency root instead of guessing the host directory layout.
从宿主注入的依赖根目录定位 `rg` 可执行文件，而不是猜测宿主目录布局。

参数 / Parameters:
- 无 / None.

返回 / Returns:
- string|nil: `rg` 可执行文件完整路径 / Full `rg` executable path.
- table|nil: 未找到时返回结构化错误对象 / Structured error object when not found.
]]
local function find_rg_binary()
    local executable_name = vulcan.os.info().os == "windows" and "rg.exe" or "rg"
    local full_path = build_tool_binary_path("rg", "14.1.1", executable_name)
    if vulcan.fs.exists(full_path) then
        return full_path, nil
    end
    return nil, {
        error = "rg_binary_not_found",
        message = "ripgrep binary not found in the current skill dependency root",
        expected_path = full_path,
    }
end

-- rg 参数与输出解析 / Build rg commands and parse the newline-delimited JSON stream.
local function quote_argument(value)
    return '"' .. tostring(value or ""):gsub('"', '\\"') .. '"'
end

local function build_rg_arguments(target_directory, extension_filter, rg_pattern, ignore_enabled)
    local arguments = { "--json", "--line-number", "--color=never", "-e", rg_pattern, target_directory }
    if ignore_enabled == false then
        table.insert(arguments, "--hidden")
        table.insert(arguments, "--no-ignore")
    end
    local extensions = {}
    if type(extension_filter) == "table" then
        for extension_name in pairs(extension_filter) do
            table.insert(extensions, extension_name)
        end
    end
    table.sort(extensions)
    for _, extension_name in ipairs(extensions) do
        table.insert(arguments, 1, "*." .. extension_name)
        table.insert(arguments, 1, "--glob")
    end
    return arguments
end

local function build_rg_command(rg_binary_path, arguments)
    local quoted_arguments = { quote_argument(rg_binary_path) }
    for _, argument in ipairs(arguments or {}) do
        table.insert(quoted_arguments, quote_argument(argument))
    end
    return table.concat(quoted_arguments, " ") .. " 2>&1"
end

--[[
调用 ripgrep，并优先使用宿主暴露的 `vulcan.process.exec`，缺失时回退到 `io.popen`。
Execute ripgrep, preferring the host-provided `vulcan.process.exec` and falling back to `io.popen` when unavailable.

参数 / Parameters:
- rg_binary_path(string): `rg` 可执行文件完整路径 / Full path to the `rg` executable.
- arguments(table): 传给 `rg` 的参数数组 / Argument array passed to `rg`.

返回 / Returns:
- string|nil: 标准输出文本 / Standard output text.
- string|nil: 标准错误文本 / Standard error text.
- table|nil: 执行失败时的结构化错误对象 / Structured error object on failure.
]]
local function run_rg_command(rg_binary_path, arguments)
    local host_exec = get_host_exec_function()
    if type(host_exec) == "function" then
        local ok, result = pcall(host_exec, {
            program = rg_binary_path,
            args = arguments,
            timeout_ms = RG_TIMEOUT_MS,
        })
        if ok and type(result) == "table" then
            -- ripgrep 退出码 1 表示“无匹配”，不是执行失败。这里显式转成空结果，避免上层把它误报成 rg_exec_failed。
            -- ripgrep exit code 1 means "no matches", not an execution failure. Convert it into an empty successful result here.
            if (not result.timed_out) and tonumber(result.code) == 1 then
                return tostring(result.stdout or ""), tostring(result.stderr or ""), nil
            end
            if result.error then
                return nil, nil, {
                    error = "rg_exec_failed",
                    message = tostring(result.error),
                    stderr = trim(result.stderr or ""),
                }
            end
            if result.timed_out then
                return nil, nil, {
                    error = "rg_timed_out",
                    message = "ripgrep execution timed out",
                }
            end
            return tostring(result.stdout or ""), tostring(result.stderr or ""), nil
        end
        if not ok then
            return nil, nil, {
                error = "rg_exec_failed",
                message = tostring(result),
            }
        end
    end

    local handle = io.popen(build_rg_command(rg_binary_path, arguments))
    if not handle then
        return nil, nil, {
            error = "rg_spawn_failed",
            message = "failed to spawn ripgrep process",
        }
    end
    local output = handle:read("*a")
    handle:close()
    return output, "", nil
end

--[[
解析 `rg --json` 的输出，只保留 `match` 事件，并按文件聚合命中行信息。
Parse `rg --json` output, keeping only `match` events and grouping line hits by file.

参数 / Parameters:
- output(string): `rg --json` 的 stdout 文本 / Stdout text from `rg --json`.
- stderr_text(string|nil): ripgrep 的 stderr 文本 / Stderr text from ripgrep.

返回 / Returns:
- table: 按文件聚合的命中结果 / Hits grouped by file.
- number: 总命中行数量 / Total matched-line count.
- table: 诊断信息数组 / Diagnostic message array.
]]
local function parse_rg_json_output(output, stderr_text)
    local hits_by_file = {}
    local total_matches = 0
    local diagnostics = {}

    for _, raw_line in ipairs(split_lines(output or "")) do
        local current = trim(raw_line)
        if current ~= "" then
            local decoded, decode_error = vulcan.json.decode(current)
            if not decoded then
                table.insert(diagnostics, "rg_json_decode_error: " .. tostring(decode_error))
            elseif decoded.type == "match" then
                local data = decoded.data or {}
                local file_path = trim(((data.path or {}).text) or "")
                local line_number = tonumber(data.line_number) or 0
                local line_text = tostring(((data.lines or {}).text) or ""):gsub("[\r\n]+$", "")
                if file_path ~= "" and line_number > 0 then
                    hits_by_file[file_path] = hits_by_file[file_path] or {}
                    table.insert(hits_by_file[file_path], {
                        line = line_number,
                        text = line_text,
                        submatches = (data.submatches or {}),
                    })
                    total_matches = total_matches + 1
                end
            elseif decoded.type == "summary" then
                local stats = ((decoded.data or {}).stats) or {}
                if tonumber(stats.matches) and tonumber(stats.matches) > total_matches then
                    total_matches = tonumber(stats.matches)
                end
            end
        end
    end

    for _, diagnostic_line in ipairs(split_lines(stderr_text or "")) do
        local normalized = trim(diagnostic_line)
        if normalized ~= "" then
            table.insert(diagnostics, normalized)
        end
    end

    return hits_by_file, total_matches, diagnostics
end

-- AST 命中归属分析 / Map ripgrep hit lines back to the most relevant AST structures.
local function attach_parent_links(symbols, parent_symbol)
    for _, symbol in ipairs(symbols or {}) do
        symbol.parent = parent_symbol
        attach_parent_links(symbol.children or {}, symbol)
    end
end

local function find_deepest_symbol_for_line(symbols, line_number)
    for _, symbol in ipairs(symbols or {}) do
        if symbol.start_line <= line_number and symbol.end_line >= line_number then
            local child_match = find_deepest_symbol_for_line(symbol.children or {}, line_number)
            return child_match or symbol
        end
    end
    return nil
end

local function is_function_like(symbol)
    return symbol and (symbol.kind == "function" or symbol.kind == "method")
end

--[[
根据命中行决定最终展示目标。
若命中的是声明起始行，则展示声明对应结构；否则优先回退到最近的函数/方法结构。
Decide the final display target from a matched line.
If the hit lands on a declaration start line, show that declaration’s structure; otherwise prefer the nearest enclosing function/method.

参数 / Parameters:
- matched_symbol(table|nil): 命中行所在的最深 AST 结构 / Deepest AST symbol containing the matched line.
- line_number(number): 当前命中行号 / Current matched line number.

返回 / Returns:
- table|nil: 最终应展示的结构节点 / Final structure node to display.
- string: 命中模式标记，取值如 `declaration` 或 `body`。
  Hit mode marker such as `declaration` or `body`.
]]
local function resolve_display_symbol(matched_symbol, line_number)
    local cursor = matched_symbol
    while cursor do
        if cursor.start_line == line_number then
            return cursor, "declaration"
        end
        cursor = cursor.parent
    end

    cursor = matched_symbol
    while cursor do
        if is_function_like(cursor) then
            return cursor, "body"
        end
        cursor = cursor.parent
    end

    return matched_symbol, "body"
end

local function mark_symbol_chain(symbol)
    local cursor = symbol
    while cursor do
        cursor.__vmcp_rg_include = true
        cursor = cursor.parent
    end
end

local function mark_symbol_subtree(symbol)
    if not symbol then
        return
    end
    symbol.__vmcp_rg_include = true
    for _, child in ipairs(symbol.children or {}) do
        mark_symbol_subtree(child)
    end
end

local function append_symbol_match_line(symbol, line_number, line_text)
    symbol.__vmcp_rg_line_matches = symbol.__vmcp_rg_line_matches or {}
    local dedupe_key = tostring(line_number) .. "::" .. tostring(line_text)
    symbol.__vmcp_rg_line_match_keys = symbol.__vmcp_rg_line_match_keys or {}
    if symbol.__vmcp_rg_line_match_keys[dedupe_key] then
        return
    end
    symbol.__vmcp_rg_line_match_keys[dedupe_key] = true
    table.insert(symbol.__vmcp_rg_line_matches, {
        line = line_number,
        text = line_text,
    })
end

local function clear_symbol_marks(symbols)
    for _, symbol in ipairs(symbols or {}) do
        symbol.__vmcp_rg_include = nil
        symbol.__vmcp_rg_line_matches = nil
        symbol.__vmcp_rg_line_match_keys = nil
        clear_symbol_marks(symbol.children or {})
    end
end

local function sort_symbol_match_lines(symbols)
    for _, symbol in ipairs(symbols or {}) do
        if symbol.__vmcp_rg_line_matches then
            table.sort(symbol.__vmcp_rg_line_matches, function(left, right)
                if left.line ~= right.line then
                    return left.line < right.line
                end
                return left.text < right.text
            end)
        end
        sort_symbol_match_lines(symbol.children or {})
    end
end

local function format_symbol_label(symbol)
    local signature = trim(symbol and symbol.signature or "")
    local display_text = signature ~= "" and signature or trim(string.format("%s %s", symbol and symbol.kind or "unknown", symbol and symbol.name or "unknown"))
    return string.format("%s [%s]", display_text, format_line_span(symbol and symbol.start_line, symbol and symbol.end_line))
end

--[[
把符号链格式化为单行扁平头部，使用 `@ 父结构 :: 子结构` 形式表达层级，便于模型快速感知命中上下文。
Format a symbol chain into a single flat header line using `@ parent :: child` so models can recognize the hit context without tree prefixes.

参数 / Parameters:
- symbol_chain(table): 从最外层结构到当前命中结构的符号链 / Symbol chain from the outermost structure to the current matched structure.

返回 / Returns:
- string: 扁平化后的结构头文本 / Flattened structural header text.
]]
local function format_symbol_chain_label(symbol_chain)
    local parts = {}
    for _, symbol in ipairs(symbol_chain or {}) do
        table.insert(parts, format_symbol_label(symbol))
    end
    return "@ " .. table.concat(parts, " :: ")
end

local function format_match_label(match_item)
    return string.format("L%d: %s", match_item.line, trim(match_item.text))
end

local function build_tree_prefix(branch_state, is_last)
    local parts = {}
    for _, has_more_siblings in ipairs(branch_state or {}) do
        table.insert(parts, has_more_siblings and "│  " or "   ")
    end
    table.insert(parts, is_last and "└ " or "├ ")
    return table.concat(parts, "")
end

local function append_tree_line(lines, branch_state, is_last, text)
    table.insert(lines, build_tree_prefix(branch_state, is_last) .. text)
end

local function collect_render_children(symbol)
    local render_children = {}

    local matches = symbol.__vmcp_rg_line_matches or {}
    local limit = math.min(#matches, MAX_MATCH_LINES_PER_SYMBOL)
    for index = 1, limit do
        table.insert(render_children, {
            type = "match",
            value = matches[index],
        })
    end
    if #matches > MAX_MATCH_LINES_PER_SYMBOL then
        table.insert(render_children, {
            type = "overflow",
            value = #matches - MAX_MATCH_LINES_PER_SYMBOL,
        })
    end

    for _, child in ipairs(symbol.children or {}) do
        if child.__vmcp_rg_include then
            table.insert(render_children, {
                type = "symbol",
                value = child,
            })
        end
    end
    return render_children
end

local function append_symbol_tree(lines, symbol, branch_state, is_last)
    append_tree_line(lines, branch_state, is_last, format_symbol_label(symbol))

    local child_branch_state = clone_array(branch_state or {})
    table.insert(child_branch_state, not is_last)

    local render_children = collect_render_children(symbol)
    for index, child_item in ipairs(render_children) do
        local child_is_last = index == #render_children
        if child_item.type == "symbol" then
            append_symbol_tree(lines, child_item.value, child_branch_state, child_is_last)
        elseif child_item.type == "match" then
            append_tree_line(lines, child_branch_state, child_is_last, format_match_label(child_item.value))
        else
            append_tree_line(lines, child_branch_state, child_is_last, string.format("... (%d more matched lines)", child_item.value))
        end
    end
end

--[[
根据 rg 命中行标记 AST 结构树，只保留与命中相关的祖先链、目标结构和必要子结构。
Mark the AST tree according to rg hit lines, retaining only related ancestor chains, target structures, and necessary descendant structures.

参数 / Parameters:
- symbol_roots(table): 文件级 AST 结构树根节点 / File-level AST tree roots.
- rg_hits(table): 当前文件的 rg 命中行数组 / rg matched lines for the current file.

返回 / Returns:
- boolean: 若存在可展示的相关结构则返回 true，否则返回 false。
  True when there are relevant structures to render; otherwise false.
]]
local function annotate_tree_with_rg_hits(symbol_roots, rg_hits)
    clear_symbol_marks(symbol_roots)
    attach_parent_links(symbol_roots, nil)

    local has_relevant_symbol = false
    for _, hit in ipairs(rg_hits or {}) do
        local matched_symbol = find_deepest_symbol_for_line(symbol_roots, hit.line)
        if matched_symbol then
            local display_symbol = resolve_display_symbol(matched_symbol, hit.line)
            if display_symbol then
                mark_symbol_chain(display_symbol)
                append_symbol_match_line(display_symbol, hit.line, hit.text)
                has_relevant_symbol = true
            end
        end
    end

    sort_symbol_match_lines(symbol_roots)
    return has_relevant_symbol
end

local function build_filtered_file_content(symbol_roots)
    local lines = {}

    --[[
    递归收集命中结构的扁平输出条目，只为真正承载命中行的节点生成 `@ ...` 头部。
    Recursively collect flat render entries and emit `@ ...` headers only for nodes that actually own matched lines.

    参数 / Parameters:
    - symbols(table): 当前层级的符号列表 / Symbols at the current traversal level.
    - ancestor_chain(table): 外层已命中的结构链 / Already matched outer structural chain.
    ]]
    local function collect_flat_entries(symbols, ancestor_chain)
        for _, symbol in ipairs(symbols or {}) do
            if symbol.__vmcp_rg_include then
                local current_chain = clone_array(ancestor_chain or {})
                table.insert(current_chain, symbol)

                if #(symbol.__vmcp_rg_line_matches or {}) > 0 then
                    table.insert(lines, format_symbol_chain_label(current_chain))
                    for _, match_item in ipairs(symbol.__vmcp_rg_line_matches or {}) do
                        table.insert(lines, format_match_label(match_item))
                    end
                end

                collect_flat_entries(symbol.children or {}, current_chain)
            end
        end
    end

    collect_flat_entries(symbol_roots, {})
    return table.concat(lines, "\n")
end

--[[
根据预先收集的文件上下文统一生成 rg 文件结果，固定只输出命中结构与命中行，保持结果协议单一稳定。
Build rg file results from pre-collected file contexts and always emit only matched structures plus matched lines so the response protocol stays single and stable.

参数 / Parameters:
- render_contexts(table): 每个文件的命中、符号与文件元信息 / Per-file hit, symbol, and file metadata contexts.
- helper_bundle(table): 复用的 codekit-ast-detail 助手集合 / Reused codekit-ast-detail helper bundle.

返回 / Returns:
- table: 渲染后的文件结果列表 / Rendered file result list.
- number: 文件级结果条目数量 / File-level rendered item count.
]]
local function build_rg_file_results(render_contexts, helper_bundle)
    local file_results = {}
    local total_items = 0

    for _, render_context in ipairs(render_contexts or {}) do
        if #render_context.symbols > 0 and #render_context.file_hits > 0 then
            local tree = helper_bundle.build_symbol_tree(render_context.symbols)
            local has_relevant_symbol = annotate_tree_with_rg_hits(tree, render_context.file_hits)
            if has_relevant_symbol then
                local content = build_filtered_file_content(tree)
                if trim(content) ~= "" then
                    table.insert(file_results, {
                        file = render_context.file_info.display_file or render_context.file_info.path,
                        lines = helper_bundle.get_file_line_count(render_context.file_info.path),
                        content = content,
                    })
                    total_items = total_items + 1
                end
            end
        end
    end

    table.sort(file_results, function(left, right)
        return left.file < right.file
    end)

    return file_results, total_items
end

local function render_error_lines(errors)
    local lines = {}
    for _, item in ipairs(errors or {}) do
        if type(item) == "string" then
            table.insert(lines, "- " .. item)
        elseif type(item) == "table" then
            if item.group then
                table.insert(lines, "- Group: " .. tostring(item.group))
            end
            for _, diagnostic in ipairs(item.diagnostics or {}) do
                table.insert(lines, "  - " .. tostring(diagnostic))
            end
        end
    end
    return lines
end

--[[
把 `codekit-rg` 结果渲染为 Markdown 纯文本，便于模型直接阅读并继续下一步分析。
Render the `codekit-rg` result as plain Markdown text so the model can read it directly and continue analysis.
]]
local function build_rg_markdown(result)
    local lines = {
        "# RG SUMMARY",
        string.format(
            "- files_scanned: %d | files_with_matches: %d | items_found: %d | rg_matches: %d | errors: %d",
            result.files_scanned or 0,
            result.files_with_matches or 0,
            result.items_found or 0,
            result.rg_matches or 0,
            #(result.errors or {})
        ),
    }

    local error_lines = render_error_lines(result.errors)
    if #error_lines > 0 then
        table.insert(lines, "")
        table.insert(lines, "## ERRORS")
        table.insert(lines, "")
        for _, line in ipairs(error_lines) do
            table.insert(lines, line)
        end
    end

    for index, file_result in ipairs(result.files or {}) do
        if index > 1 or #error_lines > 0 then
            table.insert(lines, "")
        end
        table.insert(
            lines,
            string.format("[%s Lines:%d]", tostring(file_result.file or "unknown"), tonumber(file_result.lines) or 0)
        )
        if trim(file_result.content or "") ~= "" then
            table.insert(lines, tostring(file_result.content))
        end
    end

    return table.concat(lines, "\n")
end

--[[
统一收尾 rg 结果；正常情况下直接返回 Markdown，超出预算时走共享 overflow 协议。
Finalize the rg result uniformly; return inline Markdown when safe, otherwise use the shared overflow protocol.

参数 / Parameters:
- full_result(table): 已完成统计与渲染内容拼装的最终结果对象 / Final result object with stats and rendered content assembled.

返回 / Returns:
- string: 完整 Markdown 正文，后续是否原样返回、截断还是分页由宿主统一决定。
  Full Markdown body; the host later decides whether it stays inline, gets truncated, or turns into a paging index.
]]
local function finalize_rg_result(full_result)
    local markdown_text = build_rg_markdown(full_result)
    return tostring(markdown_text or ""), vulcan.runtime.overflow_type.page
end

--[[
把结构化错误对象编码成稳定文本，确保工具入口最终始终返回 plain string。
Encode one structured error object into stable text so the public tool entry always returns a plain string.
]]
local function encode_codekit_error_payload(error_payload)
    if type(error_payload) == "string" then
        return error_payload, "text"
    end

    local ok, encoded = pcall(vulcan.json.encode, error_payload)
    if ok and type(encoded) == "string" and encoded ~= "" then
        return encoded, "json"
    end

    return tostring(error_payload), "text"
end

--[[
把当前入口的错误结果统一渲染成 Markdown 字符串，避免直接返回 table。
Render one Markdown string for current entry errors so the tool never returns a raw table.
]]
local function render_codekit_error_markdown(tool_title, error_payload)
    local payload_text, payload_language = encode_codekit_error_payload(error_payload)
    return table.concat({
        "# " .. tostring(tool_title or "CodeKit Error"),
        "",
        "## Status",
        "FAILED",
        "",
        "## Error",
        "```" .. tostring(payload_language or "text"),
        payload_text,
        "```",
    }, "\n")
end

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    local _, client_limit_error = initialize_rg_client_budget()
    if client_limit_error then
        return render_codekit_error_markdown("CodeKit RG Error", client_limit_error)
    end

    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return render_codekit_error_markdown("CodeKit RG Error", helper_error)
    end

    local target_directory, dir_error = validate_directory_argument(args and args.dir)
    if dir_error then
        return render_codekit_error_markdown("CodeKit RG Error", dir_error)
    end

    local rg_pattern, pattern_error = validate_rg_pattern_argument(args and args.rg_pattern)
    if pattern_error then
        return render_codekit_error_markdown("CodeKit RG Error", pattern_error)
    end

    local extension_filter, extension_error = helper_bundle.validate_extension_argument(args and args.ext)
    if extension_error then
        return render_codekit_error_markdown("CodeKit RG Error", extension_error)
    end

    local ignore_enabled, ignore_error = helper_bundle.validate_noignore_argument(args and args.noignore)
    if ignore_error then
        return render_codekit_error_markdown("CodeKit RG Error", ignore_error)
    end

    local export_md_error = validate_export_md_absence(args and args.export_md_path)
    if export_md_error then
        return render_codekit_error_markdown("CodeKit RG Error", export_md_error)
    end

    local rg_binary_path, rg_binary_error = find_rg_binary()
    if rg_binary_error then
        return render_codekit_error_markdown("CodeKit RG Error", rg_binary_error)
    end

    local rg_arguments = build_rg_arguments(target_directory, extension_filter, rg_pattern, ignore_enabled)
    local rg_stdout, rg_stderr, rg_error = run_rg_command(rg_binary_path, rg_arguments)
    if rg_error then
        return render_codekit_error_markdown("CodeKit RG Error", rg_error)
    end

    local hits_by_file, total_rg_matches, diagnostics = parse_rg_json_output(rg_stdout, rg_stderr)
    local matched_file_paths = {}
    for file_path in pairs(hits_by_file) do
        table.insert(matched_file_paths, file_path)
    end
    table.sort(matched_file_paths)

    if #matched_file_paths == 0 then
        return finalize_rg_result({
            files_scanned = 0,
            files_with_matches = 0,
            items_found = 0,
            rg_matches = 0,
            files = {},
            errors = diagnostics,
            truncated = false,
        })
    end

    local files, _, collection_errors, collection_error = helper_bundle.collect_files(matched_file_paths, false, nil, ignore_enabled)
    if collection_error then
        return render_codekit_error_markdown("CodeKit RG Error", collection_error)
    end

    local scanner_client, _, _, scanner_error = helper_bundle.find_binary()
    if not scanner_client then
        return render_codekit_error_markdown("CodeKit RG Error", {
            error = "ast_grep_ffi_not_found",
            message = "ast-grep FFI library not found in the current skill dependency root",
            details = scanner_error,
        })
    end

    local grouped_files = {}
    local aggregated_errors = clone_array(collection_errors or {})
    for _, file_info in ipairs(files or {}) do
        grouped_files[file_info.language] = grouped_files[file_info.language] or {}
        table.insert(grouped_files[file_info.language], file_info.path)
    end

    local normalized_by_file = {}
    for language_key, file_paths in pairs(grouped_files) do
        local matches, match_diagnostics = helper_bundle.run_language_scan(scanner_client, nil, language_key, file_paths)
        if match_diagnostics and #match_diagnostics > 0 then
            table.insert(aggregated_errors, { group = language_key, diagnostics = match_diagnostics })
        end
        for _, match in ipairs(matches or {}) do
            local symbol = helper_bundle.normalize_symbol(match, language_key)
            if symbol then
                normalized_by_file[symbol.file] = normalized_by_file[symbol.file] or {}
                table.insert(normalized_by_file[symbol.file], symbol)
            end
        end
    end

    local render_contexts = {}
    for _, file_info in ipairs(files or {}) do
        local file_hits = hits_by_file[file_info.path] or {}
        local symbols = helper_bundle.deduplicate_symbols(normalized_by_file[file_info.path] or {})
        table.insert(render_contexts, {
            file_info = file_info,
            file_hits = file_hits,
            symbols = symbols,
        })
    end

    local file_results, total_items = build_rg_file_results(render_contexts, helper_bundle)

    local meta = {
        files_scanned = #files,
        files_with_matches = #file_results,
        items_found = total_items,
        rg_matches = total_rg_matches,
        errors = aggregated_errors,
    }

    return finalize_rg_result({
        files_scanned = meta.files_scanned,
        files_with_matches = meta.files_with_matches,
        items_found = meta.items_found,
        rg_matches = meta.rg_matches,
        files = file_results,
        errors = meta.errors,
        truncated = false,
    })
end
