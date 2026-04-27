--[[
codekit-patch
基于 AST 结构路径重新定位函数/方法节点，并执行整函数替换。
Re-locate function or method nodes by AST structural selectors and replace the full function source.
]]

-- 工具常量 / Tool constants for selector matching and replacement behavior.
local AST_RUNTIME_HELPERS = nil
local find_matching_patch_targets
-- Default maximum patch requests processed by one batch call.
-- 单次批量调用默认处理的最大 patch 请求数。
local DEFAULT_MAX_PATCHES = 20

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
浅拷贝数组，避免在树遍历和代码拼装时直接修改原始列表。
Create a shallow array copy so tree traversal and code assembly do not mutate the original list in place.

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
通过 `debug.getupvalue` 从现有 `codekit-ast-detail` 入口中提取内部 helper，避免复制整套 AST 分析实现。
Extract internal helpers from the existing `codekit-ast-detail` entry through `debug.getupvalue` so the full AST pipeline does not need to be duplicated.

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
获取宿主注入的当前 skill 目录。
Resolve the current skill directory injected by the host.

返回 / Returns:
- string: 当前 skill 目录 / Current skill directory.
]]
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
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
懒加载 `codekit-ast-detail` 内部 helper，确保 `codekit-patch` 与现有 AST 规则、符号归一化和结构建树逻辑完全一致。
Lazily load internal `codekit-ast-detail` helpers so `codekit-patch` remains fully aligned with the existing AST rules, symbol normalization, and tree-building logic.

返回 / Returns:
- table|nil: helper 函数集合 / Helper bundle on success.
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
        collect_files = extract_upvalue_by_name(ast_entry, "collect_files"),
        find_binary = extract_upvalue_by_name(ast_entry, "find_binary"),
        run_language_scan = extract_upvalue_by_name(ast_entry, "run_language_scan"),
        run_inline_rule_scan = extract_upvalue_by_name(ast_entry, "run_inline_rule_scan"),
        normalize_symbol = extract_upvalue_by_name(ast_entry, "normalize_symbol"),
        deduplicate_symbols = extract_upvalue_by_name(ast_entry, "deduplicate_symbols"),
        build_symbol_tree = extract_upvalue_by_name(ast_entry, "build_symbol_tree"),
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
校验目标文件参数，要求为存在的单个文件路径。
Validate the target file argument. It must be a single existing file path.
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
    if not vulcan.fs.exists(normalized) then
        return nil, {
            error = "file_not_found",
            message = "file does not exist",
            file = normalized,
        }
    end
    if vulcan.fs.is_dir(normalized) then
        return nil, {
            error = "file_must_be_regular_file",
            message = "file must point to a regular file",
            file = normalized,
        }
    end
    return normalized, nil
end

--[[
校验 selector 与 replacement 参数。
Validate selector and replacement arguments.
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
显式拒绝旧版 `mode` 参数，避免调用方误以为工具仍支持 body/auto 分支。
Explicitly reject the legacy `mode` argument so callers do not assume body/auto branches still exist.
]]
--[[
把结构化错误对象渲染成适合 AI 直接消费的 Markdown 文本，避免宿主再看到 Lua table。
Render a structured error object into AI-friendly Markdown text so the host never receives a Lua table.

参数 / Parameters:
- error_payload(table|nil): 内部错误对象 / Internal structured error object.

返回 / Returns:
- string: Markdown 格式的错误文本 / Markdown-formatted error text.
]]
local function render_patch_error(error_payload)
    local payload = type(error_payload) == "table" and error_payload or {
        error = "unknown_patch_error",
        message = tostring(error_payload or "unknown patch error"),
    }

    local lines = {
        "# PATCH ERROR",
        string.format("- error: `%s`", tostring(payload.error or "unknown_patch_error")),
        string.format("- message: %s", tostring(payload.message or "unknown patch error")),
    }

    if payload.file then
        table.insert(lines, string.format("- file: `%s`", tostring(payload.file)))
    end
    if payload.selector then
        table.insert(lines, string.format("- selector: `%s`", tostring(payload.selector)))
    end
    if payload.helper then
        table.insert(lines, string.format("- helper: `%s`", tostring(payload.helper)))
    end
    if payload.path then
        table.insert(lines, string.format("- path: `%s`", tostring(payload.path)))
    end
    if payload.temp_file then
        table.insert(lines, string.format("- temp_file: `%s`", tostring(payload.temp_file)))
    end
    if payload.backup_file then
        table.insert(lines, string.format("- backup_file: `%s`", tostring(payload.backup_file)))
    end

    if type(payload.candidates) == "table" and #payload.candidates > 0 then
        table.insert(lines, "")
        table.insert(lines, "## Candidates")
        for _, candidate in ipairs(payload.candidates) do
            local descriptor = tostring((candidate and candidate.path) or "unknown")
            local signature = trim((candidate and candidate.signature) or "")
            local start_line = tonumber(candidate and candidate.start_line)
            local end_line = tonumber(candidate and candidate.end_line)
            local location = nil
            if start_line and end_line then
                location = string.format("L%d-%d", start_line, end_line)
            elseif start_line then
                location = string.format("L%d", start_line)
            end
            if signature ~= "" then
                descriptor = string.format("%s | `%s`", descriptor, signature)
            end
            if location then
                descriptor = string.format("%s | %s", descriptor, location)
            end
            table.insert(lines, string.format("- %s", descriptor))
        end
    end

    return table.concat(lines, "\n")
end

