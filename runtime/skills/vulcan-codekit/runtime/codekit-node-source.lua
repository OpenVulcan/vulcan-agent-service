--[[
codekit-node-source
Extract full source text for one or more function or method nodes selected by structural paths.
根据 structural_path 提取一个或多个函数或方法节点的完整源码。
]]

-- Cached helper bundle loaded from codekit-patch.
-- 从 codekit-patch 懒加载并缓存的 helper 集合。
local PATCH_RUNTIME_HELPERS = nil
local DEFAULT_MAX_NODES = 20

-- Trim leading and trailing whitespace from a text value.
-- 去除文本值首尾空白。
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

-- Split file content into logical lines while normalizing newline style.
-- 将文件内容按逻辑行切分，并统一换行风格。
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

-- Extract one named upvalue from a Lua function.
-- 从 Lua 函数中提取一个指定名称的 upvalue。
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

-- Resolve the current skill directory injected by the host.
-- 解析宿主注入的当前 skill 目录。
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

-- Resolve the current entry directory injected by the host.
-- 解析宿主注入的当前 entry 目录。
local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

-- Lazily load structural-path and AST helpers from the existing patch entry.
-- 从现有 patch 入口懒加载 structural_path 与 AST helper。
local function load_patch_runtime_helpers()
    if PATCH_RUNTIME_HELPERS then
        return PATCH_RUNTIME_HELPERS, nil
    end

    local patch_entry_path = vulcan.path.join(get_entry_dir(), "codekit-patch.lua")
    local chunk, load_error = loadfile(patch_entry_path)
    if not chunk then
        return nil, {
            error = "codekit_patch_entry_load_failed",
            message = tostring(load_error),
            path = patch_entry_path,
        }
    end

    local ok, patch_entry = pcall(chunk)
    if not ok or type(patch_entry) ~= "function" then
        return nil, {
            error = "codekit_patch_entry_invalid",
            message = ok and "codekit-patch entry did not return a function" or tostring(patch_entry),
            path = patch_entry_path,
        }
    end

    local helpers = {
        load_ast_runtime_helpers = extract_upvalue_by_name(patch_entry, "load_ast_runtime_helpers"),
        validate_file_argument = extract_upvalue_by_name(patch_entry, "validate_file_argument"),
        validate_structural_path_argument = extract_upvalue_by_name(patch_entry, "validate_structural_path_argument"),
        collect_ast_for_file = extract_upvalue_by_name(patch_entry, "collect_ast_for_file"),
        find_matching_patch_targets = extract_upvalue_by_name(patch_entry, "find_matching_patch_targets"),
        build_candidate_descriptor = extract_upvalue_by_name(patch_entry, "build_candidate_descriptor"),
    }

    for helper_name, helper_value in pairs(helpers) do
        if type(helper_value) ~= "function" then
            return nil, {
                error = "codekit_patch_helper_missing",
                message = "required helper missing from codekit-patch runtime",
                helper = helper_name,
                path = patch_entry_path,
            }
        end
    end

    PATCH_RUNTIME_HELPERS = helpers
    return PATCH_RUNTIME_HELPERS, nil
end

-- Encode a structured error payload as Markdown.
-- 将结构化错误载荷编码为 Markdown。
local function encode_codekit_error_payload(error_payload)
    if type(error_payload) == "string" then
        return error_payload, "text"
    end
    local ok, encoded = pcall(vulcan.json.encode, error_payload or {})
    if ok and encoded then
        return encoded, "json"
    end
    return tostring(error_payload), "text"
end

-- Render one CodeKit node-source error as readable Markdown.
-- 将一次 CodeKit node-source 错误渲染为可读 Markdown。
local function render_node_source_error(error_payload)
    local payload_text, payload_language = encode_codekit_error_payload(error_payload)
    return table.concat({
        "# CodeKit Node Source Error",
        "",
        "```" .. payload_language,
        payload_text,
        "```",
    }, "\n")
end

-- Read a source file and return both raw text and line array.
-- 读取源码文件，并返回原始文本与行数组。
local function read_file_content(file_path)
    local ok, raw_content = pcall(vulcan.fs.read, file_path)
    if not ok then
        return nil, {
            error = "file_read_failed",
            message = tostring(raw_content),
            file = file_path,
        }
    end

    local text = tostring(raw_content or "")
    return {
        raw = text,
        lines = split_lines(text),
    }, nil
end

