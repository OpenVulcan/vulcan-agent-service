--[[
codekit-ast-tree
中文：对目录执行轻量级 AST 树索引，只返回按目录分组的 Markdown 文本摘要，帮助 AI 先判断文件范围，再决定后续精确读取哪些文件。
English: Build a lightweight AST tree index for directories and return only a directory-grouped Markdown summary, helping the AI decide which files deserve detailed follow-up reads.
]]

local MAX_LISTED_CONTAINERS = 3
local AST_RUNTIME_HELPERS = nil
local DEFAULT_AST_CLIENT_CHAR_LIMIT = 10000
local CURRENT_AST_CLIENT_CHAR_LIMIT = DEFAULT_AST_CLIENT_CHAR_LIMIT
local LARGE_RESULT_NOTICE_TEMPLATE = "If this MCP response is truncated by a client-side length limit, the complete codekit-ast-tree result has already been written to %s. Open that file directly."
local LFS_MODULE = nil

local TYPE_LIKE_KINDS = {
    class = true,
    contract = true,
    enum = true,
    interface = true,
    library = true,
    module = true,
    namespace = true,
    object = true,
    protocol = true,
    struct = true,
    trait = true,
    type = true,
}

--[[
中文：去除字符串首尾空白，保证目录摘要字段拼接稳定。
English: Trim leading and trailing whitespace so summary fields remain stable during formatting.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
中文：判断字符串是否以前缀开头，供客户端名识别与路径处理复用。
English: Check whether a string starts with a prefix so client-name detection and path handling can reuse the helper.
]]
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
中文：懒加载 LuaFileSystem，优先用于目录创建，降低对外部命令执行的依赖。
English: Lazily load LuaFileSystem and prefer it for directory creation to reduce reliance on external command execution.
]]
local function get_lfs_module()
    if LFS_MODULE ~= nil then
        return LFS_MODULE or nil
    end

    local ok, lfs = pcall(require, "lfs")
    if ok then
        LFS_MODULE = lfs
    else
        LFS_MODULE = false
    end
    return LFS_MODULE or nil
end

--[[
中文：获取当前技能目录，优先使用宿主为新工具注入的目录变量，并兼容旧名称回退。
English: Resolve the current skill directory, preferring the host-injected variable for this tool while keeping backward-compatible fallbacks.
]]
local function get_skill_dir()
    return __skill_dir_codekit_ast_tree or __skill_dir_codekit_ast or __skill_dir_ast_grep or "."
end

--[[
中文：按名称提取 Lua 闭包中的 upvalue，便于从 `codekit-ast` 复用内部助手函数。
English: Extract a closure upvalue by name so internal helpers from `codekit-ast` can be reused safely.
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
中文：懒加载 `codekit-ast` 运行时助手，使树索引工具与主 AST 工具共享同一套校验、收集与扫描逻辑。
English: Lazily load runtime helpers from `codekit-ast` so the tree index tool shares the same validation, collection, and scan behavior.
]]
local function load_ast_runtime_helpers()
    if AST_RUNTIME_HELPERS then
        return AST_RUNTIME_HELPERS, nil
    end

    local ast_entry_path = vulcan.path_join(get_skill_dir(), "main.lua")
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
            message = ok and "codekit-ast entry did not return a function" or tostring(ast_entry),
            path = ast_entry_path,
        }
    end

    local helpers = {
        validate_path_argument = extract_upvalue_by_name(ast_entry, "validate_path_argument"),
        validate_noignore_argument = extract_upvalue_by_name(ast_entry, "validate_noignore_argument"),
        validate_extension_argument = extract_upvalue_by_name(ast_entry, "validate_extension_argument"),
        classify_target_path_modes = extract_upvalue_by_name(ast_entry, "classify_target_path_modes"),
        find_binary = extract_upvalue_by_name(ast_entry, "find_binary"),
        collect_files = extract_upvalue_by_name(ast_entry, "collect_files"),
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
中文：从当前请求上下文中解析 MCP 客户端名称，以便沿用与主 AST 工具一致的字符预算策略。
English: Resolve the MCP client name from the current request context so the same character-budget policy as the main AST tool can be reused.
]]
local function resolve_current_client_name()
    local vulcan_context = type(vulcan) == "table" and vulcan or nil
    if not vulcan_context then
        return nil
    end

    local client_info = vulcan_context.client_info
    if type(client_info) ~= "table" and type(vulcan_context.context) == "table" then
        client_info = vulcan_context.context.client_info
    end
    if type(client_info) ~= "table" then
        return nil
    end

    local client_name = trim(client_info.name or "")
    if client_name == "" then
        return nil
    end
    return client_name