--[[
把成功 patch 的结构化结果渲染成 Markdown 文本，便于 AI 直接理解修改落点。
Render the successful patch result into Markdown text so the AI can immediately understand what was changed.

参数 / Parameters:
- result_payload(table|nil): patch 成功后的结构化结果 / Structured success payload after patching.

返回 / Returns:
- string: Markdown 格式的成功文本 / Markdown-formatted success text.
]]
local function render_patch_success(result_payload)
    local payload = type(result_payload) == "table" and result_payload or {}
    local patched_node = type(payload.patched_node) == "table" and payload.patched_node or {}

    local lines = {
        "# PATCH APPLIED",
        string.format("- success: `%s`", tostring(payload.success == true)),
    }

    if payload.file then
        table.insert(lines, string.format("- file: `%s`", tostring(payload.file)))
    end
    if payload.selector then
        table.insert(lines, string.format("- selector: `%s`", tostring(payload.selector)))
    end

    table.insert(lines, "")
    table.insert(lines, "## Patched Node")
    table.insert(lines, string.format("- path: `%s`", tostring(patched_node.path or "unknown")))
    table.insert(lines, string.format("- signature: `%s`", tostring(patched_node.signature or "")))
    if patched_node.start_line and patched_node.end_line then
        table.insert(lines, string.format("- lines: `L%d-%d`", tonumber(patched_node.start_line) or 0, tonumber(patched_node.end_line) or 0))
    elseif patched_node.start_line then
        table.insert(lines, string.format("- lines: `L%d`", tonumber(patched_node.start_line) or 0))
    end

    return table.concat(lines, "\n")
end

--[[
为 AST 节点补充父节点引用，便于后续构造结构路径 selector。
Attach parent references to AST nodes so structural selector paths can be derived later.
]]
local function attach_parent_links(symbols, parent_symbol)
    for _, symbol in ipairs(symbols or {}) do
        symbol.parent = parent_symbol
        attach_parent_links(symbol.children or {}, symbol)
    end
end

--[[
判断一个节点是否为可 patch 的函数级节点。
Determine whether a node is a patchable function-level symbol.
]]
local function is_function_like(symbol)
    return symbol and (symbol.kind == "function" or symbol.kind == "method")
end

--[[
深度优先收集树中所有可 patch 的函数级节点。
Collect every patchable function-level node from the tree with a depth-first traversal.
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
统一 selector 文本，做大小写归一与空白压缩，便于宽松匹配。
Normalize selector text with lowercase conversion and whitespace compaction for flexible matching.
]]
local function normalize_selector_text(text)
    return trim((tostring(text or ""):lower():gsub("%s+", " ")))
end

--[[
提取结构签名中参数列表前的声明前缀，用于生成宽松 selector 别名。
Extract the declaration prefix before the parameter list so flexible selector aliases can be generated.
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
为单个结构节点生成一组宽松 selector 别名，使 `with_vmm`、`fn with_vmm`、`pub async fn with_vmm` 等表达都能命中。
Generate a set of flexible selector aliases for one symbol so forms like `with_vmm`, `fn with_vmm`, and `pub async fn with_vmm` can all match.
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
构造某个函数节点的结构路径链，包含所有父级容器以及节点自身。
Build the structural path chain for a function node, including all parent containers and the node itself.
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
把 selector 文本按 `/` 切分为多个路径段，并做统一规范化。
Split selector text by `/` into path segments and normalize each segment.
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
判断一个函数节点是否命中给定 selector，采用“路径后缀 + 别名匹配”策略。
Determine whether a function node matches the given selector using suffix-path matching plus alias matching.
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
为匹配候选生成更完整的规范路径，便于在歧义场景下提示 AI 重新选择。
Build a fuller canonical path for a candidate so the AI can retry with a more specific selector when ambiguity occurs.
]]
local function build_canonical_symbol_path(symbol)
    local segments = {}
    for _, node in ipairs(build_symbol_chain(symbol)) do
        table.insert(segments, extract_declaration_prefix(node.signature or node.name or "unknown"))
    end
    return table.concat(segments, "/")
end

--[[
构造仅由结构名称组成的稳定身份路径，用于在代码行号漂移后重新定位同一函数节点。
Build a stable identity path composed only of structural names so the same function node can be re-located after line numbers drift.
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
为歧义候选构造结构化信息，返回文件、路径、签名与行号范围。
Build structured ambiguity candidate details including file, path, signature, and line range.
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
读取目标文件并返回原始文本、行数组、换行符风格和尾随换行状态。
Read the target file and return raw text, line array, newline style, and trailing-newline state.
]]
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
        newline = text:find("\r\n", 1, true) and "\r\n" or "\n",
        has_trailing_newline = text:match("[\r\n]$") ~= nil,
    }, nil
end

--[[
构造与原文件同目录、同扩展名的临时文件路径，便于后续先校验再替换。
Build a temporary file path that stays in the same directory and keeps the original extension so validation can happen before the final swap.
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
尝试删除一个文件，失败时静默忽略，用于清理临时文件与回滚副本。
Try to delete one file and silently ignore failures, which is useful for temp-file cleanup and rollback backup cleanup.
]]
local function safe_remove_file(file_path)
    if type(file_path) ~= "string" or file_path == "" then
        return
    end
    if vulcan.fs.exists(file_path) then
        pcall(os.remove, file_path)
    end
end

--[[
执行同目录重命名，用于临时文件替换与失败回滚。
Rename one file within the same directory, used for temp-file swapping and rollback restoration.
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
提取最小公共缩进并去除，方便把 AI 给出的 replacement 重新缩进到目标节点层级。
Remove the minimal common indentation so AI-provided replacement text can be re-indented to the target node level.
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
按目标缩进重新缩进 replacement 文本行，保留空行。
Re-indent replacement text lines with the target indentation while preserving blank lines.
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
根据原函数起始行推断该节点的声明缩进。
Infer the declaration indentation of the target node from its first source line.
]]
local function detect_symbol_indent(file_lines, symbol)
    local first_line = file_lines[(tonumber(symbol.start_line) or 1)] or ""
    return first_line:match("^(%s*)") or ""