-- Infer a Markdown code-fence language from the target file extension.
-- 从目标文件扩展名推断 Markdown 代码块语言。
local function infer_fence_language(file_path)
    local extension = tostring(file_path or ""):match("%.([^.\\/:]+)$")
    local normalized = trim(extension or ""):lower()
    local language_by_extension = {
        js = "javascript",
        jsx = "jsx",
        ts = "typescript",
        tsx = "tsx",
        lua = "lua",
        rs = "rust",
        go = "go",
        py = "python",
        java = "java",
        kt = "kotlin",
        kts = "kotlin",
        cs = "csharp",
        cpp = "cpp",
        cc = "cpp",
        cxx = "cpp",
        hpp = "cpp",
        c = "c",
        h = "c",
        rb = "ruby",
        php = "php",
        swift = "swift",
    }
    return language_by_extension[normalized] or normalized
end

-- Extract the selected symbol source lines from the file content.
-- 从文件内容中提取所选符号的源码行。
local function extract_symbol_source(file_content, symbol)
    local start_line = tonumber(symbol and symbol.start_line) or 0
    local end_line = tonumber(symbol and symbol.end_line) or start_line
    if start_line <= 0 or end_line < start_line then
        return nil, {
            error = "invalid_symbol_range",
            message = "matched symbol does not expose a valid source line range",
            start_line = start_line,
            end_line = end_line,
        }
    end

    local extracted_lines = {}
    for line_number = start_line, end_line do
        table.insert(extracted_lines, file_content.lines[line_number] or "")
    end
    return table.concat(extracted_lines, "\n"), nil
end

-- Compute a stable lightweight source hash for node and file freshness checks.
-- 计算稳定的轻量源码哈希，用于节点和文件新鲜度检查。
local function compute_source_hash(text)
    local hash = 5381
    local source = tostring(text or "")
    for index = 1, #source do
        hash = ((hash * 131) + source:byte(index)) % 4294967296
    end
    return string.format("%08x", hash)
end

-- Validate and normalize the optional max_nodes argument.
-- 校验并规范化可选的 max_nodes 参数。
local function normalize_max_nodes(value)
    if value == nil then
        return DEFAULT_MAX_NODES
    end
    local normalized = tonumber(value)
    if not normalized or normalized < 1 then
        return DEFAULT_MAX_NODES
    end
    return math.floor(normalized)
end

-- Parse one structural_path string into one or more non-empty structural path lines.
-- 将单个 structural_path 字符串解析为一个或多个非空结构路径行。
local function parse_structural_path_lines(value)
    if type(value) ~= "string" then
        return nil, {
            error = "invalid_structural_path_argument",
            message = "structural_path must be a non-empty string or newline-separated structural path list",
            actual_type = type(value),
        }
    end

    local structural_paths = {}
    for _, line in ipairs(split_lines(value)) do
        local structural_path = trim(line)
        if structural_path ~= "" then
            table.insert(structural_paths, structural_path)
        end
    end

    if #structural_paths == 0 then
        return nil, {
            error = "invalid_structural_path_argument",
            message = "structural_path must include at least one non-empty structural path line",
        }
    end

    return structural_paths, nil
end

-- Push one parsed node request into the normalized request list.
-- 将一个解析后的节点请求写入规范化请求列表。
local function push_node_request(requests, node_index, file_path, structural_path)
    table.insert(requests, {
        request_index = #requests + 1,
        node_index = node_index,
        file = file_path,
        structural_path = structural_path,
    })
end

-- Push one invalid node request so batch execution can report it without aborting other nodes.
-- 写入一个无效节点请求，使批量执行能报告该节点错误而不终止其他节点。
local function push_node_error_request(requests, node_index, file_path, structural_path, error_payload)
    table.insert(requests, {
        request_index = #requests + 1,
        node_index = node_index,
        file = trim(file_path or ""),
        structural_path = trim(structural_path or ""),
        initial_error = error_payload,
    })
end

