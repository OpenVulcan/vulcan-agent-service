--[[
vmcp-patch
中文：基于 AST 结构路径重新定位函数/方法节点，并执行整函数替换。
English: Re-locate function or method nodes by AST structural selectors and replace the full function source.
]]

-- 工具常量 / Tool constants for selector matching and replacement behavior.
local AST_RUNTIME_HELPERS = nil
local find_matching_patch_targets

-- 基础字符串工具 / Basic string helpers shared by selector matching and patch rendering.
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
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
中文：浅拷贝数组，避免在树遍历和代码拼装时直接修改原始列表。
English: Create a shallow array copy so tree traversal and code assembly do not mutate the original list in place.

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
中文：通过 `debug.getupvalue` 从现有 `vmcp-ast` 入口中提取内部 helper，避免复制整套 AST 分析实现。
English: Extract internal helpers from the existing `vmcp-ast` entry through `debug.getupvalue` so the full AST pipeline does not need to be duplicated.

参数 / Parameters:
- fn(function): 待扫描 upvalue 的函数 / Function whose upvalues will be scanned.
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
中文：获取当前 skill 目录，优先使用宿主注入的 `__skill_dir_ast_grep`。
English: Resolve the current skill directory, preferring the host-injected `__skill_dir_ast_grep`.

返回 / Returns:
- string: 当前 skill 目录 / Current skill directory.
]]
local function get_skill_dir()
    return __skill_dir_ast_grep or "."
end

--[[
中文：懒加载 `vmcp-ast` 内部 helper，确保 `vmcp-patch` 与现有 AST 规则、符号归一化和结构建树逻辑完全一致。
English: Lazily load internal `vmcp-ast` helpers so `vmcp-patch` remains fully aligned with the existing AST rules, symbol normalization, and tree-building logic.

返回 / Returns:
- table|nil: helper 函数集合 / Helper bundle on success.
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
        collect_files = extract_upvalue_by_name(ast_entry, "collect_files"),
        find_binary = extract_upvalue_by_name(ast_entry, "find_binary"),
        run_language_scan = extract_upvalue_by_name(ast_entry, "run_language_scan"),
        normalize_symbol = extract_upvalue_by_name(ast_entry, "normalize_symbol"),
        deduplicate_symbols = extract_upvalue_by_name(ast_entry, "deduplicate_symbols"),
        build_symbol_tree = extract_upvalue_by_name(ast_entry, "build_symbol_tree"),
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
中文：校验目标文件参数，要求为存在的单个文件路径。
English: Validate the target file argument. It must be a single existing file path.
]]
local function validate_file_argument(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_file_argument",
            message = "file must be a non-empty string",
            actual_type = type(value),
        }
    end

    local normalized = trim(value)
    if not vulcan.fs_exists(normalized) then
        return nil, {
            error = "file_not_found",
            message = "file does not exist",
            file = normalized,
        }
    end
    if vulcan.fs_is_dir(normalized) then
        return nil, {
            error = "file_must_be_regular_file",
            message = "file must point to a regular file",
            file = normalized,
        }
    end
    return normalized, nil
end

--[[
中文：校验 selector 与 replacement 参数。
English: Validate selector and replacement arguments.
]]
local function validate_selector_argument(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_selector_argument",
            message = "selector must be a non-empty string",
            actual_type = type(value),
        }
    end
    return trim(value), nil
end

local function validate_replacement_argument(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_replacement_argument",
            message = "replacement must be a non-empty string containing the full function source",
            actual_type = type(value),
        }
    end
    return tostring(value), nil
end

--[[
中文：显式拒绝旧版 `mode` 参数，避免调用方误以为工具仍支持 body/auto 分支。
English: Explicitly reject the legacy `mode` argument so callers do not assume body/auto branches still exist.
]]
local function validate_mode_absence(value)
    if value ~= nil then
        return {
            error = "mode_not_supported",
            message = "vmcp-patch only accepts full-function replacements and does not support the mode argument",
        }
    end
    return nil
end

--[[
中文：为 AST 节点补充父节点引用，便于后续构造结构路径 selector。
English: Attach parent references to AST nodes so structural selector paths can be derived later.
]]
local function attach_parent_links(symbols, parent_symbol)
    for _, symbol in ipairs(symbols or {}) do
        symbol.parent = parent_symbol
        attach_parent_links(symbol.children or {}, symbol)
    end