end

--[[
把完整函数 replacement 调整到目标节点的声明缩进层级。
Re-indent a full-function replacement so it matches the declaration indentation of the target node.
]]
local function build_full_replacement_lines(symbol, file_lines, replacement_text)
    local declaration_indent = detect_symbol_indent(file_lines, symbol)
    local replacement_lines = dedent_lines(split_lines(replacement_text))
    return reindent_lines(replacement_lines, declaration_indent)
end

--[[
校验 replacement 是否满足“完整函数源码”这一严格输入规则。
Validate that the replacement follows the strict full-function-source rule.
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
把节点替换结果回写到文件行数组中，返回新的完整文件行数组。
Apply the node replacement to the file line array and return the new full-file line array.
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
按原文件的换行风格把完整文件内容重新拼接为文本。
Rebuild the full file text using the original file's newline style.
]]
local function join_file_lines(lines, newline, has_trailing_newline)
    local text = table.concat(lines or {}, newline or "\n")
    if has_trailing_newline then
        text = text .. (newline or "\n")
    end
    return text
end

--[[
收集单文件 AST 结构树，为 selector 匹配和 patch 提供结构上下文。
Collect the AST tree for a single file to provide the structural context needed by selector matching and patch application.
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
            message = "the target file could not be analyzed by codekit-ast-detail",
            file = file_path,
        }
    end

    local file_info = files[1]
    local scanner_client, _, _, scanner_error = helper_bundle.find_binary()
    if not scanner_client then
        return nil, nil, {
            error = "ast_grep_ffi_not_found",
            message = "ast-grep FFI library not found in the current skill dependency root",
            details = scanner_error,
        }
    end

    local matches, diagnostics = helper_bundle.run_language_scan(scanner_client, nil, file_info.language, { file_info.path })
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
构造通用 ERROR 节点扫描规则，利用 ast-grep 的错误节点匹配做跨语言语法损坏探测。
Build a generic ERROR-node scan rule so ast-grep can detect syntax damage across languages.
]]
local function build_error_node_rule(language_key)
    return table.concat({
        "id: codekit-patch-error-node",
        "language: " .. tostring(language_key or ""),
        "rule:",
        "  kind: ERROR",
        "",
    }, "\n")
end

--[[
执行 ERROR 节点扫描，若写入后的文件出现解析错误节点，则返回结构化错误信息。
Run an ERROR-node scan and return a structured error when the patched file contains parser error nodes.
]]
local function scan_ast_error_nodes(file_path, file_info, helper_bundle)
    local scanner_client, _, _, scanner_error = helper_bundle.find_binary()
    if not scanner_client then
        return nil, {
            error = "ast_grep_ffi_not_found",
            message = "ast-grep FFI library not found in the current skill dependency root",
            details = scanner_error,
        }
    end

    local matches, diagnostics = helper_bundle.run_inline_rule_scan(
        scanner_client,
        file_info.language,
        build_error_node_rule(file_info.language),
        { file_path }
    )
    if matches == nil then
        return nil, {
            error = "error_node_scan_failed",
            message = "ast-grep FFI could not complete error-node validation for the patched file",
            file = file_path,
            details = diagnostics,
        }
    end
    return matches, nil
end

