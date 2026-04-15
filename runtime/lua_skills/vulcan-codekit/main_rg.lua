--[[
codekit-rg
中文：先基于 ripgrep 做文本命中，再结合 codekit-ast 的结构能力，仅输出与命中行直接相关的 AST 结构。
English: Perform ripgrep text matching first, then reuse codekit-ast structural analysis to return only AST structures directly related to the matched lines.
]]

-- 工具常量 / Tool constants for rg execution and response shaping.
local RG_TIMEOUT_MS = 30000
local MAX_INLINE_RESULT_BYTES = 10000
local MAX_MATCH_LINES_PER_SYMBOL = 12
local LARGE_RESULT_NOTICE_TEMPLATE = "If this MCP response is truncated by a client-side length limit, the complete result has already been written to %s. Open that file directly."
local LFS_MODULE = nil

-- 缓存的 codekit-ast 助手集合 / Cached codekit-ast helper bundle extracted from the existing skill entry.
local AST_RUNTIME_HELPERS = nil
local FILE_SOURCE_CACHE = {}

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
中文：浅拷贝数组，避免在树渲染或结果拼装时直接改写原始列表。
English: Create a shallow array copy so tree rendering and result assembly do not mutate the original list in place.

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
中文：统一格式化结构范围，输出 `Lx-y` 或 `Lx` 形式，便于结果直接定位到代码区间。
English: Format structural ranges into `Lx-y` or `Lx` so the output can be used as an immediate line anchor.

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
中文：获取当前 skill 目录，优先使用宿主注入的 `__skill_dir_ast_grep`，缺失时回退到当前目录。
English: Resolve the current skill directory. Prefer the host-injected `__skill_dir_ast_grep` and fall back to the current directory when absent.

参数 / Parameters:
- 无 / None.

返回 / Returns:
- string: 当前 skill 目录 / Current skill directory.
]]
local function get_skill_dir()
    return __skill_dir_codekit_rg or __skill_dir_ast_grep or "."
end