end

--[[
中文：判断一个节点是否为可 patch 的函数级节点。
English: Determine whether a node is a patchable function-level symbol.
]]
local function is_function_like(symbol)
    return symbol and (symbol.kind == "function" or symbol.kind == "method")
end

--[[
中文：深度优先收集树中所有可 patch 的函数级节点。
English: Collect every patchable function-level node from the tree with a depth-first traversal.
]]
local function collect_patchable_symbols(symbols, collected)
    for _, symbol in ipairs(symbols or {}) do
        if is_function_like(symbol) then
            table.insert(collected, symbol)
        end
        collect_patchable_symbols(symbol.children or {}, collected)
    end
end

--[[
中文：统一 selector 文本，做大小写归一与空白压缩，便于宽松匹配。
English: Normalize selector text with lowercase conversion and whitespace compaction for flexible matching.
]]
local function normalize_selector_text(text)
    return trim((tostring(text or ""):lower():gsub("%s+", " ")))
end

--[[
中文：提取结构签名中参数列表前的声明前缀，用于生成宽松 selector 别名。
English: Extract the declaration prefix before the parameter list so flexible selector aliases can be generated.
]]
local function extract_declaration_prefix(signature)
    local normalized = trim(signature or "")
    local declaration = normalized:match("^(.-)%(")
    if declaration and trim(declaration) ~= "" then
        return trim(declaration)
    end
    return normalized
end

--[[
中文：为单个结构节点生成一组宽松 selector 别名，使 `with_vmm`、`fn with_vmm`、`pub async fn with_vmm` 等表达都能命中。
English: Generate a set of flexible selector aliases for one symbol so forms like `with_vmm`, `fn with_vmm`, and `pub async fn with_vmm` can all match.
]]
local function build_symbol_segment_aliases(symbol)
    local aliases = {}
    local function add_alias(value)
        local normalized = normalize_selector_text(value)
        if normalized ~= "" then
            aliases[normalized] = true
        end
    end

    add_alias(symbol.name)

    local declaration = extract_declaration_prefix(symbol.signature or "")
    add_alias(declaration)

    if declaration ~= "" and symbol.name and symbol.name ~= "" then
        local bare_name = normalize_selector_text(symbol.name)
        local declaration_lower = normalize_selector_text(declaration)
        local keyword_patterns = {
            "fn%s+" .. bare_name,
            "def%s+" .. bare_name,
            "function%s+" .. bare_name,
            "func%s+" .. bare_name,
            "sub%s+" .. bare_name,
        }
        for _, pattern in ipairs(keyword_patterns) do
            local captured = declaration_lower:match("(" .. pattern .. ")")
            if captured and captured ~= "" then
                add_alias(captured)
            end
        end
    end

    return aliases
end

--[[
中文：构造某个函数节点的结构路径链，包含所有父级容器以及节点自身。
English: Build the structural path chain for a function node, including all parent containers and the node itself.
]]
local function build_symbol_chain(symbol)
    local chain = {}
    local cursor = symbol
    while cursor do
        table.insert(chain, 1, cursor)
        cursor = cursor.parent
    end
    return chain
end

--[[
中文：把 selector 文本按 `/` 切分为多个路径段，并做统一规范化。
English: Split selector text by `/` into path segments and normalize each segment.
]]
local function split_selector_segments(selector)
    local segments = {}
    for segment in tostring(selector or ""):gmatch("[^/]+") do
        local normalized = normalize_selector_text(segment)
        if normalized ~= "" then
            table.insert(segments, normalized)
        end
    end
    return segments
end

--[[
中文：判断一个函数节点是否命中给定 selector，采用“路径后缀 + 别名匹配”策略。
English: Determine whether a function node matches the given selector using suffix-path matching plus alias matching.
]]
local function symbol_matches_selector(symbol, selector_segments)
    local chain = build_symbol_chain(symbol)
    if #selector_segments == 0 or #selector_segments > #chain then
        return false
    end

    local chain_index = #chain
    for selector_index = #selector_segments, 1, -1 do
        local aliases = build_symbol_segment_aliases(chain[chain_index])
        if not aliases[selector_segments[selector_index]] then
            return false
        end
        chain_index = chain_index - 1
    end
    return true
end