-- Normalize the required `nodes[]` payload into executable node requests.
-- 将必填的 `nodes[]` 载荷规范化为可执行的节点请求。
local function normalize_node_requests(args, helpers)
    local requests = {}
    local raw_nodes = args and args.nodes

    if type(raw_nodes) ~= "table" or #raw_nodes == 0 then
        return nil, {
            error = "invalid_nodes_argument",
            message = "node-source requires nodes[]; each node item must include file and structural_path fields",
            actual_type = type(raw_nodes),
        }
    end

    for node_index, node in ipairs(raw_nodes) do
        if type(node) ~= "table" then
            push_node_error_request(requests, node_index, "", "", {
                error = "invalid_nodes_argument",
                message = "nodes must be an array of objects with file and structural_path fields",
                node_index = node_index,
                actual_type = type(node),
            })
        else
            local file_path, file_error = helpers.validate_file_argument(node.file)
            if file_error then
                file_error.node_index = node_index
                push_node_error_request(requests, node_index, node.file, node.structural_path, file_error)
            else
                local structural_paths, structural_path_error = parse_structural_path_lines(node.structural_path)
                if structural_path_error then
                    structural_path_error.node_index = node_index
                    structural_path_error.file = file_path
                    push_node_error_request(requests, node_index, file_path, node.structural_path, structural_path_error)
                else
                    for _, structural_path in ipairs(structural_paths) do
                        push_node_request(requests, node_index, file_path, structural_path)
                    end
                end
            end

        end
    end

    if #requests == 0 then
        return nil, {
            error = "empty_nodes_argument",
            message = "node-source requires at least one structural_path",
        }
    end

    return requests, nil
end

-- Build a stable key for de-duplicating repeated structural paths that resolve to the same node.
-- 构造稳定键，用于去重多个 structural_path 命中的同一节点。
local function build_node_identity_key(candidate)
    return table.concat({
        tostring(candidate.file or ""),
        tostring(candidate.path or ""),
        tostring(candidate.start_line or ""),
        tostring(candidate.end_line or ""),
    }, "\t")
end

-- Render successful node source extraction results.
-- 渲染成功的节点源码提取结果集合。
local function render_node_source_result(summary, results)
    local lines = {
        "# NODE SOURCE SUMMARY",
        "",
        "- overflow_mode: `truncate`",
        string.format("- nodes_requested: `%d`", tonumber(summary.nodes_requested) or 0),
        string.format("- nodes_returned: `%d`", tonumber(summary.nodes_returned) or 0),
        string.format("- ok: `%d`", tonumber(summary.ok) or 0),
        string.format("- ambiguous: `%d`", tonumber(summary.ambiguous) or 0),
        string.format("- missing: `%d`", tonumber(summary.missing) or 0),
        string.format("- duplicate: `%d`", tonumber(summary.duplicate) or 0),
        string.format("- skipped: `%d`", tonumber(summary.skipped) or 0),
        string.format("- errors: `%d`", tonumber(summary.errors) or 0),
        string.format("- max_nodes: `%d`", tonumber(summary.max_nodes) or DEFAULT_MAX_NODES),
    }

    for index, result in ipairs(results or {}) do
        local candidate = result.candidate or {}
        local fence_language = infer_fence_language(result.file)
        table.insert(lines, "")
        table.insert(lines, string.format("## Node %d", index))
        table.insert(lines, "")
        table.insert(lines, string.format("- status: `%s`", tostring(result.status or "unknown")))
        table.insert(lines, string.format("- request_index: `%d`", tonumber(result.request_index) or 0))
        if result.node_index then
            table.insert(lines, string.format("- node_index: `%d`", tonumber(result.node_index) or 0))
        end
        table.insert(lines, string.format("- file: `%s`", tostring(result.file or "")))
        table.insert(lines, string.format("- structural_path: `%s`", tostring(result.structural_path or "")))
        if result.status == "ok" or result.status == "duplicate" then
            table.insert(lines, string.format("- path: `%s`", tostring(candidate.path or "")))
            table.insert(lines, string.format("- signature: `%s`", tostring(candidate.signature or "")))
            table.insert(lines, string.format("- lines: `L%d-%d`", tonumber(candidate.start_line) or 0, tonumber(candidate.end_line) or 0))
            if result.node_hash then
                table.insert(lines, string.format("- node_hash: `%s`", tostring(result.node_hash)))
            end
            if result.file_hash then
                table.insert(lines, string.format("- file_hash: `%s`", tostring(result.file_hash)))
            end
        end
        if result.message then
            table.insert(lines, string.format("- message: `%s`", tostring(result.message)))
        end
        if result.error then
            table.insert(lines, string.format("- error: `%s`", tostring(result.error)))
        end
        if result.duplicate_of then
            table.insert(lines, string.format("- duplicate_of: `%s`", tostring(result.duplicate_of)))
        end
        if result.candidates and #result.candidates > 0 then
            table.insert(lines, "- candidates:")
            for _, item in ipairs(result.candidates) do
                table.insert(
                    lines,
                    string.format(
                        "  - `%s` L%d-%d",
                        tostring(item.path or ""),
                        tonumber(item.start_line) or 0,
                        tonumber(item.end_line) or 0
                    )
                )
            end
        end
        if result.status == "ok" then
            table.insert(lines, "")
            table.insert(lines, "```" .. fence_language)
            table.insert(lines, tostring(result.source_text or ""))
            table.insert(lines, "```")
        end
    end

    return table.concat(lines, "\n")
