--[[
vmcp-rg
中文：先基于 ripgrep 做文本命中，再结合 vmcp-ast 的结构能力，仅输出与命中行直接相关的 AST 结构。
English: Perform ripgrep text matching first, then reuse vmcp-ast structural analysis to return only AST structures directly related to the matched lines.
]]

-- 工具常量 / Tool constants for rg execution and response shaping.
local RG_TOOL_CACHE_NAMESPACE = "vmcp-rg"
local RG_TIMEOUT_MS = 30000
local DEFAULT_TRUNCATE_CHARS = 20000
local MAX_MATCH_LINES_PER_SYMBOL = 12

-- 缓存的 vmcp-ast 助手集合 / Cached vmcp-ast helper bundle extracted from the existing skill entry.
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
中文：浅拷贝数组，避免在分页或输出拼装时直接改写原始列表。
English: Create a shallow array copy so pagination and rendering do not mutate the original list in place.

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
    return __skill_dir_ast_grep or "."
end

--[[
中文：通过 `debug.getupvalue` 从现有 `vmcp-ast` 入口中提取内部助手函数，避免复制一整套 AST 解析实现。
English: Extract internal helper functions from the existing `vmcp-ast` entry with `debug.getupvalue` to avoid duplicating the full AST parsing pipeline.

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
中文：懒加载 `vmcp-ast` 内部助手，确保 `vmcp-rg` 与现有 AST 规则、文件收集和结构归一化逻辑保持一致。
English: Lazily load internal `vmcp-ast` helpers so `vmcp-rg` stays aligned with the existing AST rules, file collection logic, and symbol normalization flow.

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
            message = ok and "vmcp-ast entry did not return a function" or tostring(ast_entry),
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
                message = "required helper missing from vmcp-ast runtime",
                helper = helper_name,
                path = ast_entry_path,
            }
        end
    end

    AST_RUNTIME_HELPERS = helpers
    return AST_RUNTIME_HELPERS, nil
end

--[[
中文：校验目录参数，要求非空字符串且必须是已存在目录；`path` 作为兼容别名一并接受。
English: Validate the directory argument. It must be a non-empty string pointing to an existing directory; `path` is also accepted as a compatibility alias.

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
中文：校验 ripgrep 正则参数，要求非空字符串；`pattern` 作为兼容别名一并接受。
English: Validate the ripgrep regex argument. It must be a non-empty string; `pattern` is accepted as a compatibility alias.

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
local function annotate_tree_with_rg_hits(symbol_roots, rg_hits)
    clear_symbol_marks(symbol_roots)
    attach_parent_links(symbol_roots, nil)

    local has_relevant_symbol = false
    for _, hit in ipairs(rg_hits or {}) do
        local matched_symbol = find_deepest_symbol_for_line(symbol_roots, hit.line)
        if matched_symbol then
            local display_symbol, hit_mode = resolve_display_symbol(matched_symbol, hit.line)
            if display_symbol then
                mark_symbol_chain(display_symbol)
                if is_function_like(display_symbol) then
                    mark_symbol_expand_source(display_symbol)
                elseif hit_mode ~= "declaration" then
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

-- 结果分页 / Result pagination helpers scoped specifically for vmcp-rg.
local function build_page_result(meta, page_files, page, total_pages, cache_id)
    local result = {
        page = page,
        total_pages = total_pages,
        has_next_page = page < total_pages,
        files_scanned = meta.files_scanned or 0,
        files_with_matches = meta.files_with_matches or 0,
        items_found = meta.items_found or 0,
        rg_matches = meta.rg_matches or 0,
        files = page_files or {},
        errors = meta.errors or {},
        truncated = total_pages > 1,
    }
    if total_pages > 1 then
        result.cache_id = cache_id
        if page < total_pages then
            result.next_page = page + 1
        end
    end
    return result
end

local function estimate_page_chars(meta, page_files)
    local ok, encoded = pcall(vulcan.json_encode, build_page_result(meta, page_files, 1, 1, "cache-placeholder"))
    if ok and type(encoded) == "string" then
        return #encoded
    end
    return 0
end

local function paginate_file_results(meta, file_results, char_limit)
    local pages = {}
    local index = 1
    while index <= #file_results do
        local page_files = {}
        local candidate_index = index
        while candidate_index <= #file_results do
            local candidate_files = clone_array(page_files)
            table.insert(candidate_files, file_results[candidate_index])
            local candidate_chars = estimate_page_chars(meta, candidate_files)
            if #page_files == 0 or candidate_chars <= char_limit then
                page_files = candidate_files
                candidate_index = candidate_index + 1
            else
                break
            end
        end
        table.insert(pages, page_files)
        index = candidate_index
    end
    if #pages == 0 then
        table.insert(pages, {})
    end
    return pages
end

local function read_cached_page(cache_id, page)
    local cached = vulcan.cache_get(RG_TOOL_CACHE_NAMESPACE, cache_id)
    if type(cached) ~= "table" then
        return nil, {
            error = "cache_not_found",
            message = "cache_id not found or expired",
            cache_id = cache_id,
        }
    end

    local pages = cached.pages or {}
    local meta = cached.meta or {}
    if page < 1 or page > #pages then
        return nil, {
            error = "page_out_of_range",
            message = "requested page is outside the cached page range",
            cache_id = cache_id,
            page = page,
            total_pages = #pages,
        }
    end
    return build_page_result(meta, pages[page] or {}, page, #pages, cache_id), nil
end

local function validate_page_argument(value)
    if value == nil then
        return 1, nil
    end
    if type(value) ~= "number" or value < 1 or value ~= math.floor(value) then
        return nil, {
            error = "invalid_page_argument",
            message = "page must be a positive integer when provided",
            actual_type = type(value),
        }
    end
    return value, nil
end

local function validate_cache_id_argument(value)
    if value == nil then
        return nil, nil
    end
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_cache_id_argument",
            message = "cache_id must be a non-empty string when provided",
            actual_type = type(value),
        }
    end
    return trim(value), nil
end

local function validate_truncate_chars_argument(value)
    if value == nil then
        return DEFAULT_TRUNCATE_CHARS, nil
    end
    if type(value) ~= "number" or value < 1000 or value ~= math.floor(value) then
        return nil, {
            error = "invalid_truncate_chars_argument",
            message = "truncate_chars must be an integer greater than or equal to 1000 when provided",
            actual_type = type(value),
        }
    end
    return value, nil
end

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    local requested_page, page_error = validate_page_argument(args and args.page)
    if page_error then
        return page_error
    end

    local cache_id, cache_id_error = validate_cache_id_argument(args and args.cache_id)
    if cache_id_error then
        return cache_id_error
    end

    local truncate_chars, truncate_chars_error = validate_truncate_chars_argument(args and args.truncate_chars)
    if truncate_chars_error then
        return truncate_chars_error
    end

    if cache_id then
        if args and ((args.dir or args.path) ~= nil or args.ext ~= nil or (args.rg_pattern or args.pattern) ~= nil) then
            return {
                error = "cache_request_with_search_arguments",
                message = "cache_id requests must not include fresh search arguments",
                cache_id = cache_id,
            }
        end
        local cached_page, cached_error = read_cached_page(cache_id, requested_page)
        if cached_error then
            return cached_error
        end
        return cached_page
    end

    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return helper_error
    end

    local target_directory, dir_error = validate_directory_argument((args and args.dir) or (args and args.path))
    if dir_error then
        return dir_error
    end

    local rg_pattern, pattern_error = validate_rg_pattern_argument((args and args.rg_pattern) or (args and args.pattern))
    if pattern_error then
        return pattern_error
    end

    local extension_filter, extension_error = helper_bundle.validate_extension_argument(args and args.ext)
    if extension_error then
        return extension_error
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
        return {
            page = 1,
            total_pages = 1,
            has_next_page = false,
            files_scanned = 0,
            files_with_matches = 0,
            items_found = 0,
            rg_matches = 0,
            files = {},
            errors = diagnostics,
            truncated = false,
        }
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
            local has_relevant_symbol = annotate_tree_with_rg_hits(tree, file_hits)
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

    local pages = paginate_file_results(meta, file_results, truncate_chars)
    if #pages <= 1 then
        return build_page_result(meta, file_results, 1, 1, nil)
    end

    local generated_cache_id = vulcan.cache_put(RG_TOOL_CACHE_NAMESPACE, {
        meta = meta,
        pages = pages,
        truncate_chars = truncate_chars,
    })
    return build_page_result(meta, pages[1] or {}, 1, #pages, generated_cache_id)
end