--[[
中文：为匹配候选生成更完整的规范路径，便于在歧义场景下提示 AI 重新选择。
English: Build a fuller canonical path for a candidate so the AI can retry with a more specific selector when ambiguity occurs.
]]
local function build_canonical_symbol_path(symbol)
    local segments = {}
    for _, node in ipairs(build_symbol_chain(symbol)) do
        table.insert(segments, extract_declaration_prefix(node.signature or node.name or "unknown"))
    end
    return table.concat(segments, "/")
end

--[[
中文：构造仅由结构名称组成的稳定身份路径，用于在代码行号漂移后重新定位同一函数节点。
English: Build a stable identity path composed only of structural names so the same function node can be re-located after line numbers drift.
]]
local function build_symbol_identity_path(symbol)
    local segments = {}
    for _, node in ipairs(build_symbol_chain(symbol)) do
        local segment = trim(node.name or "")
        if segment == "" then
            segment = extract_declaration_prefix(node.signature or node.kind or "unknown")
        end
        if segment ~= "" then
            table.insert(segments, segment)
        end
    end
    return table.concat(segments, "/")
end

--[[
中文：为歧义候选构造结构化信息，返回文件、路径、签名与行号范围。
English: Build structured ambiguity candidate details including file, path, signature, and line range.
]]
local function build_candidate_descriptor(symbol)
    return {
        file = symbol.file,
        path = build_canonical_symbol_path(symbol),
        signature = trim(symbol.signature or ""),
        start_line = symbol.start_line,
        end_line = symbol.end_line,
    }
end

--[[
中文：读取目标文件并返回原始文本、行数组、换行符风格和尾随换行状态。
English: Read the target file and return raw text, line array, newline style, and trailing-newline state.
]]
local function read_file_content(file_path)
    local ok, raw_content = pcall(vulcan.fs_read, file_path)
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
        newline = text:find("\r\n", 1, true) and "\r\n" or "\n",
        has_trailing_newline = text:match("[\r\n]$") ~= nil,
    }, nil
end

--[[
中文：构造与原文件同目录、同扩展名的临时文件路径，便于后续先校验再替换。
English: Build a temporary file path that stays in the same directory and keeps the original extension so validation can happen before the final swap.
]]
local function build_sidecar_file_path(file_path, label)
    local directory, file_name = tostring(file_path or ""):match("^(.*[\\/])([^\\/]+)$")
    directory = directory or ""
    file_name = file_name or tostring(file_path or "")
    local base_name, extension = file_name:match("^(.*)(%.[^%.]+)$")
    if not base_name then
        base_name = file_name
        extension = ""
    end

    local unique_suffix = string.format("%d_%d", os.time(), math.floor((os.clock() % 1) * 1000000))
    return directory .. base_name .. "." .. tostring(label or "tmp") .. "." .. unique_suffix .. extension
end

--[[
中文：尝试删除一个文件，失败时静默忽略，用于清理临时文件与回滚副本。
English: Try to delete one file and silently ignore failures, which is useful for temp-file cleanup and rollback backup cleanup.
]]
local function safe_remove_file(file_path)
    if type(file_path) ~= "string" or file_path == "" then
        return
    end
    if vulcan.fs_exists(file_path) then
        pcall(os.remove, file_path)
    end
end

--[[
中文：执行同目录重命名，用于临时文件替换与失败回滚。
English: Rename one file within the same directory, used for temp-file swapping and rollback restoration.
]]
local function rename_file(source_path, target_path)
    local ok, renamed, message = pcall(os.rename, source_path, target_path)
    if not ok then
        return false, tostring(renamed)
    end
    if not renamed then
        return false, tostring(message or "rename failed")
    end
    return true, nil
end

--[[
中文：提取最小公共缩进并去除，方便把 AI 给出的 replacement 重新缩进到目标节点层级。
English: Remove the minimal common indentation so AI-provided replacement text can be re-indented to the target node level.
]]
local function dedent_lines(lines)
    local min_indent = nil
    for _, line in ipairs(lines or {}) do
        if trim(line) ~= "" then
            local indent = #(line:match("^(%s*)") or "")
            if min_indent == nil or indent < min_indent then
                min_indent = indent
            end
        end
    end

    if not min_indent or min_indent <= 0 then
        return clone_array(lines or {})
    end

    local result = {}
    for _, line in ipairs(lines or {}) do
        if trim(line) == "" then
            table.insert(result, "")
        else
            table.insert(result, line:sub(min_indent + 1))
        end
    end
    return result
