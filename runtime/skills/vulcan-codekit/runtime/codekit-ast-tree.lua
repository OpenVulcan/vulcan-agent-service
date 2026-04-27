--[[
codekit-ast-tree
对单个目录执行轻量级 AST 树索引，只返回按目录分组的 Markdown 文本摘要，帮助 AI 先判断文件范围，再决定后续精确读取哪些文件。
Build a lightweight AST tree index for a single directory and return only a directory-grouped Markdown summary, helping the AI decide which files deserve detailed follow-up reads.
]]

local MAX_LISTED_CONTAINERS = 3
local AST_RUNTIME_HELPERS = nil
local LFS_MODULE = nil
local SHARED_LENGTH_HELPERS = nil

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
去除字符串首尾空白，保证路径与摘要文本的拼接稳定。
Trim leading and trailing whitespace so path parsing and summary formatting remain stable.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
判断字符串是否以前缀开头，用于客户端名规则匹配与路径处理。
Check whether a string starts with a prefix for client-name rule matching and path handling.
]]
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
获取当前技能目录，优先使用宿主注入给 `codekit-ast-tree` 的目录变量。
Resolve the current skill directory, preferring the host-injected directory variable for `codekit-ast-tree`.
]]
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

--[[
懒加载共享预算模块，让 tree/detail/rg 复用同一套 MCP 输出/读取预算映射。
Lazily load the shared budget module so tree/detail/rg reuse the same MCP output/read budget mapping.
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
从主 `codekit-ast-detail` 闭包中按名称提取 upvalue，供目录型树工具复用底层能力。
Extract named upvalues from the main `codekit-ast-detail` closure so the directory tree tool can reuse core helpers.
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
懒加载主 `codekit-ast-detail` 的运行时助手，让本工具复用 FFI 扫描器定位、文件收集、建树与行数统计逻辑。
Lazily load runtime helpers from the main `codekit-ast-detail` tool so this tool can reuse FFI scanner lookup, file collection, tree building, and line-count logic.
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
        classify_target_path_modes = extract_upvalue_by_name(ast_entry, "classify_target_path_modes"),
        find_binary = extract_upvalue_by_name(ast_entry, "find_binary"),
        collect_files = extract_upvalue_by_name(ast_entry, "collect_files"),
        validate_extension_argument = extract_upvalue_by_name(ast_entry, "validate_extension_argument"),
        validate_noignore_argument = extract_upvalue_by_name(ast_entry, "validate_noignore_argument"),
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
把 `paths` 参数规范化为单个目录路径；虽然参数名为复数，但当前协议只允许一个目录值。
Normalize the `paths` argument into a single directory path; although the parameter name is plural, the current contract allows exactly one directory value.
]]
local function validate_paths_argument(value)
    if type(value) ~= "string" then
        return nil, {
            error = "invalid_paths_argument",
            message = "paths must be a non-empty string containing exactly one directory path",
            actual_type = type(value),
        }
    end

    local normalized_paths = {}
    local normalized = tostring(value or ""):gsub("\r\n", "\n")
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        local normalized_line = trim(line)
        if normalized_line ~= "" then
            table.insert(normalized_paths, normalized_line)
        end
    end

    if #normalized_paths == 0 then
        return nil, {
            error = "invalid_paths_argument",
            message = "paths must contain exactly one non-empty directory path",
            actual_type = "string",
        }
    end
    if #normalized_paths > 1 then
        return nil, {
            error = "multiple_directories_not_supported",
            message = "codekit-ast-tree accepts exactly one directory path; multiple directories are not supported",
            provided_paths = #normalized_paths,
        }
    end

    return normalized_paths, nil
end

--[[
显式拒绝 `comment` 参数，避免调用方误以为目录树工具仍支持备注展开。
Explicitly reject the `comment` argument so callers do not assume the directory tree tool still supports comment expansion.
]]
local function validate_comment_absence(value)
    if value ~= nil then
        return {
            error = "comment_not_supported",
            message = "codekit-ast-tree does not support the comment argument",
        }
    end
    return nil
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

--[[
在单次工具调用开始时初始化当前客户端的 AST tree 预算。
Initialize the current AST-tree budget at the start of each tool call.
]]
local function initialize_ast_client_budget()
    local helpers, helper_error = load_shared_length_helpers()
    if helper_error then
        return nil, helper_error
    end
    return helpers.initialize_client_budget(vulcan)
end