--[[
中文：通过 `debug.getupvalue` 从现有 `codekit-ast` 入口中提取内部助手函数，避免复制一整套 AST 解析实现。
English: Extract internal helper functions from the existing `codekit-ast` entry with `debug.getupvalue` to avoid duplicating the full AST parsing pipeline.

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
中文：懒加载 `codekit-ast` 内部助手，确保 `codekit-rg` 与现有 AST 规则、文件收集和结构归一化逻辑保持一致。
English: Lazily load internal `codekit-ast` helpers so `codekit-rg` stays aligned with the existing AST rules, file collection logic, and symbol normalization flow.

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

    local ast_entry_path = vulcan.path_join(get_skill_dir(), "main.lua")
    local chunk, load_error = loadfile(ast_entry_path)
    if not chunk then
        return nil, {
            error = "vmcp_ast_entry_load_failed",
            message = tostring(load_error),
            path = ast_entry_path,
        }
    end

    local ok, ast_entry = pcall(chunk)
    if not ok or type(ast_entry) ~= "function" then
        return nil, {
            error = "vmcp_ast_entry_invalid",
            message = ok and "codekit-ast entry did not return a function" or tostring(ast_entry),
            path = ast_entry_path,
        }
    end

    local helpers = {
        validate_extension_argument = extract_upvalue_by_name(ast_entry, "validate_extension_argument"),
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
                error = "vmcp_ast_helper_missing",
                message = "required helper missing from codekit-ast runtime",
                helper = helper_name,
                path = ast_entry_path,
            }
        end
    end

    AST_RUNTIME_HELPERS = helpers
    return AST_RUNTIME_HELPERS, nil
end

--[[
中文：校验目录参数，要求非空字符串且必须是已存在目录。
English: Validate the directory argument. It must be a non-empty string pointing to an existing directory.

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
    if not vulcan.fs_exists(normalized) then
        return nil, {
            error = "dir_not_found",
            message = "dir does not exist",
            dir = normalized,
        }
    end
    if not vulcan.fs_is_dir(normalized) then
        return nil, {
            error = "dir_must_be_directory",
            message = "dir must point to an existing directory",
            dir = normalized,
        }
    end
    return normalized, nil
end

--[[
中文：校验 ripgrep 正则参数，要求非空字符串。
English: Validate the ripgrep regex argument. It must be a non-empty string.

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

local function is_absolute_path(path)
    local normalized = tostring(path or "")
    return normalized:match("^%a:[/\\]") ~= nil
        or starts_with(normalized, "\\\\")
        or starts_with(normalized, "/")
end

--[[
中文：校验工作目录参数，要求为绝对路径且必须指向已存在目录。
English: Validate the workdir argument; it must be an absolute path pointing to an existing directory.
]]
local function validate_workdir_argument(value)
    if value == nil then
        return nil, nil
    end
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_workdir_argument",
            message = "workdir must be a non-empty absolute directory path when provided",
            actual_type = type(value),
        }
    end

    local normalized = trim(value)
    if not is_absolute_path(normalized) then
        return nil, {
            error = "invalid_workdir_argument",
            message = "workdir must be an absolute directory path",
            workdir = normalized,
        }
    end
    if not vulcan.fs_exists(normalized) or not vulcan.fs_is_dir(normalized) then
        return nil, {
            error = "workdir_not_found",
            message = "workdir must point to an existing directory",
            workdir = normalized,
        }
    end
    return normalized, nil
end

--[[
中文：校验 Markdown 导出路径，要求为绝对路径并以 `.md` 结尾。
English: Validate the Markdown export path; it must be absolute and end with `.md`.
]]
local function validate_export_md_argument(value)
    if value == nil then
        return nil, nil
    end
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must be a non-empty absolute file path when provided",
            actual_type = type(value),
        }
    end

    local normalized = trim(value)
    if not is_absolute_path(normalized) then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must be an absolute file path",
            export_md_path = normalized,
        }
    end
    if not normalized:lower():match("%.md$") then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must end with .md",
            export_md_path = normalized,
        }
    end
    return normalized, nil
end

--[[
中文：校验“是否展开完整函数源码”的布尔参数。未提供时默认为 false。
English: Validate the boolean flag that controls whether full function source should be expanded. Defaults to false when omitted.

参数 / Parameters:
- value(any): 调用方传入的原始参数值 / Raw argument value from the caller.

返回 / Returns:
- boolean: 是否开启完整函数源码显示 / Whether full function source rendering is enabled.
- table|nil: 参数非法时返回结构化错误 / Structured error when the argument is invalid.
]]
local function validate_show_full_function_argument(value)
    if value == nil then
        return false, nil
    end
    if type(value) ~= "boolean" then
        return false, {
            error = "invalid_show_full_function_argument",
            message = "show_full_function must be a boolean when provided",
        }
    end
    return value, nil
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
中文：在 LuaFileSystem 不可用时，回退到宿主 `vulcan.exec` 递归创建目录，保证大结果落盘与 Markdown 导出仍可执行。
English: Fall back to host-side `vulcan.exec` recursive directory creation when LuaFileSystem is unavailable so large-result spilling and Markdown export still work.
]]
local function ensure_directory_via_exec(directory_path)
    if type(vulcan.exec) ~= "function" then
        return false, {
            error = "directory_creation_failed",
            message = "neither LuaFileSystem nor vulcan.exec is available for directory creation",
            path = directory_path,
        }
    end

    local os_info = vulcan.osinfo()
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

    local ok, result = pcall(vulcan.exec, request)
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
    if vulcan.fs_exists(directory_path) and vulcan.fs_is_dir(directory_path) then
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
    if vulcan.fs_exists(normalized) then
        if vulcan.fs_is_dir(normalized) then
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
        if not vulcan.fs_exists(current) then
            local ok, mkdir_error = lfs.mkdir(current)
            if not ok then
                return false, {
                    error = "directory_creation_failed",
                    message = tostring(mkdir_error or "mkdir failed"),
                    path = current,
                }
            end
        elseif not vulcan.fs_is_dir(current) then
            return false, {
                error = "directory_creation_failed",
                message = "path exists but is not a directory",
                path = current,
            }
        end
    end
    return true, nil