end

--[[
中文：按目标缩进重新缩进 replacement 文本行，保留空行。
English: Re-indent replacement text lines with the target indentation while preserving blank lines.
]]
local function reindent_lines(lines, indent)
    local result = {}
    for _, line in ipairs(lines or {}) do
        if trim(line) == "" then
            table.insert(result, "")
        else
            table.insert(result, tostring(indent or "") .. line)
        end
    end
    return result
end

--[[
中文：根据原函数起始行推断该节点的声明缩进。
English: Infer the declaration indentation of the target node from its first source line.
]]
local function detect_symbol_indent(file_lines, symbol)
    local first_line = file_lines[(tonumber(symbol.start_line) or 1)] or ""
    return first_line:match("^(%s*)") or ""
end

--[[
中文：把完整函数 replacement 调整到目标节点的声明缩进层级。
English: Re-indent a full-function replacement so it matches the declaration indentation of the target node.
]]
local function build_full_replacement_lines(symbol, file_lines, replacement_text)
    local declaration_indent = detect_symbol_indent(file_lines, symbol)
    local replacement_lines = dedent_lines(split_lines(replacement_text))
    return reindent_lines(replacement_lines, declaration_indent)
end

--[[
中文：校验 replacement 是否满足“完整函数源码”这一严格输入规则。
English: Validate that the replacement follows the strict full-function-source rule.
]]
local function validate_full_replacement_shape(symbol, replacement_text)
    local non_empty_lines = {}
    for _, line in ipairs(split_lines(replacement_text)) do
        local normalized = normalize_selector_text(line)
        if normalized ~= "" then
            table.insert(non_empty_lines, normalized)
        end
        if #non_empty_lines >= 5 then
            break
        end
    end

    if #non_empty_lines == 0 then
        return {
            error = "replacement_must_be_full_function",
            message = "replacement must contain the full function source, not an empty body fragment",
            selector = build_canonical_symbol_path(symbol),
        }
    end

    local first_line = non_empty_lines[1]
    local target_name = normalize_selector_text(symbol.name or "")
    if target_name == "" or not first_line:find(target_name, 1, true) then
        return {
            error = "replacement_must_be_full_function",
            message = "replacement must start from the target function declaration line",
            selector = build_canonical_symbol_path(symbol),
            expected_name = symbol.name,
        }
    end

    if not first_line:find("(", 1, true) then
        return {
            error = "replacement_must_be_full_function",
            message = "replacement must start from a function declaration instead of body-only statements",
            selector = build_canonical_symbol_path(symbol),
        }
    end

    return nil
end

--[[
中文：把节点替换结果回写到文件行数组中，返回新的完整文件行数组。
English: Apply the node replacement to the file line array and return the new full-file line array.
]]
local function build_replaced_file_lines(file_lines, symbol, replacement_lines)
    local rebuilt = {}
    local start_line = tonumber(symbol.start_line) or 1
    local end_line = tonumber(symbol.end_line) or start_line

    for index = 1, start_line - 1 do
        table.insert(rebuilt, file_lines[index] or "")
    end
    for _, line in ipairs(replacement_lines or {}) do
        table.insert(rebuilt, line)
    end
    for index = end_line + 1, #file_lines do
        table.insert(rebuilt, file_lines[index] or "")
    end
    return rebuilt
end

--[[
中文：按原文件的换行风格把完整文件内容重新拼接为文本。
English: Rebuild the full file text using the original file's newline style.
]]
local function join_file_lines(lines, newline, has_trailing_newline)
    local text = table.concat(lines or {}, newline or "\n")
    if has_trailing_newline then
        text = text .. (newline or "\n")
    end
    return text
end