end

--[[
中文：根据客户端名称计算 AST 文本预算，规则与主 `codekit-ast` 保持一致。
English: Resolve the AST text budget from the client name, keeping the same rules as the main `codekit-ast` tool.
]]
local function resolve_ast_client_char_limit(client_name)
    local normalized_name = trim(client_name or ""):lower()
    if normalized_name == "" then
        return DEFAULT_AST_CLIENT_CHAR_LIMIT
    end
    if starts_with(normalized_name, "qwen-code-mcp-client") or normalized_name:find("qwen", 1, true) then
        return 25000
    end
    if normalized_name == "codex-mcp-client" then
        return 10000
    end
    if normalized_name:find("opencode", 1, true) then
        return 50000
    end
    if normalized_name:find("claude-code", 1, true) then
        return 50000
    end
    return DEFAULT_AST_CLIENT_CHAR_LIMIT
end

--[[
中文：在每次工具调用开始时初始化当前客户端的 AST 字符预算。
English: Initialize the AST character budget for the current client at the start of each tool call.
]]
local function initialize_ast_client_char_limit()
    CURRENT_AST_CLIENT_CHAR_LIMIT = resolve_ast_client_char_limit(resolve_current_client_name())
    return CURRENT_AST_CLIENT_CHAR_LIMIT
end

--[[
中文：在必要时按段创建目录，供超限结果落盘复用。
English: Create directories segment by segment when needed so oversized-result spilling can reuse the helper.
]]
local function ensure_directory(directory_path)
    local normalized = trim(directory_path or "")
    if normalized == "" or vulcan.fs_exists(normalized) then
        return true, nil
    end

    local lfs = get_lfs_module()
    local separator = package.config and package.config:sub(1, 1) or "\\"
    local current = ""

    if normalized:match("^%a:[/\\]") then
        current = normalized:sub(1, 2) .. separator
        normalized = normalized:sub(4)
    elseif starts_with(normalized, "\\\\") then
        current = "\\\\"
        normalized = normalized:sub(3)
    elseif starts_with(normalized, "/") then
        current = "/"
        normalized = normalized:sub(2)
    end

    for segment in normalized:gmatch("[^/\\]+") do
        if current == "" or current == "/" or current == "\\\\" or current:match("^[A-Za-z]:[\\/]?$") then
            current = current .. segment
        else
            current = current .. separator .. segment
        end

        if not vulcan.fs_exists(current) then
            if lfs and type(lfs.mkdir) == "function" then
                local ok, result = pcall(lfs.mkdir, current)
                if not ok and not vulcan.fs_exists(current) then
                    return nil, {
                        error = "ensure_directory_failed",
                        message = tostring(result),
                        path = current,
                    }
                end
            else
                local command
                if vulcan.osinfo().os == "windows" then
                    local quoted_path = current:gsub("'", "''")
                    command = {
                        program = "powershell",
                        args = { "-NoProfile", "-Command", "New-Item -ItemType Directory -LiteralPath '" .. quoted_path .. "' -Force | Out-Null" },
                        timeout_ms = 30000,
                    }
                else
                    command = {
                        program = "mkdir",
                        args = { "-p", current },
                        timeout_ms = 30000,
                    }
                end

                local ok, result = pcall(vulcan.exec, command)
                if not ok or (result and (result.error or tonumber(result.code or 0) ~= 0)) then
                    return nil, {
                        error = "ensure_directory_failed",
                        message = ok and tostring(result and (result.error or result.stderr or result.stdout or "failed to create directory")) or tostring(result),
                        path = current,
                    }
                end
            end
        end
    end

    return true, nil