end

local function write_text_file(file_path, content)
    local parent_directory = get_parent_directory(file_path)
    if parent_directory then
        local ensured, ensure_error = ensure_directory(parent_directory)
        if not ensured then
            return nil, ensure_error
        end
    end

    local ok, write_error = pcall(vulcan.fs_write, file_path, tostring(content or ""))
    if not ok then
        return nil, {
            error = "file_write_failed",
            message = tostring(write_error),
            file = file_path,
        }
    end
    return file_path, nil
end

local function resolve_large_result_directory(workdir)
    if workdir then
        return vulcan.path_join(workdir, ".vulcan", "mcp", "cache"), nil
    end

    local temp_root = trim(vulcan.temp_dir or "")
    if temp_root == "" then
        return nil, {
            error = "temp_dir_unavailable",
            message = "vulcan.temp_dir is unavailable; cannot spill large outputs",
        }
    end
    return vulcan.path_join(temp_root, "mcp", "cache"), nil
end

local function build_spill_file_id(prefix)
    return string.format("%s_%d_%06d", tostring(prefix or "result"), os.time(), math.floor((os.clock() % 1) * 1000000))
end

local function build_large_result_notice(full_output_path)
    return string.format(LARGE_RESULT_NOTICE_TEMPLATE, full_output_path)
end

local function shallow_copy_object(source)
    local copied = {}
    for key, value in pairs(source or {}) do
        copied[key] = value
    end
    return copied
end

--[[
中文：查找共享 `__tools/bin` 目录中的 `rg` 可执行文件，遵循与 ast-grep 相同的共享工具布局。
English: Locate the `rg` executable inside the shared `__tools/bin` directory, following the same shared-tool layout used by ast-grep.

参数 / Parameters:
- 无 / None.

返回 / Returns:
- string|nil: `rg` 可执行文件完整路径 / Full `rg` executable path.
- table|nil: 未找到时返回结构化错误对象 / Structured error object when not found.
]]
local function find_rg_binary()
    local executable_name = vulcan.osinfo().os == "windows" and "rg.exe" or "rg"
    local binary_path = vulcan.path_join(vulcan.path_join(vulcan.path_join(get_skill_dir(), ".."), "__tools"), "bin")
    local full_path = vulcan.path_join(binary_path, executable_name)
    if vulcan.fs_exists(full_path) then
        return full_path, nil
    end
    return nil, {
        error = "rg_binary_not_found",
        message = "ripgrep binary not found in shared tool directory",
        expected_path = full_path,
    }
end

-- rg 参数与输出解析 / Build rg commands and parse the newline-delimited JSON stream.
local function quote_argument(value)
    return '"' .. tostring(value or ""):gsub('"', '\\"') .. '"'
end

local function build_rg_arguments(target_directory, extension_filter, rg_pattern)
    local arguments = { "--json", "--line-number", "--color=never", "-e", rg_pattern, target_directory }
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
中文：调用 ripgrep，并优先使用宿主暴露的 `vulcan.exec`，缺失时回退到 `io.popen`。
English: Execute ripgrep, preferring the host-provided `vulcan.exec` and falling back to `io.popen` when unavailable.

参数 / Parameters:
- rg_binary_path(string): `rg` 可执行文件完整路径 / Full path to the `rg` executable.
- arguments(table): 传给 `rg` 的参数数组 / Argument array passed to `rg`.