--[[
中文：收集单文件 AST 结构树，为 selector 匹配和 patch 提供结构上下文。
English: Collect the AST tree for a single file to provide the structural context needed by selector matching and patch application.
]]
local function collect_ast_for_file(file_path, helper_bundle)
    local files, _, collection_errors, collection_error = helper_bundle.collect_files({ file_path }, false, nil, true)
    if collection_error then
        return nil, nil, collection_error
    end
    if collection_errors and #collection_errors > 0 then
        return nil, nil, {
            error = "file_collection_failed",
            message = "failed to collect the target file for AST analysis",
            diagnostics = collection_errors,
            file = file_path,
        }
    end
    if not files or #files == 0 then
        return nil, nil, {
            error = "file_not_analyzable",
            message = "the target file could not be analyzed by vmcp-ast",
            file = file_path,
        }
    end

    local file_info = files[1]
    local ast_binary_path, ast_binary_directory, ast_executable_name = helper_bundle.find_binary()
    if not ast_binary_path then
        return nil, nil, {
            error = "ast_grep_binary_not_found",
            message = "ast-grep binary not found in shared tool directory",
        }
    end

    local matches, diagnostics = helper_bundle.run_language_scan(ast_binary_directory, ast_executable_name, file_info.language, { file_info.path })
    if diagnostics and #diagnostics > 0 then
        return nil, nil, {
            error = "ast_scan_failed",
            message = "ast-grep scanning reported diagnostics for the target file",
            diagnostics = diagnostics,
            file = file_info.path,
        }
    end

    local normalized_symbols = {}
    for _, match in ipairs(matches or {}) do
        local symbol = helper_bundle.normalize_symbol(match, file_info.language)
        if symbol then
            table.insert(normalized_symbols, symbol)
        end
    end
    local unique_symbols = helper_bundle.deduplicate_symbols(normalized_symbols)
    local tree = helper_bundle.build_symbol_tree(unique_symbols)
    attach_parent_links(tree, nil)
    return tree, file_info, nil
end

--[[
中文：构造通用 ERROR 节点扫描规则，利用 ast-grep 的错误节点匹配做跨语言语法损坏探测。
English: Build a generic ERROR-node scan rule so ast-grep can detect syntax damage across languages.
]]
local function build_error_node_rule(language_key)
    return table.concat({
        "id: vmcp-patch-error-node",
        "language: " .. tostring(language_key or ""),
        "rule:",
        "  kind: ERROR",
        "",
    }, "\n")
end

--[[
中文：执行 ERROR 节点扫描，若写入后的文件出现解析错误节点，则返回结构化错误信息。
English: Run an ERROR-node scan and return a structured error when the patched file contains parser error nodes.
]]
local function scan_ast_error_nodes(file_path, file_info, helper_bundle)
    if type(vulcan.exec) ~= "function" then
        return {}, nil
    end

    local ast_binary_path, ast_binary_directory = helper_bundle.find_binary()
    if not ast_binary_path then
        return nil, {
            error = "ast_grep_binary_not_found",
            message = "ast-grep binary not found in shared tool directory",
        }
    end

    local ok, result = pcall(vulcan.exec, {
        program = ast_binary_path,
        args = {
            "scan",
            "--inline-rules",
            build_error_node_rule(file_info.language),
            "--json=compact",
            "--color=never",
            file_path,
        },
        cwd = ast_binary_directory,
        timeout_ms = 30000,
    })
    if not ok then
        return nil, {
            error = "error_node_scan_failed",
            message = tostring(result),
            file = file_path,
        }
    end
    if type(result) ~= "table" then
        return nil, {
            error = "error_node_scan_failed",
            message = "unexpected ast-grep execution result while validating the patched file",
            file = file_path,
        }
    end
    if result.timed_out then
        return nil, {
            error = "error_node_scan_timed_out",
            message = "ast-grep error-node validation timed out for the patched file",
            file = file_path,
        }
    end
    if result.error then
        return nil, {
            error = "error_node_scan_failed",
            message = "ast-grep could not complete error-node validation for the patched file",
            file = file_path,
            details = {
                error = tostring(result.error or ""),
                stderr = trim(result.stderr or ""),
            },
        }
    end

    local raw_output = trim(result.stdout or "")
    if raw_output == "" then
        return {}, nil
    end

    local decoded, decode_error = vulcan.json_decode(raw_output)
    if not decoded then
        return nil, {
            error = "error_node_scan_decode_failed",
            message = "failed to decode ast-grep error-node validation output",
            file = file_path,
            details = tostring(decode_error),
        }
    end
    return decoded, nil
end