--[[
将 ERROR 节点命中结果压缩成易读诊断，便于在失败时快速理解问题位置。
Compress ERROR-node matches into readable diagnostics so failures can be understood quickly.
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
对写入后的文件做通用结构校验，包括 ERROR 节点检测与目标函数重定位检查。
Validate the patched file generically by checking ERROR nodes and re-locating the target function.
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
在单文件 AST 树中按 selector 重新定位可 patch 的函数节点。
Re-locate patchable function nodes inside a single-file AST tree by selector.
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
将 patch 结果写回磁盘，按“完整函数替换”规则覆盖目标函数节点源码。
Persist the patch result to disk by replacing the target function node with a full-function replacement.
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

    local temp_written, temp_write_error = pcall(vulcan.fs.write, temp_path, new_file_text)
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
            local fallback_ok, fallback_error = pcall(vulcan.fs.write, file_path, file_content.raw)
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

-- Compute a stable lightweight source hash for stale-check comparisons.
-- 计算稳定的轻量源码哈希，用于 stale check 比对。
local function compute_source_hash(text)
    local hash = 5381
    local source = tostring(text or "")
    for index = 1, #source do
        hash = ((hash * 131) + source:byte(index)) % 4294967296
    end
    return string.format("%08x", hash)
end

-- Extract the original source text for a matched symbol.
-- 提取匹配符号的原始源码文本。
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

    local lines = {}
    for line_number = start_line, end_line do
        table.insert(lines, file_content.lines[line_number] or "")
    end
    return table.concat(lines, "\n"), nil
end

-- Normalize the optional atomic argument; batch patching defaults to atomic.
-- 规范化可选 atomic 参数；批量 patch 默认使用原子语义。
local function normalize_atomic_argument(value)
    if value == false or value == "false" then
        return false
    end
    return true
end

-- Normalize the optional max_patches argument.
-- 规范化可选 max_patches 参数。
local function normalize_max_patches_argument(value)
    local normalized = tonumber(value)
    if not normalized or normalized < 1 then
        return DEFAULT_MAX_PATCHES
    end
    return math.floor(normalized)
end

-- Normalize an expected hash string for comparison.
-- 规范化预期哈希字符串以便比较。
local function normalize_expected_hash(value)
    if value == nil then
        return nil
    end
    local normalized = trim(value):lower():gsub("^sha256:", ""):gsub("^hash:", "")
    if normalized == "" then
        return nil
    end
    return normalized
end

-- Parse an expected source range from table or string forms.
-- 从 table 或字符串形式解析预期源码范围。
local function parse_expected_range(value)
    if value == nil then
        return nil, nil
    end
    if type(value) == "table" then
        local start_line = tonumber(value.start_line or value.start or value[1])
        local end_line = tonumber(value.end_line or value["end"] or value[2] or start_line)
        if start_line and end_line then
            return {
                start_line = math.floor(start_line),
                end_line = math.floor(end_line),
            }, nil
        end
        return nil, {
            error = "invalid_expected_range",
            message = "expected_range table must include start_line and end_line",
        }
    end
    if type(value) == "string" then
        local start_text, end_text = tostring(value):match("L?(%d+)%s*[%-%:]%s*L?(%d+)")
        if not start_text then
            start_text = tostring(value):match("L?(%d+)")
            end_text = start_text
        end
        if start_text and end_text then
            return {
                start_line = tonumber(start_text),
                end_line = tonumber(end_text),
            }, nil
        end
        return nil, {
            error = "invalid_expected_range",
            message = "expected_range string must look like L10-L42 or 10-42",
        }
    end
    return nil, {
        error = "invalid_expected_range",
        message = "expected_range must be a table or string",
        actual_type = type(value),
    }
end

-- Build one normalized patch request from raw arguments.
-- 从原始参数构造一个规范化 patch 请求。
local function build_patch_request(index, raw_patch)
    local source = type(raw_patch) == "table" and raw_patch or {}
    local file_path, file_error = validate_file_argument(source.file)
    local selector, selector_error = validate_selector_argument(source.selector)
    local replacement_text, replacement_error = validate_replacement_argument(source.replacement)
    local expected_range, expected_range_error = parse_expected_range(source.expected_range)

    local initial_error = file_error or selector_error or replacement_error or expected_range_error
    if initial_error then
        initial_error.patch_index = index
    end

    return {
        patch_index = index,
        file = file_path or trim(source.file or ""),
        selector = selector or trim(source.selector or ""),
        replacement = replacement_text,
        expected_node_hash = normalize_expected_hash(source.expected_node_hash or source.expected_source_hash),
        expected_file_hash = normalize_expected_hash(source.expected_file_hash),
        expected_range = expected_range,
        initial_error = initial_error,
    }
end

-- Normalize legacy single-patch arguments and new patches[] payloads.
-- 统一规范化旧版单 patch 参数与新版 patches[] 载荷。
local function normalize_patch_requests(args)
    local request = type(args) == "table" and args or {}
    local raw_patches = request.patches
    local patches = {}
    if type(raw_patches) == "table" and #raw_patches > 0 then
        for index, raw_patch in ipairs(raw_patches) do
            table.insert(patches, build_patch_request(index, raw_patch))
        end
    else
        table.insert(patches, build_patch_request(1, request))
    end
    return patches
end

-- Return one result row for a rejected patch request.
-- 返回一个被拒绝的 patch 请求结果行。
local function build_rejected_result(patch_request, error_payload)
    return {
        patch_index = patch_request.patch_index,
        status = "rejected",
        file = patch_request.file,
        selector = patch_request.selector,
        error = tostring((error_payload and error_payload.error) or "patch_rejected"),
        message = tostring((error_payload and error_payload.message) or "patch request was rejected"),
        candidates = error_payload and error_payload.candidates or nil,
        expected_node_hash = error_payload and error_payload.expected_node_hash or nil,
        actual_node_hash = error_payload and error_payload.actual_node_hash or nil,
        expected_file_hash = error_payload and error_payload.expected_file_hash or nil,
        actual_file_hash = error_payload and error_payload.actual_file_hash or nil,
        expected_range = error_payload and error_payload.expected_range or nil,
        actual_range = error_payload and error_payload.actual_range or nil,
    }
end

-- Return one result row for a validated patch request.
-- 返回一个已通过预检的 patch 请求结果行。
local function build_validated_result(plan)
    return {
        patch_index = plan.patch_index,
        status = "validated",
        file = plan.file,
        selector = plan.selector,
        path = plan.candidate.path,
        signature = plan.candidate.signature,
        start_line = plan.candidate.start_line,
        end_line = plan.candidate.end_line,
        previous_node_hash = plan.node_hash,
    }
end

-- Sort candidate descriptors in stable file/path/range order.
-- 按稳定的文件、路径和范围顺序排序候选描述。
local function sort_candidate_descriptors(candidates)
    table.sort(candidates, function(left, right)
        if left.file ~= right.file then
            return left.file < right.file
        end
        if left.path ~= right.path then
            return left.path < right.path
        end
        return (left.start_line or 0) < (right.start_line or 0)
    end)
end

-- Load and cache file content and AST context for one file.
-- 加载并缓存单个文件的文本与 AST 上下文。
local function get_batch_file_context(file_path, helper_bundle, cache)
    if cache[file_path] then
        return cache[file_path]
    end

    local file_content, file_error = read_file_content(file_path)
    local symbol_roots, file_info, ast_error = nil, nil, nil
    if not file_error then
        symbol_roots, file_info, ast_error = collect_ast_for_file(file_path, helper_bundle)
    end

    local context = {
        file = file_path,
        content = file_content,
        file_info = file_info,
        symbol_roots = symbol_roots,
        error = file_error or ast_error,
    }
    cache[file_path] = context
    return context
end

-- Prepare one patch request by resolving its selector and validating its replacement.
-- 通过解析 selector 与校验 replacement 准备一个 patch 请求。
local function prepare_patch_request(patch_request, helper_bundle, file_context_cache)
    if patch_request.initial_error then
        return nil, patch_request.initial_error
    end

    local context = get_batch_file_context(patch_request.file, helper_bundle, file_context_cache)
    if context.error then
        return nil, context.error
    end

    if patch_request.expected_file_hash then
        local file_hash = compute_source_hash(context.content.raw)
        if patch_request.expected_file_hash ~= file_hash then
            return nil, {
                error = "stale_file_hash",
                message = "expected_file_hash does not match the current file source",
                file = patch_request.file,
                selector = patch_request.selector,
                expected_file_hash = patch_request.expected_file_hash,
                actual_file_hash = file_hash,
            }
        end
    end

    local matches = find_matching_patch_targets(context.symbol_roots, patch_request.selector)
    if #matches == 0 then
        return nil, {
            error = "selector_not_found",
            message = "no patchable function matched the selector",
            file = patch_request.file,
            selector = patch_request.selector,
        }
    end
    if #matches > 1 then
        local candidates = {}
        for _, symbol in ipairs(matches) do
            table.insert(candidates, build_candidate_descriptor(symbol))
        end
        sort_candidate_descriptors(candidates)
        return nil, {
            error = "ambiguous_selector",
            message = "multiple patchable functions matched the selector; retry with a more specific structural path",
            file = patch_request.file,
            selector = patch_request.selector,
            candidates = candidates,
        }
    end

    local symbol = matches[1]
    local source_text, source_error = extract_symbol_source(context.content, symbol)
    if source_error then
        return nil, source_error
    end

    local node_hash = compute_source_hash(source_text)
    if patch_request.expected_node_hash and patch_request.expected_node_hash ~= node_hash then
        return nil, {
            error = "stale_node_hash",
            message = "expected_node_hash does not match the current node source",
            file = patch_request.file,
            selector = patch_request.selector,
            expected_node_hash = patch_request.expected_node_hash,
            actual_node_hash = node_hash,
        }
    end

    if patch_request.expected_range then
        local expected_start = tonumber(patch_request.expected_range.start_line)
        local expected_end = tonumber(patch_request.expected_range.end_line)
        if expected_start ~= tonumber(symbol.start_line) or expected_end ~= tonumber(symbol.end_line) then
            return nil, {
                error = "stale_node_range",
                message = "expected_range does not match the current node range",
                file = patch_request.file,
                selector = patch_request.selector,
                expected_range = patch_request.expected_range,
                actual_range = {
                    start_line = symbol.start_line,
                    end_line = symbol.end_line,
                },
            }
        end
    end

    local replacement_shape_error = validate_full_replacement_shape(symbol, patch_request.replacement)
    if replacement_shape_error then
        return nil, replacement_shape_error
    end

    local replacement_lines = build_full_replacement_lines(symbol, context.content.lines, patch_request.replacement)
    local candidate = build_candidate_descriptor(symbol)
    return {
        patch_index = patch_request.patch_index,
        file = patch_request.file,
        selector = patch_request.selector,
        symbol = symbol,
        candidate = candidate,
        node_hash = node_hash,
        source_text = source_text,
        replacement_lines = replacement_lines,
        file_context = context,
    }, nil
end

-- Add a problem to one prepared plan and update its result row.
-- 向一个已准备计划追加问题并更新其结果行。
local function reject_plan(plan, results_by_index, error_payload)
    plan.rejected = true
    results_by_index[plan.patch_index] = build_rejected_result({
        patch_index = plan.patch_index,
        file = plan.file,
        selector = plan.selector,
    }, error_payload)
end

-- Detect overlapping source ranges within the same target file.
-- 检测同一目标文件内互相重叠的源码范围。
local function reject_overlapping_plans(plans_by_file, results_by_index)
    for _, plans in pairs(plans_by_file) do
        table.sort(plans, function(left, right)
            return (left.symbol.start_line or 0) < (right.symbol.start_line or 0)
        end)
        local previous = nil
        for _, plan in ipairs(plans) do
            if previous and (tonumber(plan.symbol.start_line) or 0) <= (tonumber(previous.symbol.end_line) or 0) then
                local overlap_error = {
                    error = "overlapping_patch_nodes",
                    message = "multiple patches target overlapping source ranges in the same file",
                    file = plan.file,
                    selector = plan.selector,
                }
                reject_plan(previous, results_by_index, overlap_error)
                reject_plan(plan, results_by_index, overlap_error)
            end
            previous = plan
        end
    end
end

-- Apply all replacements for one file to an in-memory line array.
-- 将一个文件的全部 replacement 应用到内存行数组。
local function build_batch_file_lines(file_content, plans)
    local rebuilt = clone_array(file_content.lines)
    table.sort(plans, function(left, right)
        return (left.symbol.start_line or 0) > (right.symbol.start_line or 0)
    end)
    for _, plan in ipairs(plans) do
        rebuilt = build_replaced_file_lines(rebuilt, plan.symbol, plan.replacement_lines)
    end
    return rebuilt
end

-- Validate a patched file against parser errors and every original target identity.
-- 校验 patch 后文件的解析错误与每个原始目标身份。
local function validate_ast_after_write_for_plans(file_path, helper_bundle, plans)
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

    local relocated_by_index = {}
    for _, plan in ipairs(plans or {}) do
        local identity_selector = build_symbol_identity_path(plan.symbol)
        local matches = find_matching_patch_targets(symbol_roots, identity_selector)
        if #matches == 0 then
            return nil, {
                error = "patched_target_not_found",
                message = "patched file no longer contains the target function under the original structural path",
                file = file_path,
                selector = identity_selector,
                patch_index = plan.patch_index,
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
                patch_index = plan.patch_index,
                candidates = candidates,
            }
        end

        local relocated_symbol = matches[1]
        if relocated_symbol.kind ~= plan.symbol.kind or trim(relocated_symbol.name or "") ~= trim(plan.symbol.name or "") then
            return nil, {
                error = "patched_target_identity_changed",
                message = "patched file changed the target function identity and was rejected",
                file = file_path,
                patch_index = plan.patch_index,
                details = {
                    expected = {
                        kind = plan.symbol.kind,
                        name = trim(plan.symbol.name or ""),
                    },
                    actual = {
                        kind = relocated_symbol.kind,
                        name = trim(relocated_symbol.name or ""),
                    },
                },
            }
        end
        relocated_by_index[plan.patch_index] = relocated_symbol
    end

    return relocated_by_index, nil
end

-- Create and validate a temporary patched file for one target file.
-- 为一个目标文件创建并校验临时 patch 文件。
local function create_validated_patch_record(file_path, plans, helper_bundle)
    local file_context = plans[1].file_context
    local new_lines = build_batch_file_lines(file_context.content, plans)
    local new_text = join_file_lines(new_lines, file_context.content.newline, file_context.content.has_trailing_newline)
    local temp_path = build_sidecar_file_path(file_path, "vmcp_patch_batch_tmp")
    local backup_path = build_sidecar_file_path(file_path, "vmcp_patch_batch_backup")
    safe_remove_file(temp_path)
    safe_remove_file(backup_path)

    local temp_written, temp_write_error = pcall(vulcan.fs.write, temp_path, new_text)
    if not temp_written then
        safe_remove_file(temp_path)
        return nil, {
            error = "temp_file_write_failed",
            message = tostring(temp_write_error),
            file = file_path,
            temp_file = temp_path,
        }
    end

    local relocated_by_index, validation_error = validate_ast_after_write_for_plans(temp_path, helper_bundle, plans)
    if validation_error then
        safe_remove_file(temp_path)
        return nil, validation_error
    end

    -- Compute post-patch node hashes from the validated temporary source so success metadata can drive later stale checks.
    -- 从已校验的临时源码计算 patch 后节点哈希，确保成功元数据可继续用于后续 stale check。
    local new_file_content = {
        raw = new_text,
        lines = new_lines,
        newline = file_context.content.newline,
        has_trailing_newline = file_context.content.has_trailing_newline,
    }
    local new_node_hash_by_index = {}
    for _, plan in ipairs(plans or {}) do
        local relocated_symbol = relocated_by_index and relocated_by_index[plan.patch_index]
        local new_source_text, new_source_error = extract_symbol_source(new_file_content, relocated_symbol)
        if new_source_error then
            safe_remove_file(temp_path)
            return nil, {
                error = "patched_node_source_extract_failed",
                message = "patched node source could not be extracted after AST validation",
                file = file_path,
                patch_index = plan.patch_index,
                details = new_source_error,
            }
        end
        new_node_hash_by_index[plan.patch_index] = compute_source_hash(new_source_text)
    end

    return {
        file = file_path,
        temp_path = temp_path,
        backup_path = backup_path,
        plans = plans,
        original_raw = file_context.content.raw,
        relocated_by_index = relocated_by_index,
        new_node_hash_by_index = new_node_hash_by_index,
    }, nil
end

-- Restore all committed records from their backup files.
-- 从备份文件恢复所有已提交记录。
local function rollback_patch_records(records)
    local rollback_errors = {}
    for _, record in ipairs(records or {}) do
        safe_remove_file(record.file)
        local restored, restore_error = rename_file(record.backup_path, record.file)
        if not restored then
            local fallback_ok, fallback_error = pcall(vulcan.fs.write, record.file, record.original_raw or "")
            if not fallback_ok then
                table.insert(rollback_errors, {
                    file = record.file,
                    restore_error = tostring(restore_error),
                    fallback_error = tostring(fallback_error),
                })
            end
        end
    end
    return rollback_errors
end

-- Commit a set of pre-validated patch records, rolling back on any failure.
-- 提交一组已预校验的 patch 记录，并在失败时回滚。
local function commit_patch_records(records, helper_bundle)
    local committed = {}
    for _, record in ipairs(records or {}) do
        local moved_to_backup, backup_error = rename_file(record.file, record.backup_path)
        if not moved_to_backup then
            rollback_patch_records(committed)
            safe_remove_file(record.temp_path)
            return {
                error = "backup_creation_failed",
                message = tostring(backup_error),
                file = record.file,
                backup_file = record.backup_path,
            }
        end

        local moved_temp_into_place, swap_error = rename_file(record.temp_path, record.file)
        if not moved_temp_into_place then
            table.insert(committed, record)
            rollback_patch_records(committed)
            safe_remove_file(record.temp_path)
            return {
                error = "temp_swap_failed",
                message = tostring(swap_error),
                file = record.file,
                backup_file = record.backup_path,
            }
        end
        table.insert(committed, record)
    end

    for _, record in ipairs(records or {}) do
        local relocated_by_index, validation_error = validate_ast_after_write_for_plans(record.file, helper_bundle, record.plans)
        if validation_error then
            local rollback_errors = rollback_patch_records(committed)
            return {
                error = "patch_reverted_after_validation_failure",
                message = "patched files failed AST validation and original files were restored",
                file = record.file,
                validation = validation_error,
                rollback_errors = rollback_errors,
            }
        end
        record.relocated_by_index = relocated_by_index
    end

    for _, record in ipairs(records or {}) do
        safe_remove_file(record.backup_path)
    end
    return nil
end

-- Build patch plans grouped by target file after validation.
-- 在校验后按目标文件构建 patch 计划分组。
local function build_valid_plan_groups(plans)
    local groups = {}
    for _, plan in ipairs(plans or {}) do
        if not plan.rejected then
            if not groups[plan.file] then
                groups[plan.file] = {}
            end
            table.insert(groups[plan.file], plan)
        end
    end
    return groups
end

-- Render a stale-check range diagnostic as a compact line value.
-- 将 stale check 的范围诊断渲染为紧凑的行内值。
local function format_range_diagnostic(range_value)
    if type(range_value) ~= "table" then
        return tostring(range_value or "")
    end
    local start_line = tonumber(range_value.start_line or range_value.start or range_value[1])
    local end_line = tonumber(range_value.end_line or range_value["end"] or range_value[2] or start_line)
    if start_line and end_line then
        return string.format("L%d-%d", start_line, end_line)
    end
    local ok, encoded = pcall(vulcan.json.encode, range_value)
    if ok and encoded then
        return tostring(encoded)
    end
    return tostring(range_value)
end

-- Render the batch patch result as Markdown.
-- 将批量 patch 结果渲染为 Markdown。
local function render_patch_batch_result(summary, results)
    local lines = {
        "# PATCH BATCH RESULT",
        string.format("- requested: `%d`", tonumber(summary.requested) or 0),
        string.format("- applied: `%d`", tonumber(summary.applied) or 0),
        string.format("- status: `%s`", tostring(summary.status or "unknown")),
        string.format("- atomic: `%s`", tostring(summary.atomic == true)),
        string.format("- reason: `%s`", tostring(summary.reason or "")),
    }

    table.insert(lines, "")
    table.insert(lines, "## Patches")
    for _, result in ipairs(results or {}) do
        table.insert(lines, "")
        table.insert(lines, string.format("### Patch %d", tonumber(result.patch_index) or 0))
        table.insert(lines, string.format("- status: `%s`", tostring(result.status or "unknown")))
        table.insert(lines, string.format("- file: `%s`", tostring(result.file or "")))
        table.insert(lines, string.format("- selector: `%s`", tostring(result.selector or "")))
        if result.path then
            table.insert(lines, string.format("- path: `%s`", tostring(result.path)))
        end
        if result.signature then
            table.insert(lines, string.format("- signature: `%s`", tostring(result.signature)))
        end
        if result.start_line and result.end_line then
            table.insert(lines, string.format("- lines: `L%d-%d`", tonumber(result.start_line) or 0, tonumber(result.end_line) or 0))
        end
        if result.previous_node_hash then
            table.insert(lines, string.format("- previous_node_hash: `%s`", tostring(result.previous_node_hash)))
        end
        if result.new_node_hash then
            table.insert(lines, string.format("- new_node_hash: `%s`", tostring(result.new_node_hash)))
        elseif result.node_hash then
            table.insert(lines, string.format("- node_hash: `%s`", tostring(result.node_hash)))
        end
        if result.error then
            table.insert(lines, string.format("- error: `%s`", tostring(result.error)))
        end
        if result.message then
            table.insert(lines, string.format("- message: %s", tostring(result.message)))
        end
        if result.expected_node_hash then
            table.insert(lines, string.format("- expected_node_hash: `%s`", tostring(result.expected_node_hash)))
        end
        if result.actual_node_hash then
            table.insert(lines, string.format("- actual_node_hash: `%s`", tostring(result.actual_node_hash)))
        end
        if result.expected_file_hash then
            table.insert(lines, string.format("- expected_file_hash: `%s`", tostring(result.expected_file_hash)))
        end
        if result.actual_file_hash then
            table.insert(lines, string.format("- actual_file_hash: `%s`", tostring(result.actual_file_hash)))
        end
        if result.expected_range then
            table.insert(lines, string.format("- expected_range: `%s`", format_range_diagnostic(result.expected_range)))
        end
        if result.actual_range then
            table.insert(lines, string.format("- actual_range: `%s`", format_range_diagnostic(result.actual_range)))
        end
        if type(result.candidates) == "table" and #result.candidates > 0 then
            table.insert(lines, "- candidates:")
            for _, candidate in ipairs(result.candidates) do
                table.insert(
                    lines,
                    string.format(
                        "  - `%s` L%d-%d",
                        tostring(candidate.path or ""),
                        tonumber(candidate.start_line) or 0,
                        tonumber(candidate.end_line) or 0
                    )
                )
            end
        end
    end

    return table.concat(lines, "\n")
end

-- Mark validated plans as not applied because atomic validation failed.
-- 因 atomic 校验失败而将已通过预检的计划标记为未应用。
local function mark_validated_results_not_applied(results_by_index)
    for _, result in pairs(results_by_index or {}) do
        if result.status == "validated" then
            result.status = "not_applied"
            result.message = "atomic batch was rejected before writing any file"
        end
    end
end

-- Convert indexed result map into a stable array.
-- 将索引结果映射转换为稳定数组。
local function collect_ordered_results(results_by_index, requested_count)
    local results = {}
    for index = 1, requested_count do
        if results_by_index[index] then
            table.insert(results, results_by_index[index])
        end
    end
    return results
end

-- Execute a batch patch request with atomic or partial application semantics.
-- 按 atomic 或部分应用语义执行批量 patch 请求。
local function execute_patch_batch(args, helper_bundle)
    local atomic = normalize_atomic_argument(args and args.atomic)
    local max_patches = normalize_max_patches_argument(args and args.max_patches)
    local patch_requests = normalize_patch_requests(args)
    local results_by_index = {}
    local plans = {}
    local file_context_cache = {}

    for _, patch_request in ipairs(patch_requests) do
        if patch_request.patch_index > max_patches then
            results_by_index[patch_request.patch_index] = {
                patch_index = patch_request.patch_index,
                status = "skipped",
                file = patch_request.file,
                selector = patch_request.selector,
                message = "patch request skipped because max_patches was reached",
            }
        else
            local plan, prepare_error = prepare_patch_request(patch_request, helper_bundle, file_context_cache)
            if prepare_error then
                results_by_index[patch_request.patch_index] = build_rejected_result(patch_request, prepare_error)
            else
                table.insert(plans, plan)
                results_by_index[patch_request.patch_index] = build_validated_result(plan)
            end
        end
    end

    local plans_by_file = build_valid_plan_groups(plans)
    reject_overlapping_plans(plans_by_file, results_by_index)
    plans_by_file = build_valid_plan_groups(plans)

    local has_rejections = false
    for _, result in pairs(results_by_index) do
        if result.status == "rejected" or result.status == "skipped" then
            has_rejections = true
            break
        end
    end

    local summary = {
        requested = #patch_requests,
        applied = 0,
        status = "validated",
        atomic = atomic,
        reason = "",
    }

    if atomic and has_rejections then
        mark_validated_results_not_applied(results_by_index)
        summary.status = "rejected"
        summary.reason = "validation_failed"
        return render_patch_batch_result(summary, collect_ordered_results(results_by_index, #patch_requests))
    end

    local records = {}
    local record_errors = {}
    for file_path, file_plans in pairs(plans_by_file) do
        if #file_plans > 0 then
            local record, record_error = create_validated_patch_record(file_path, file_plans, helper_bundle)
            if record_error then
                table.insert(record_errors, {
                    file = file_path,
                    error = record_error,
                    plans = file_plans,
                })
            else
                table.insert(records, record)
            end
        end
    end

    if #record_errors > 0 then
        if atomic then
            for _, record in ipairs(records) do
                safe_remove_file(record.temp_path)
            end
            for _, record_error in ipairs(record_errors) do
                for _, plan in ipairs(record_error.plans or {}) do
                    results_by_index[plan.patch_index] = build_rejected_result({
                        patch_index = plan.patch_index,
                        file = plan.file,
                        selector = plan.selector,
                    }, record_error.error)
                end
            end
            mark_validated_results_not_applied(results_by_index)
            summary.status = "rejected"
            summary.reason = "temp_validation_failed"
            return render_patch_batch_result(summary, collect_ordered_results(results_by_index, #patch_requests))
        end
        for _, record_error in ipairs(record_errors) do
            for _, plan in ipairs(record_error.plans or {}) do
                results_by_index[plan.patch_index] = build_rejected_result({
                    patch_index = plan.patch_index,
                    file = plan.file,
                    selector = plan.selector,
                }, record_error.error)
            end
        end
    end

    if atomic then
        table.sort(records, function(left, right)
            return left.file < right.file
        end)
        local commit_error = commit_patch_records(records, helper_bundle)
        if commit_error then
            for _, record in ipairs(records) do
                safe_remove_file(record.temp_path)
            end
            for _, result in pairs(results_by_index) do
                if result.status == "validated" then
                    result.status = "not_applied"
                    result.error = commit_error.error
                    result.message = commit_error.message
                end
            end
            summary.status = "rejected"
            summary.reason = tostring(commit_error.error or "commit_failed")
            return render_patch_batch_result(summary, collect_ordered_results(results_by_index, #patch_requests))
        end
        for _, record in ipairs(records) do
            for _, plan in ipairs(record.plans or {}) do
                local relocated = record.relocated_by_index and record.relocated_by_index[plan.patch_index] or plan.symbol
                local new_node_hash = record.new_node_hash_by_index and record.new_node_hash_by_index[plan.patch_index] or nil
                results_by_index[plan.patch_index] = {
                    patch_index = plan.patch_index,
                    status = "applied",
                    file = plan.file,
                    selector = plan.selector,
                    path = build_canonical_symbol_path(relocated),
                    signature = trim(relocated.signature or ""),
                    start_line = relocated.start_line,
                    end_line = relocated.end_line,
                    previous_node_hash = plan.node_hash,
                    new_node_hash = new_node_hash,
                    node_hash = new_node_hash,
                }
                summary.applied = summary.applied + 1
            end
        end
        summary.status = "applied"
        summary.reason = "ok"
        return render_patch_batch_result(summary, collect_ordered_results(results_by_index, #patch_requests))
    end

    for _, record in ipairs(records) do
        local commit_error = commit_patch_records({ record }, helper_bundle)
        if commit_error then
            safe_remove_file(record.temp_path)
            for _, plan in ipairs(record.plans or {}) do
                results_by_index[plan.patch_index] = build_rejected_result({
                    patch_index = plan.patch_index,
                    file = plan.file,
                    selector = plan.selector,
                }, commit_error)
            end
        else
            for _, plan in ipairs(record.plans or {}) do
                local relocated = record.relocated_by_index and record.relocated_by_index[plan.patch_index] or plan.symbol
                local new_node_hash = record.new_node_hash_by_index and record.new_node_hash_by_index[plan.patch_index] or nil
                results_by_index[plan.patch_index] = {
                    patch_index = plan.patch_index,
                    status = "applied",
                    file = plan.file,
                    selector = plan.selector,
                    path = build_canonical_symbol_path(relocated),
                    signature = trim(relocated.signature or ""),
                    start_line = relocated.start_line,
                    end_line = relocated.end_line,
                    previous_node_hash = plan.node_hash,
                    new_node_hash = new_node_hash,
                    node_hash = new_node_hash,
                }
                summary.applied = summary.applied + 1
            end
        end
    end

    local has_failure = false
    for _, result in pairs(results_by_index) do
        if result.status ~= "applied" then
            has_failure = true
            break
        end
    end
    summary.status = has_failure and "partial" or "applied"
    summary.reason = has_failure and "partial_application" or "ok"
    return render_patch_batch_result(summary, collect_ordered_results(results_by_index, #patch_requests))
end

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    -- Keep helper functions as direct closure upvalues for sibling CodeKit entries.
    -- 为同级 CodeKit 入口保留 helper 函数作为直接闭包 upvalue。
    if args and args.__codekit_helper_probe == "__never__" then
        return {
            validate_file_argument = validate_file_argument,
            validate_selector_argument = validate_selector_argument,
            collect_ast_for_file = collect_ast_for_file,
            find_matching_patch_targets = find_matching_patch_targets,
            build_candidate_descriptor = build_candidate_descriptor,
        }
    end

    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return render_patch_error(helper_error)
    end
    return execute_patch_batch(args, helper_bundle)
end