返回 / Returns:
- string|nil: 标准输出文本 / Standard output text.
- string|nil: 标准错误文本 / Standard error text.
- table|nil: 执行失败时的结构化错误对象 / Structured error object on failure.
]]
local function run_rg_command(rg_binary_path, arguments)
    if type(vulcan.exec) == "function" then
        local ok, result = pcall(vulcan.exec, {
            program = rg_binary_path,
            args = arguments,
            timeout_ms = RG_TIMEOUT_MS,
        })
        if ok and type(result) == "table" then
            -- 中文：ripgrep 退出码 1 表示“无匹配”，不是执行失败。这里显式转成空结果，避免上层把它误报成 rg_exec_failed。
            -- English: ripgrep exit code 1 means "no matches", not an execution failure. Convert it into an empty successful result here.
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
中文：解析 `rg --json` 的输出，只保留 `match` 事件，并按文件聚合命中行信息。
English: Parse `rg --json` output, keeping only `match` events and grouping line hits by file.

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
            local decoded, decode_error = vulcan.json_decode(current)
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
中文：根据命中行决定最终展示目标。
若命中的是声明起始行，则展示声明对应结构；否则优先回退到最近的函数/方法结构。
English: Decide the final display target from a matched line.
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

--[[
中文：标记某个函数/方法节点需要展开完整源码片段，而不是只展示单条命中行。
English: Mark a function or method node so it renders its full source excerpt instead of only individual hit lines.

参数 / Parameters:
- symbol(table|nil): 需要展开源码的结构节点 / Symbol node that should render a full source excerpt.
]]
local function mark_symbol_expand_source(symbol)
    if symbol then
        symbol.__vmcp_rg_expand_source = true
    end
end

--[[
中文：按文件路径读取并缓存源码行数组，供函数源码片段渲染复用。
English: Read and cache source lines by file path so function excerpt rendering can reuse the same content.

参数 / Parameters:
- file_path(string): 目标源码文件完整路径 / Full path of the source file.

返回 / Returns:
- table|nil: 文件的逐行数组 / Line array of the file.
- string|nil: 读取失败时的错误文本 / Error text when reading fails.
]]
local function read_file_lines(file_path)
    if FILE_SOURCE_CACHE[file_path] then
        return FILE_SOURCE_CACHE[file_path], nil
    end

    local ok, content = pcall(vulcan.fs_read, file_path)
    if not ok then
        return nil, tostring(content)
    end

    local lines = split_lines(content or "")
    FILE_SOURCE_CACHE[file_path] = lines
    return lines, nil
end

--[[
中文：根据结构节点的起止行号提取完整源码片段，用于“函数命中时展示整段函数代码”的输出。
English: Extract the full source excerpt of a symbol from its line span, used when a function hit should render the whole function body.

参数 / Parameters:
- symbol(table): 结构节点，至少包含 file/start_line/end_line。
  Symbol node containing at least file/start_line/end_line.

返回 / Returns:
- table: 形如 `{ line, text }` 的源码行数组 / Source line array in `{ line, text }` shape.
]]
local function build_source_excerpt_lines(symbol)
    if not symbol or not symbol.file then
        return {}
    end

    local file_lines = read_file_lines(symbol.file)
    if not file_lines then
        return {}
    end

    local excerpt_lines = {}
    local start_line = math.max(tonumber(symbol.start_line) or 1, 1)
    local end_line = math.max(tonumber(symbol.end_line) or start_line, start_line)
    for line_number = start_line, end_line do
        local line_text = file_lines[line_number]
        if line_text ~= nil then
            table.insert(excerpt_lines, {
                line = line_number,
                text = line_text,
            })
        end
    end
    return excerpt_lines
end

local function clear_symbol_marks(symbols)
    for _, symbol in ipairs(symbols or {}) do
        symbol.__vmcp_rg_include = nil
        symbol.__vmcp_rg_expand_source = nil
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

local function format_match_label(match_item)
    return string.format("L%d | %s", match_item.line, match_item.text)
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

    local matches = symbol.__vmcp_rg_expand_source and build_source_excerpt_lines(symbol) or (symbol.__vmcp_rg_line_matches or {})
    local limit = symbol.__vmcp_rg_expand_source and #matches or math.min(#matches, MAX_MATCH_LINES_PER_SYMBOL)
    for index = 1, limit do
        table.insert(render_children, {
            type = "match",
            value = matches[index],
        })
    end
    if (not symbol.__vmcp_rg_expand_source) and #matches > MAX_MATCH_LINES_PER_SYMBOL then
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
中文：根据 rg 命中行标记 AST 结构树，只保留与命中相关的祖先链、目标结构和必要子结构。
English: Mark the AST tree according to rg hit lines, retaining only related ancestor chains, target structures, and necessary descendant structures.