--[[
中文：将 ERROR 节点命中结果压缩成易读诊断，便于在失败时快速理解问题位置。
English: Compress ERROR-node matches into readable diagnostics so failures can be understood quickly.
]]
local function summarize_error_node_matches(matches)
    local diagnostics = {}
    for index, match in ipairs(matches or {}) do
        if index > 5 then
            break
        end
        local start_line = match and match.range and match.range.start and match.range.start.line
        table.insert(diagnostics, {
            line = start_line and (tonumber(start_line) + 1) or nil,
            text = trim(match and match.lines or ""),
        })
    end
    return diagnostics
end

--[[
中文：对写入后的文件做通用结构校验，包括 ERROR 节点检测与目标函数重定位检查。
English: Validate the patched file generically by checking ERROR nodes and re-locating the target function.
]]
local function validate_ast_after_write(file_path, helper_bundle, original_symbol)
    local symbol_roots, file_info, validation_error = collect_ast_for_file(file_path, helper_bundle)
    if validation_error then
        return nil, {
            error = "post_write_ast_validation_failed",
            message = "patched file failed AST validation and was rejected",
            file = file_path,
            details = validation_error,
        }
    end

    local error_matches, error_scan_error = scan_ast_error_nodes(file_path, file_info, helper_bundle)
    if error_scan_error then
        return nil, error_scan_error
    end
    if error_matches and #error_matches > 0 then
        return nil, {
            error = "syntax_error_nodes_detected",
            message = "patched file introduced parser error nodes and was rejected",
            file = file_path,
            details = {
                count = #error_matches,
                diagnostics = summarize_error_node_matches(error_matches),
            },
        }
    end

    local identity_selector = build_symbol_identity_path(original_symbol)
    local matches = find_matching_patch_targets(symbol_roots, identity_selector)
    if #matches == 0 then
        return nil, {
            error = "patched_target_not_found",
            message = "patched file no longer contains the target function under the original structural path",
            file = file_path,
            selector = identity_selector,
        }
    end
    if #matches > 1 then
        local candidates = {}
        for _, symbol in ipairs(matches) do
            table.insert(candidates, build_candidate_descriptor(symbol))
        end
        return nil, {
            error = "patched_target_ambiguous",
            message = "patched file produced multiple candidate functions for the original structural path",
            file = file_path,
            selector = identity_selector,
            candidates = candidates,
        }
    end

    local relocated_symbol = matches[1]
    if relocated_symbol.kind ~= original_symbol.kind or trim(relocated_symbol.name or "") ~= trim(original_symbol.name or "") then
        return nil, {
            error = "patched_target_identity_changed",
            message = "patched file changed the target function identity and was rejected",
            file = file_path,
            details = {
                expected = {
                    kind = original_symbol.kind,
                    name = trim(original_symbol.name or ""),
                },
                actual = {
                    kind = relocated_symbol.kind,
                    name = trim(relocated_symbol.name or ""),
                },
            },
        }
    end

    return relocated_symbol, nil
end

--[[
中文：在单文件 AST 树中按 selector 重新定位可 patch 的函数节点。
English: Re-locate patchable function nodes inside a single-file AST tree by selector.
]]
find_matching_patch_targets = function(symbol_roots, selector)
    local patchable_symbols = {}
    collect_patchable_symbols(symbol_roots or {}, patchable_symbols)

    local selector_segments = split_selector_segments(selector)
    local matches = {}
    for _, symbol in ipairs(patchable_symbols) do
        if symbol_matches_selector(symbol, selector_segments) then
            table.insert(matches, symbol)
        end
    end
    return matches
end