end

--[[
中文：向文本文件写入完整结果，必要时先确保父目录已存在。
English: Write the full result into a text file, ensuring the parent directory exists first when needed.
]]
local function write_text_file(file_path, content)
    local parent_directory = tostring(file_path or ""):match("^(.*)[/\\][^/\\]+$")
    if parent_directory then
        local ensured, ensure_error = ensure_directory(parent_directory)
        if not ensured then
            return nil, ensure_error
        end
    end

    local ok, write_error = pcall(vulcan.fs_write, file_path, tostring(content or ""))
    if not ok then
        return nil, {
            error = "write_text_file_failed",
            message = tostring(write_error),
            file_path = file_path,
        }
    end
    return true, nil
end

--[[
中文：解析大结果缓存目录，统一写入宿主提供的临时目录。
English: Resolve the cache directory for oversized results, always using the host-provided temporary directory.
]]
local function resolve_large_result_directory()
    local temp_root = trim(vulcan.temp_dir or "")
    if temp_root == "" then
        return nil, {
            error = "temp_dir_unavailable",
            message = "vulcan.temp_dir is unavailable; cannot spill large outputs",
        }
    end
    return vulcan.path_join(temp_root, "mcp", "cache"), nil
end

--[[
中文：生成结果落盘文件名，避免不同调用之间互相覆盖。
English: Build a spill-file identifier so different invocations do not overwrite each other.
]]
local function build_spill_file_id(prefix)
    return string.format("%s_%d_%06d", tostring(prefix or "result"), os.time(), math.floor((os.clock() % 1) * 1000000))
end

--[[
中文：构造超限时的缓存提示文本，把缓存绝对路径放在最前面，降低客户端截断时丢失关键信息的风险。
English: Build the oversize-result notice, placing the absolute cache path first so key information survives client truncation more reliably.
]]
local function build_large_result_notice(full_output_path)
    return string.format("> " .. LARGE_RESULT_NOTICE_TEMPLATE, full_output_path)
end

--[[
中文：把起止行号压缩为 `Lx` 或 `Lx-y` 形式，保持与主 AST 工具的行号表达风格一致。
English: Compress start/end line numbers into the `Lx` or `Lx-y` form so the line-span style stays aligned with the main AST tool.
]]
local function format_line_span(start_line, end_line)
    local normalized_start = tonumber(start_line) or 0
    local normalized_end = tonumber(end_line) or normalized_start
    if normalized_start <= 0 then
        return "L?"
    end
    if normalized_end <= normalized_start then
        return string.format("L%d", normalized_start)
    end
    return string.format("L%d-%d", normalized_start, normalized_end)
end