参数 / Parameters:
- symbol_roots(table): 文件级 AST 结构树根节点 / File-level AST tree roots.
- rg_hits(table): 当前文件的 rg 命中行数组 / rg matched lines for the current file.

返回 / Returns:
- boolean: 若存在可展示的相关结构则返回 true，否则返回 false。
  True when there are relevant structures to render; otherwise false.
]]
local function annotate_tree_with_rg_hits(symbol_roots, rg_hits, show_full_function)
    clear_symbol_marks(symbol_roots)
    attach_parent_links(symbol_roots, nil)

    local has_relevant_symbol = false
    for _, hit in ipairs(rg_hits or {}) do
        local matched_symbol = find_deepest_symbol_for_line(symbol_roots, hit.line)
        if matched_symbol then
            local display_symbol = resolve_display_symbol(matched_symbol, hit.line)
            if display_symbol then
                mark_symbol_chain(display_symbol)
                if is_function_like(display_symbol) and show_full_function then
                    mark_symbol_expand_source(display_symbol)
                else
                    append_symbol_match_line(display_symbol, hit.line, hit.text)
                end
                has_relevant_symbol = true
            end
        end
    end

    sort_symbol_match_lines(symbol_roots)
    return has_relevant_symbol
end

local function build_filtered_file_content(symbol_roots)
    local lines = {}
    local included_roots = {}
    for _, symbol in ipairs(symbol_roots or {}) do
        if symbol.__vmcp_rg_include then
            table.insert(included_roots, symbol)
        end
    end
    for index, symbol in ipairs(included_roots) do
        append_symbol_tree(lines, symbol, {}, index == #included_roots)
    end
    return table.concat(lines, "\n")
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
中文：把 codekit-rg 结果渲染为 Markdown 文本，便于导出成可阅读的结果文件。
English: Render the codekit-rg result into Markdown text so it can be exported as a readable result file.
]]
local function build_rg_markdown(result)
    local lines = {
        "# codekit-rg Export",
        "",
        string.format("- Files scanned: %d", result.files_scanned or 0),
        string.format("- Files with matches: %d", result.files_with_matches or 0),
        string.format("- Items found: %d", result.items_found or 0),
        string.format("- rg matches: %d", result.rg_matches or 0),
    }

    local error_lines = render_error_lines(result.errors)
    if #error_lines > 0 then
        table.insert(lines, "")
        table.insert(lines, "## Errors")
        table.insert(lines, "")
        for _, line in ipairs(error_lines) do
            table.insert(lines, line)
        end
    end

    for _, file_result in ipairs(result.files or {}) do
        table.insert(lines, "")
        table.insert(lines, "## " .. tostring(file_result.file or "unknown"))
        table.insert(lines, "")
        table.insert(lines, string.format("- Lines: %d", tonumber(file_result.lines) or 0))
        table.insert(lines, "")
        table.insert(lines, "```text")
        if trim(file_result.content or "") ~= "" then
            table.insert(lines, tostring(file_result.content))
        end
        table.insert(lines, "```")
    end

    return table.concat(lines, "\n")
end

local function attach_large_result_notice(full_result, full_output_path, export_md_path, total_bytes)
    local annotated = shallow_copy_object(full_result)
    annotated["!msg"] = build_large_result_notice(full_output_path)
    annotated.full_output_file = full_output_path
    annotated.full_output_bytes = total_bytes
    if export_md_path then
        annotated.exported_markdown_path = export_md_path
    end
    return annotated
end