--[[
中文：将 patch 结果写回磁盘，按“完整函数替换”规则覆盖目标函数节点源码。
English: Persist the patch result to disk by replacing the target function node with a full-function replacement.
]]
local function apply_patch_to_symbol(file_path, symbol, replacement_text)
    local file_content, file_error = read_file_content(file_path)
    if file_error then
        return nil, file_error
    end

    local replacement_shape_error = validate_full_replacement_shape(symbol, replacement_text)
    if replacement_shape_error then
        return nil, replacement_shape_error
    end

    local replacement_lines = build_full_replacement_lines(symbol, file_content.lines, replacement_text)

    local new_file_lines = build_replaced_file_lines(file_content.lines, symbol, replacement_lines)
    local new_file_text = join_file_lines(new_file_lines, file_content.newline, file_content.has_trailing_newline)

    local temp_path = build_sidecar_file_path(file_path, "vmcp_patch_tmp")
    local backup_path = build_sidecar_file_path(file_path, "vmcp_patch_backup")
    safe_remove_file(temp_path)
    safe_remove_file(backup_path)

    local temp_written, temp_write_error = pcall(vulcan.fs_write, temp_path, new_file_text)
    if not temp_written then
        safe_remove_file(temp_path)
        return nil, {
            error = "temp_file_write_failed",
            message = tostring(temp_write_error),
            file = file_path,
            temp_file = temp_path,
        }
    end

    local _, temp_validation_error = validate_ast_after_write(temp_path, AST_RUNTIME_HELPERS, symbol)
    if temp_validation_error then
        safe_remove_file(temp_path)
        return nil, temp_validation_error
    end

    local moved_to_backup, backup_error = rename_file(file_path, backup_path)
    if not moved_to_backup then
        safe_remove_file(temp_path)
        return nil, {
            error = "backup_creation_failed",
            message = tostring(backup_error),
            file = file_path,
            backup_file = backup_path,
        }
    end

    local moved_temp_into_place, swap_error = rename_file(temp_path, file_path)
    if not moved_temp_into_place then
        rename_file(backup_path, file_path)
        safe_remove_file(temp_path)
        return nil, {
            error = "temp_swap_failed",
            message = tostring(swap_error),
            file = file_path,
            backup_file = backup_path,
        }
    end

    local relocated_symbol, final_validation_error = validate_ast_after_write(file_path, AST_RUNTIME_HELPERS, symbol)
    if final_validation_error then
        safe_remove_file(file_path)
        local restored, restore_error = rename_file(backup_path, file_path)
        if not restored then
            local fallback_ok, fallback_error = pcall(vulcan.fs_write, file_path, file_content.raw)
            if not fallback_ok then
                return nil, {
                    error = "rollback_failed",
                    message = "final validation failed and rollback could not restore the original file",
                    file = file_path,
                    validation = final_validation_error,
                    restore_error = tostring(restore_error),
                    fallback_error = tostring(fallback_error),
                }
            end
        end
        return nil, {
            error = "patch_reverted_after_validation_failure",
            message = "patched file failed AST validation and the original file was restored",
            file = file_path,
            validation = final_validation_error,
        }
    end

    safe_remove_file(backup_path)

    return {
        success = true,
        file = file_path,
        selector = build_canonical_symbol_path(relocated_symbol or symbol),
        patched_node = {
            file = (relocated_symbol and relocated_symbol.file) or symbol.file,
            path = build_canonical_symbol_path(relocated_symbol or symbol),
            signature = trim(((relocated_symbol and relocated_symbol.signature) or symbol.signature) or ""),
            start_line = (relocated_symbol and relocated_symbol.start_line) or symbol.start_line,
            end_line = (relocated_symbol and relocated_symbol.end_line) or symbol.end_line,
        },
    }, nil
end

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return helper_error
    end

    local file_path, file_error = validate_file_argument(args and args.file)
    if file_error then
        return file_error
    end

    local selector, selector_error = validate_selector_argument(args and args.selector)
    if selector_error then
        return selector_error
    end

    local replacement_text, replacement_error = validate_replacement_argument(args and args.replacement)
    if replacement_error then
        return replacement_error
    end

    local mode_error = validate_mode_absence(args and args.mode)
    if mode_error then
        return mode_error
    end

    local symbol_roots, _, ast_error = collect_ast_for_file(file_path, helper_bundle)
    if ast_error then
        return ast_error
    end

    local matches = find_matching_patch_targets(symbol_roots, selector)
    if #matches == 0 then
        return {
            error = "selector_not_found",
            message = "no patchable function matched the selector",
            file = file_path,
            selector = selector,
        }
    end

    if #matches > 1 then
        local candidates = {}
        for _, symbol in ipairs(matches) do
            table.insert(candidates, build_candidate_descriptor(symbol))
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
        return {
            error = "ambiguous_selector",
            message = "multiple patchable functions matched the selector; retry with a more specific structural path",
            file = file_path,
            selector = selector,
            candidates = candidates,
        }
    end

    local result, patch_error = apply_patch_to_symbol(file_path, matches[1], replacement_text)
    if patch_error then
        return patch_error
    end
    return result
end