--[[
中文：为排序构造统一路径键，Windows 下按不区分大小写处理。
English: Build a normalized path key for sorting, handling Windows paths case-insensitively.
]]
local function normalize_sort_key(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    if vulcan.osinfo().os == "windows" then
        normalized = normalized:lower()
    end
    return normalized
end

--[[
中文：从完整文件路径中提取父目录路径，若无法拆分则回退到当前目录标记。
English: Extract the parent directory from a full file path and fall back to the current-directory marker when splitting fails.
]]
local function get_parent_directory(path)
    local parent = tostring(path or ""):match("^(.*)[/\\][^/\\]+$")
    return parent and parent ~= "" and parent or "."
end

--[[
中文：从完整文件路径中提取基础文件名，用于目录分组下的单行摘要展示。
English: Extract the basename from a full file path for one-line summaries inside each directory group.
]]
local function get_file_name(path)
    local name = tostring(path or ""):match("([^/\\]+)$")
    return name and name ~= "" and name or tostring(path or "")
end

--[[
中文：递归统计类型或 impl 节点下的方法数量，用于构造 `m` 指标。
English: Recursively count methods beneath a type or impl node so the `m` metric can be produced.
]]
local function count_descendant_methods(nodes)
    local total = 0
    for _, node in ipairs(nodes or {}) do
        local kind = tostring(node.kind or "")
        if kind == "method" or kind == "function" then
            total = total + 1
        end
        total = total + count_descendant_methods(node.children or {})
    end
    return total
end

--[[
中文：判断节点类型是否属于稳定可输出的顶级类型级结构。
English: Determine whether a node kind belongs to the stable top-level type-like structures worth surfacing.
]]
local function is_type_like_kind(kind)
    return TYPE_LIKE_KINDS[tostring(kind or "")] == true
end

--[[
中文：从符号头部提炼更紧凑的容器名称，尽量去掉可见性关键字与泛型尾部噪音。
English: Derive a more compact container name from the symbol header, removing visibility keywords and noisy generic tails when possible.
]]
local function resolve_container_name(kind, node)
    local candidate = trim(node.name or "")
    if candidate == "" or candidate == "unknown" then
        candidate = trim(node.header or "")
    end

    candidate = candidate:gsub("^pub%s+", "")

    local keyword = tostring(kind or "")
    local escaped_keyword = keyword:gsub("([^%w])", "%%%1")
    local matched = candidate:match("%f[%a]" .. escaped_keyword .. "%s+([%w_%.:<>]+)")
    if matched and matched ~= "" then
        candidate = matched
    end

    candidate = candidate:gsub("[<{].*$", "")
    candidate = candidate:gsub("[:{%s]+$", "")
    return trim(candidate)
end

--[[
中文：将顶级类型或 impl 节点格式化为紧凑标签，并附带行号范围。
English: Format a top-level type or impl node as a compact label annotated with its line span.
]]
local function format_container_label(node, helpers)
    local kind = tostring(node.kind or "symbol")
    local name = resolve_container_name(kind, node)

    local label = kind
    if name ~= "" and name ~= "unknown" then
        label = label .. " " .. name
    end

    return string.format("%s@%s", label, format_line_span(node.start_line, node.end_line))
end

--[[
中文：统计文件级指标，并提取少量顶级类型/impl 标签用于后续展示。
English: Compute file-level metrics and extract a small set of top-level type/impl labels for later rendering.
]]
local function summarize_tree_metrics(root_nodes)
    local type_count = 0
    local impl_count = 0
    local free_function_count = 0
    local method_count = 0
    local containers = {}

    for _, node in ipairs(root_nodes or {}) do
        local kind = tostring(node.kind or "")
        if is_type_like_kind(kind) then
            type_count = type_count + 1
            method_count = method_count + count_descendant_methods(node.children or {})
            table.insert(containers, node)
        elseif kind == "impl" then
            impl_count = impl_count + 1
            method_count = method_count + count_descendant_methods(node.children or {})
            table.insert(containers, node)
        elseif kind == "function" or kind == "method" then
            free_function_count = free_function_count + 1
        end
    end

    table.sort(containers, function(left, right)
        if tonumber(left.start_line) ~= tonumber(right.start_line) then
            return (tonumber(left.start_line) or 0) < (tonumber(right.start_line) or 0)
        end
        return tostring(left.name or "") < tostring(right.name or "")
    end)

    return type_count, impl_count, free_function_count, method_count, containers
end

--[[
中文：把文件级指标压缩为单个方括号字段，仅保留非零项以减少无效字符。
English: Compress file-level metrics into one bracketed field and keep only non-zero entries to reduce noise.
]]
local function build_metric_text(line_count, type_count, impl_count, free_function_count, method_count)
    local parts = {
        string.format("l:%d", tonumber(line_count) or 0),
    }

    if tonumber(type_count) and type_count > 0 then
        table.insert(parts, string.format("t:%d", type_count))
    end
    if tonumber(impl_count) and impl_count > 0 then
        table.insert(parts, string.format("i:%d", impl_count))
    end
    if tonumber(free_function_count) and free_function_count > 0 then
        table.insert(parts, string.format("f:%d", free_function_count))
    end
    if tonumber(method_count) and method_count > 0 then
        table.insert(parts, string.format("m:%d", method_count))
    end

    return string.format("[%s]", table.concat(parts, "|"))
end

--[[
中文：把少量顶级类型/impl 标签拼成紧凑摘要，并在超出上限时附加 `+N` 提示。
English: Join a few top-level type/impl labels into a compact summary and append `+N` when more entries are omitted.
]]
local function build_container_text(containers, helpers)
    if #(containers or {}) == 0 then
        return ""
    end

    local labels = {}
    local visible_count = math.min(#containers, MAX_LISTED_CONTAINERS)
    for index = 1, visible_count do
        table.insert(labels, format_container_label(containers[index], helpers))
    end
    if #containers > visible_count then
        table.insert(labels, string.format("+%d", #containers - visible_count))
    end

    return table.concat(labels, "; ")
end

--[[
中文：把单个文件的 AST 树压缩成一行 Markdown 列表项，兼顾目录级导航与后续精确取数。
English: Compress one file's AST tree into a single Markdown bullet line for directory-level navigation and later precise follow-up reads.
]]
local function build_file_summary(file_path, root_nodes, helpers)
    local line_count = helpers.get_file_line_count(file_path)
    local type_count, impl_count, free_function_count, method_count, containers = summarize_tree_metrics(root_nodes or {})
    local metric_text = build_metric_text(line_count, type_count, impl_count, free_function_count, method_count)
    local container_text = build_container_text(containers, helpers)

    local summary = string.format("- %s %s", get_file_name(file_path), metric_text)
    if container_text ~= "" then
        summary = summary .. " :: " .. container_text
    end

    return {
        directory = get_parent_directory(file_path),
        file_name = get_file_name(file_path),
        sort_key = normalize_sort_key(file_path),
        summary = summary,
    }
end

--[[
中文：构建最终 Markdown 文本，按目录分组并保证组内文件稳定排序。
English: Build the final Markdown text grouped by directory while keeping file order stable inside each group.
]]
local function build_tree_content(groups_by_directory)
    local directories = {}
    for _, group in pairs(groups_by_directory or {}) do
        table.insert(directories, group)
    end

    table.sort(directories, function(left, right)
        return normalize_sort_key(left.path) < normalize_sort_key(right.path)
    end)

    if #directories == 0 then
        return "> (no source files found)"
    end

    local lines = {}
    for directory_index, group in ipairs(directories) do
        table.sort(group.items, function(left, right)
            return left.sort_key < right.sort_key
        end)

        table.insert(lines, string.format("## DIR %s [files:%d]", tostring(group.path or "."), #group.items))
        for _, item in ipairs(group.items) do
            table.insert(lines, item.summary)
        end

        if directory_index < #directories then
            table.insert(lines, "")
        end
    end

    return table.concat(lines, "\n")
end

--[[
中文：按与主 AST 工具一致的字符预算规则处理最终文本；超限时写入缓存并把缓存地址备注前置。
English: Finalize the output text with the same character-budget rule as the main AST tool; when oversized, spill to cache and prepend a cache-address notice.
]]
local function finalize_tree_content(content)
    local normalized = tostring(content or "")
    if #normalized <= CURRENT_AST_CLIENT_CHAR_LIMIT then
        return normalized
    end

    local output_directory, output_directory_error = resolve_large_result_directory()
    if output_directory_error then
        return output_directory_error
    end

    local file_id = build_spill_file_id("codekit_ast_tree")
    local full_output_path = vulcan.path_join(output_directory, file_id .. ".md")
    local _, write_error = write_text_file(full_output_path, normalized)
    if write_error then
        return write_error
    end

    return table.concat({
        build_large_result_notice(full_output_path),
        string.format("> Inline limit: %d chars", CURRENT_AST_CLIENT_CHAR_LIMIT),
        "",
        normalized,
    }, "\n")
end

--[[
中文：将文件摘要安全地插入到目录分组表中，供最终 Markdown 拼装使用。
English: Insert a file summary into the directory-group map so the final Markdown content can be assembled.
]]
local function append_file_summary(groups_by_directory, file_summary)
    local directory_path = tostring(file_summary.directory or ".")
    local group = groups_by_directory[directory_path]
    if not group then
        group = {
            path = directory_path,
            items = {},
        }
        groups_by_directory[directory_path] = group
    end
    table.insert(group.items, file_summary)
end

--[[
中文：把扫描过程中的非致命诊断写入日志，保持工具返回正文尽量纯净。
English: Write non-fatal scan diagnostics to logs so the main tool response can stay as clean text.
]]
local function log_diagnostics(diagnostics)
    if not diagnostics then
        return
    end

    for _, item in ipairs(diagnostics) do
        local message = item
        if type(item) == "table" then
            message = vulcan.json_encode(item)
        end
        vulcan.log("warn", "[codekit-ast-tree] " .. tostring(message))
    end
end

-- 技能入口 / Skill entry point invoked by the MCP host runtime.
return function(args)
    initialize_ast_client_char_limit()

    local helpers, helpers_error = load_ast_runtime_helpers()
    if helpers_error then
        return helpers_error
    end

    local target_paths, path_error = helpers.validate_path_argument(args and args.path)
    if path_error then
        return path_error
    end

    local extension_filter, extension_error = helpers.validate_extension_argument(args and args.ext)
    if extension_error then
        return extension_error
    end

    local ignore_enabled, ignore_error = helpers.validate_noignore_argument(args and args.noignore)
    if ignore_error then
        return ignore_error
    end

    local target_mode, target_mode_error = helpers.classify_target_path_modes(target_paths)
    if target_mode_error then
        return target_mode_error
    end
    if target_mode ~= "directory" then
        return {
            error = "directory_paths_required",
            message = "codekit-ast-tree accepts directory paths only; pass one or more directories separated by newlines",
        }
    end

    local binary_path, binary_directory, executable_name = helpers.find_binary()
    if not binary_path then
        return {
            error = "ast_grep_binary_not_found",
            expected_path = vulcan.path_join(vulcan.path_join(get_skill_dir(), ".."), "__tools", "bin", executable_name),
        }
    end

    local files, _, errors, collection_error = helpers.collect_files(target_paths, true, extension_filter, ignore_enabled)
    if collection_error then
        return collection_error
    end

    log_diagnostics(errors)

    local grouped_files = {}
    for _, file_info in ipairs(files or {}) do
        grouped_files[file_info.language] = grouped_files[file_info.language] or {}
        table.insert(grouped_files[file_info.language], file_info.path)
    end

    local normalized_by_file = {}
    for language_key, file_paths in pairs(grouped_files) do
        local matches, diagnostics = helpers.run_language_scan(binary_directory, executable_name, language_key, file_paths)
        log_diagnostics(diagnostics)
        for _, match in ipairs(matches or {}) do
            local symbol = helpers.normalize_symbol(match, language_key)
            if symbol then
                normalized_by_file[symbol.file] = normalized_by_file[symbol.file] or {}
                table.insert(normalized_by_file[symbol.file], symbol)
            end
        end
    end

    local groups_by_directory = {}
    for _, file_info in ipairs(files or {}) do
        local symbols = helpers.deduplicate_symbols(normalized_by_file[file_info.path] or {})
        local tree = (#symbols > 0) and helpers.build_symbol_tree(symbols) or {}
        append_file_summary(groups_by_directory, build_file_summary(file_info.path, tree, helpers))
    end

    return finalize_tree_content(build_tree_content(groups_by_directory))
end