local function finalize_rg_result(full_result, workdir, export_md_path)
    local markdown_text = build_rg_markdown(full_result)
    if export_md_path then
        local _, export_error = write_text_file(export_md_path, markdown_text)
        if export_error then
            return export_error
        end
        -- 中文：显式导出时进入静默导出模式，仅返回导出成功提示与路径，避免同时返回结构内联结果。
        -- English: When explicit export is requested, switch to silent export mode and return only a success notice with the file path instead of inline structure output.
        return {
            exported_markdown_path = export_md_path,
            message = string.format("Markdown file has been generated at %s", export_md_path),
        }
    end

    local serializable_result = shallow_copy_object(full_result)

    local ok, encoded = pcall(vulcan.json_encode, serializable_result)
    if not ok or type(encoded) ~= "string" then
        return {
            error = "result_encoding_failed",
            message = "failed to encode codekit-rg result as JSON",
        }
    end

    if #encoded <= MAX_INLINE_RESULT_BYTES then
        serializable_result.truncated = false
        return serializable_result
    end

    local output_directory, output_directory_error = resolve_large_result_directory(workdir)
    if output_directory_error then
        return output_directory_error
    end

    local file_id = build_spill_file_id("vmcp_rg")
    local full_output_path = vulcan.path_join(output_directory, file_id .. ".md")
    local _, write_error = write_text_file(full_output_path, markdown_text)
    if write_error then
        return write_error
    end

    return attach_large_result_notice(serializable_result, full_output_path, export_md_path, #markdown_text)
end

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return helper_error
    end

    local target_directory, dir_error = validate_directory_argument(args and args.dir)
    if dir_error then
        return dir_error
    end

    local rg_pattern, pattern_error = validate_rg_pattern_argument(args and args.rg_pattern)
    if pattern_error then
        return pattern_error
    end

    local extension_filter, extension_error = helper_bundle.validate_extension_argument(args and args.ext)
    if extension_error then
        return extension_error
    end

    local workdir, workdir_error = validate_workdir_argument(args and args.workdir)
    if workdir_error then
        return workdir_error
    end

    local export_md_path, export_md_error = validate_export_md_argument(args and args.export_md_path)
    if export_md_error then
        return export_md_error
    end

    local show_full_function, show_full_function_error = validate_show_full_function_argument(args and args.show_full_function)
    if show_full_function_error then
        return show_full_function_error
    end

    local rg_binary_path, rg_binary_error = find_rg_binary()
    if rg_binary_error then
        return rg_binary_error
    end

    local rg_arguments = build_rg_arguments(target_directory, extension_filter, rg_pattern)
    local rg_stdout, rg_stderr, rg_error = run_rg_command(rg_binary_path, rg_arguments)
    if rg_error then
        return rg_error
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
        }, workdir, export_md_path)
    end

    local files, _, collection_errors, collection_error = helper_bundle.collect_files(matched_file_paths, false, nil, true)
    if collection_error then
        return collection_error
    end

    local ast_binary_path, ast_binary_directory, ast_executable_name = helper_bundle.find_binary()
    if not ast_binary_path then
        return {
            error = "ast_grep_binary_not_found",
            message = "ast-grep binary not found in shared tool directory",
        }
    end

    local grouped_files = {}
    local aggregated_errors = clone_array(collection_errors or {})
    for _, file_info in ipairs(files or {}) do
        grouped_files[file_info.language] = grouped_files[file_info.language] or {}
        table.insert(grouped_files[file_info.language], file_info.path)
    end

    local normalized_by_file = {}
    for language_key, file_paths in pairs(grouped_files) do
        local matches, match_diagnostics = helper_bundle.run_language_scan(ast_binary_directory, ast_executable_name, language_key, file_paths)
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

    local file_results = {}
    local total_items = 0
    for _, file_info in ipairs(files or {}) do
        local file_hits = hits_by_file[file_info.path] or {}
        local symbols = helper_bundle.deduplicate_symbols(normalized_by_file[file_info.path] or {})
        if #symbols > 0 and #file_hits > 0 then
            local tree = helper_bundle.build_symbol_tree(symbols)
            local has_relevant_symbol = annotate_tree_with_rg_hits(tree, file_hits, show_full_function)
            if has_relevant_symbol then
                local content = build_filtered_file_content(tree)
                if trim(content) ~= "" then
                    table.insert(file_results, {
                        file = file_info.display_file or file_info.path,
                        lines = helper_bundle.get_file_line_count(file_info.path),
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
    }, workdir, export_md_path)
end