--[[
懒加载 LuaFileSystem，供超限结果落盘时创建目录使用。
Lazily load LuaFileSystem so oversized-result spilling can create directories when needed.
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
逐段创建目录，用于超限文本写入缓存时保证目标目录存在。
Create a directory path segment by segment so spill files can be written safely when text exceeds the inline limit.
]]
local function ensure_directory(directory_path)
    local normalized = trim(directory_path or "")
    if normalized == "" or vulcan.fs.exists(normalized) then
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

        if not vulcan.fs.exists(current) then
            if lfs and type(lfs.mkdir) == "function" then
                local ok, result = pcall(lfs.mkdir, current)
                if not ok and not vulcan.fs.exists(current) then
                    return nil, {
                        error = "ensure_directory_failed",
                        message = tostring(result),
                        path = current,
                    }
                end
            else
                return nil, {
                    error = "ensure_directory_failed",
                    message = "LuaFileSystem is unavailable and fallback directory creation is disabled",
                    path = current,
                }
            end
        end
    end

    return true, nil
end

--[[
把起止行号压缩为 `Lx` 或 `Lx-y` 形式，保持与主 AST 工具的行号表达风格一致。
Compress start/end line numbers into the `Lx` or `Lx-y` form so the line-span style stays aligned with the main AST tool.
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
为排序构造统一路径键，Windows 下按不区分大小写处理。
Build a normalized path key for sorting, handling Windows paths case-insensitively.
]]
local function normalize_sort_key(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    if vulcan.os.info().os == "windows" then
        normalized = normalized:lower()
    end
    return normalized
end

--[[
从完整文件路径中提取父目录路径，若无法拆分则回退到当前目录标记。
Extract the parent directory from a full file path and fall back to the current-directory marker when splitting fails.
]]
local function get_parent_directory(path)
    local parent = tostring(path or ""):match("^(.*)[/\\][^/\\]+$")
    return parent and parent ~= "" and parent or "."
end

--[[
从完整文件路径中提取基础文件名，用于目录分组下的单行摘要展示。
Extract the basename from a full file path for one-line summaries inside each directory group.
]]
local function get_file_name(path)
    local name = tostring(path or ""):match("([^/\\]+)$")
    return name and name ~= "" and name or tostring(path or "")
end

--[[
递归统计类型或 impl 节点下的方法数量，用于构造 `m` 指标。
Recursively count methods beneath a type or impl node so the `m` metric can be produced.
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
判断节点类型是否属于稳定可输出的顶级类型级结构。
Determine whether a node kind belongs to the stable top-level type-like structures worth surfacing.
]]
local function is_type_like_kind(kind)
    return TYPE_LIKE_KINDS[tostring(kind or "")] == true
end

--[[
从符号头部提炼更紧凑的容器名称，尽量去掉可见性关键字与泛型尾部噪音。
Derive a more compact container name from the symbol header, removing visibility keywords and noisy generic tails when possible.
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
将顶级类型或 impl 节点格式化为紧凑标签，并附带行号范围。
Format a top-level type or impl node as a compact label annotated with its line span.
]]
local function format_container_label(node)
    local kind = tostring(node.kind or "symbol")
    local name = resolve_container_name(kind, node)

    local label = kind
    if name ~= "" and name ~= "unknown" then
        label = label .. " " .. name
    end

    return string.format("%s@%s", label, format_line_span(node.start_line, node.end_line))
end

--[[
统计文件级指标，并提取少量顶级类型/impl 标签用于后续展示。
Compute file-level metrics and extract a small set of top-level type/impl labels for later rendering.
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
把文件级指标压缩为单个方括号字段，仅保留非零项以减少无效字符。
Compress file-level metrics into one bracketed field and keep only non-zero entries to reduce noise.
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
把少量顶级类型/impl 标签拼成紧凑摘要，并在超出上限时附加 `+N` 提示。
Join a few top-level type/impl labels into a compact summary and append `+N` when more entries are omitted.
]]
local function build_container_text(containers)
    if #(containers or {}) == 0 then
        return ""
    end

    local labels = {}
    local visible_count = math.min(#containers, MAX_LISTED_CONTAINERS)
    for index = 1, visible_count do
        table.insert(labels, format_container_label(containers[index]))
    end
    if #containers > visible_count then
        table.insert(labels, string.format("+%d", #containers - visible_count))
    end

    return table.concat(labels, "; ")
end

--[[
把单个文件的 AST 树压缩成一行 Markdown 列表项，兼顾目录级导航与后续精确取数。
Compress one file's AST tree into a single Markdown bullet line for directory-level navigation and later precise follow-up reads.
]]
local function build_file_summary(file_path, root_nodes, helpers)
    local line_count = helpers.get_file_line_count(file_path)
    local type_count, impl_count, free_function_count, method_count, containers = summarize_tree_metrics(root_nodes or {})
    local metric_text = build_metric_text(line_count, type_count, impl_count, free_function_count, method_count)
    local container_text = build_container_text(containers)

    local summary = string.format("- %s %s", get_file_name(file_path), metric_text)
    if container_text ~= "" then
        summary = summary .. " :: " .. container_text
    end

    return {
        directory = get_parent_directory(file_path),
        sort_key = normalize_sort_key(file_path),
        summary = summary,
        item_count = #root_nodes,
    }
end

--[[
将文件摘要安全地插入到目录分组表中，供最终 Markdown 拼装使用。
Insert a file summary into the directory-group map so the final Markdown content can be assembled.
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
构建最终 Markdown 文本，先给出总览摘要，再按目录分组输出文件摘要。
Build the final Markdown text by emitting a summary first and then directory-grouped file summaries.
]]
local function build_tree_content(groups_by_directory, files_scanned, files_with_symbols, items_found)
    local directories = {}
    for _, group in pairs(groups_by_directory or {}) do
        table.insert(directories, group)
    end

    table.sort(directories, function(left, right)
        return normalize_sort_key(left.path) < normalize_sort_key(right.path)
    end)

    local lines = {
        "# AST TREE SUMMARY",
        string.format("- files_scanned: %d", tonumber(files_scanned) or 0),
        string.format("- files_with_symbols: %d", tonumber(files_with_symbols) or 0),
        string.format("- items_found: %d", tonumber(items_found) or 0),
    }

    if #directories == 0 then
        table.insert(lines, "")
        table.insert(lines, "> (no source files found)")
        return table.concat(lines, "\n")
    end

    table.insert(lines, "")
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
把扫描过程中的非致命诊断写入日志，保持工具返回正文尽量纯净。
Write non-fatal scan diagnostics to logs so the main tool response can stay as clean text.
]]
local function log_diagnostics(diagnostics)
    if not diagnostics then
        return
    end

    for _, item in ipairs(diagnostics) do
        local message = item
        if type(item) == "table" then
            message = vulcan.json.encode(item)
        end
        vulcan.runtime.log("warn", "[codekit-ast-tree] " .. tostring(message))
    end
end

--[[
完成 tree 文本输出；是否原样返回还是分页改由宿主统一决定。
Finalize the tree body; whether it stays inline or becomes paged is now decided by the host.
]]
local function finalize_tree_content(content, summary_lines)
    return tostring(content or ""), vulcan.runtime.overflow_type.page
end

-- 技能入口 / Skill entry point invoked by the MCP host runtime.
return function(args)
    local _, client_limit_error = initialize_ast_client_budget()
    if client_limit_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", client_limit_error)
    end

    local helpers, helpers_error = load_ast_runtime_helpers()
    if helpers_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", helpers_error)
    end

    local target_paths, paths_error = validate_paths_argument(args and args.paths)
    if paths_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", paths_error)
    end

    local comment_error = validate_comment_absence(args and args.comment)
    if comment_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", comment_error)
    end

    local extension_filter, extension_error = helpers.validate_extension_argument(args and args.ext)
    if extension_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", extension_error)
    end

    local ignore_enabled, ignore_error = helpers.validate_noignore_argument(args and args.noignore)
    if ignore_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", ignore_error)
    end

    local target_mode, target_mode_error = helpers.classify_target_path_modes(target_paths)
    if target_mode_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", target_mode_error)
    end
    if target_mode ~= "directory" then
        return render_codekit_error_markdown("CodeKit AST Tree Error", {
            error = "single_directory_required",
            message = "codekit-ast-tree accepts exactly one directory path; file paths and multiple directories are not supported",
        })
    end

    local scanner_client, _, _, scanner_error = helpers.find_binary()
    if not scanner_client then
        return render_codekit_error_markdown("CodeKit AST Tree Error", {
            error = "ast_grep_ffi_not_found",
            message = "ast-grep FFI library was not found or could not be loaded",
            details = scanner_error,
        })
    end

    local files, _, errors, collection_error = helpers.collect_files(target_paths, true, extension_filter, ignore_enabled)
    if collection_error then
        return render_codekit_error_markdown("CodeKit AST Tree Error", collection_error)
    end

    log_diagnostics(errors)

    local grouped_files = {}
    for _, file_info in ipairs(files or {}) do
        grouped_files[file_info.language] = grouped_files[file_info.language] or {}
        table.insert(grouped_files[file_info.language], file_info.path)
    end

    local normalized_by_file = {}
    for language_key, file_paths in pairs(grouped_files) do
        local matches, diagnostics = helpers.run_language_scan(scanner_client, nil, language_key, file_paths)
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
    local files_with_symbols = 0
    local items_found = 0
    for _, file_info in ipairs(files or {}) do
        local symbols = helpers.deduplicate_symbols(normalized_by_file[file_info.path] or {})
        local tree = (#symbols > 0) and helpers.build_symbol_tree(symbols) or {}
        if #symbols > 0 then
            files_with_symbols = files_with_symbols + 1
            items_found = items_found + #symbols
        end
        append_file_summary(groups_by_directory, build_file_summary(file_info.path, tree, helpers))
    end

    return finalize_tree_content(
        build_tree_content(groups_by_directory, #files, files_with_symbols, items_found),
        {
            string.format("files_scanned: %d", #files),
            string.format("files_with_symbols: %d", files_with_symbols or 0),
            string.format("items_found: %d", items_found or 0),
        }
    )
end