end

-- Return content with the host-managed truncate overflow mode.
-- 返回内容并声明由宿主管理的 truncate 超限模式。
local function return_truncated_content(content)
    return tostring(content or ""), vulcan.runtime.overflow_type.truncate
end

-- Tool entry point invoked by the MCP runtime.
-- MCP 运行时调用的工具入口。
return function(args)
    local helpers, helpers_error = load_patch_runtime_helpers()
    if helpers_error then
        return render_node_source_error(helpers_error)
    end

    local ast_helpers, ast_helpers_error = helpers.load_ast_runtime_helpers()
    if ast_helpers_error then
        return render_node_source_error(ast_helpers_error)
    end

    local requests, requests_error = normalize_node_requests(args, helpers)
    if requests_error then
        return render_node_source_error(requests_error)
    end

    local max_nodes = normalize_max_nodes(args and args.max_nodes)
    local processed_count = 0
    local results = {}
    local summary = {
        nodes_requested = #requests,
        nodes_returned = 0,
        ok = 0,
        ambiguous = 0,
        missing = 0,
        duplicate = 0,
        skipped = 0,
        errors = 0,
        max_nodes = max_nodes,
    }
    local ast_cache_by_file = {}
    local file_cache_by_file = {}
    local seen_nodes = {}

    for _, request in ipairs(requests) do
        if processed_count >= max_nodes then
            summary.skipped = summary.skipped + 1
            table.insert(results, {
                status = "skipped",
                request_index = request.request_index,
                node_index = request.node_index,
                file = request.file,
                structural_path = request.structural_path,
                message = "request skipped because max_nodes was reached",
            })
        else
            processed_count = processed_count + 1
            if request.initial_error then
                summary.errors = summary.errors + 1
                table.insert(results, {
                    status = "error",
                    request_index = request.request_index,
                    node_index = request.node_index,
                    file = request.file,
                    structural_path = request.structural_path,
                    error = tostring(request.initial_error.error or "invalid_node_request"),
                    message = tostring(request.initial_error.message or "node request is invalid"),
                })
                goto continue
            end

            local ast_entry = ast_cache_by_file[request.file]
            if not ast_entry then
                local symbol_roots, _, ast_error = helpers.collect_ast_for_file(request.file, ast_helpers)
                ast_entry = {
                    symbol_roots = symbol_roots,
                    error = ast_error,
                }
                ast_cache_by_file[request.file] = ast_entry
            end

            if ast_entry.error then
                summary.errors = summary.errors + 1
                table.insert(results, {
                    status = "error",
                    request_index = request.request_index,
                    node_index = request.node_index,
                    file = request.file,
                    structural_path = request.structural_path,
                    error = tostring(ast_entry.error.error or "ast_analysis_failed"),
                    message = tostring(ast_entry.error.message or ast_entry.error.error or "failed to analyze file"),
                })
            else
                local matches = helpers.find_matching_patch_targets(ast_entry.symbol_roots, request.structural_path)
                if #matches == 0 then
                    summary.missing = summary.missing + 1
                    table.insert(results, {
                        status = "missing",
                        request_index = request.request_index,
                        node_index = request.node_index,
                        file = request.file,
                        structural_path = request.structural_path,
                        message = "no function or method matched the structural_path",
                    })
                elseif #matches > 1 then
                    local candidates = {}
                    for _, symbol in ipairs(matches) do
                        table.insert(candidates, helpers.build_candidate_descriptor(symbol))
                    end
                    table.sort(candidates, function(left, right)
                        if left.file ~= right.file then
                            return left.file < right.file
                        end
                        if left.path ~= right.path then
                            return left.path < right.path
                        end
                        return (left.start_line or 0) < (right.start_line or 0)
                    end)
                    summary.ambiguous = summary.ambiguous + 1
                    table.insert(results, {
                        status = "ambiguous",
                        request_index = request.request_index,
                        node_index = request.node_index,
                        file = request.file,
                        structural_path = request.structural_path,
                        message = "multiple functions or methods matched the structural_path",
                        candidates = candidates,
                    })
                else
                    local candidate = helpers.build_candidate_descriptor(matches[1])
                    local identity_key = build_node_identity_key(candidate)
                    if seen_nodes[identity_key] then
                        summary.duplicate = summary.duplicate + 1
                        table.insert(results, {
                            status = "duplicate",
                            request_index = request.request_index,
                            node_index = request.node_index,
                            file = request.file,
                            structural_path = request.structural_path,
                            candidate = candidate,
                            node_hash = seen_nodes[identity_key].node_hash,
                            file_hash = seen_nodes[identity_key].file_hash,
                            duplicate_of = seen_nodes[identity_key].request_index,
                            message = "structural_path resolved to a node that was already returned",
                        })
                    else
                        local file_content = file_cache_by_file[request.file]
                        if not file_content then
                            local read_content, read_error = read_file_content(request.file)
                            if read_error then
                                summary.errors = summary.errors + 1
                                table.insert(results, {
                                    status = "error",
                                    request_index = request.request_index,
                                    node_index = request.node_index,
                                    file = request.file,
                                    structural_path = request.structural_path,
                                    error = tostring(read_error.error or "file_read_failed"),
                                    message = tostring(read_error.message or read_error.error or "failed to read file"),
                                })
                                goto continue
                            end
                            file_content = read_content
                            file_cache_by_file[request.file] = file_content
                        end

                        local source_text, source_error = extract_symbol_source(file_content, matches[1])
                        if source_error then
                            summary.errors = summary.errors + 1
                            table.insert(results, {
                                status = "error",
                                request_index = request.request_index,
                                node_index = request.node_index,
                                file = request.file,
                                structural_path = request.structural_path,
                                candidate = candidate,
                                error = tostring(source_error.error or "source_extract_failed"),
                                message = tostring(source_error.message or source_error.error or "failed to extract node source"),
                            })
                        else
                            local node_hash = compute_source_hash(source_text)
                            local file_hash = compute_source_hash(file_content.raw)
                            summary.ok = summary.ok + 1
                            summary.nodes_returned = summary.nodes_returned + 1
                            seen_nodes[identity_key] = {
                                request_index = tostring(request.request_index),
                                node_hash = node_hash,
                                file_hash = file_hash,
                            }
                            table.insert(results, {
                                status = "ok",
                                request_index = request.request_index,
                                node_index = request.node_index,
                                file = request.file,
                                structural_path = request.structural_path,
                                candidate = candidate,
                                node_hash = node_hash,
                                file_hash = file_hash,
                                source_text = source_text,
                            })
                        end
                    end
                end
            end
        end
        ::continue::
    end

    --[[
    Build a strict result ordering key so mixed ok/error batches remain deterministic.
    构造严格的结果排序键，确保 ok/error 混合批次也保持确定性。

    Parameters:
    参数：
    - result(table): One rendered node-source result item.
    - result(table)：一个待渲染的 node-source 结果项。

    Returns:
    返回：
    - table: Comparable group/file/range/request keys.
    - table：可比较的分组、文件、范围与请求排序键。
    ]]
    local function build_result_sort_key(result)
        local candidate = result and result.candidate or {}
        local status = tostring(result and result.status or "")
        if status == "ok" or status == "duplicate" then
            return {
                group = 1,
                file = tostring(result.file or ""),
                start_line = tonumber(candidate.start_line) or 0,
                end_line = tonumber(candidate.end_line) or 0,
                request_index = tonumber(result.request_index) or 0,
            }
        end
        return {
            group = 2,
            file = "",
            start_line = 0,
            end_line = 0,
            request_index = tonumber(result and result.request_index) or 0,
        }
    end

    table.sort(results, function(left, right)
        local left_key = build_result_sort_key(left)
        local right_key = build_result_sort_key(right)
        if left_key.group ~= right_key.group then
            return left_key.group < right_key.group
        end
        if left_key.file ~= right_key.file then
            return left_key.file < right_key.file
        end
        if left_key.start_line ~= right_key.start_line then
            return left_key.start_line < right_key.start_line
        end
        if left_key.end_line ~= right_key.end_line then
            return left_key.end_line < right_key.end_line
        end
        return left_key.request_index < right_key.request_index
    end)

    return return_truncated_content(render_node_source_result(summary, results))
end
