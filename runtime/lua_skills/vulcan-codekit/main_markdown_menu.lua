--[[
codekit-markdown-menu
中文：扫描目录、文件或混合路径中的 Markdown 文件，只提取 `#`、`##`、`###` 标题及其行号，生成适合快速筛选文档的菜单视图。
English: Scan directories, files, or mixed path sets for Markdown files and extract only `#`, `##`, and `###` headings with line numbers, producing a menu-oriented view for fast document triage.
]]

local MAX_MATCHED_FILES = 5000
local LFS_MODULE = nil
local AST_RUNTIME_HELPERS = nil

--[[
中文：去除字符串首尾空白，作为最基础的文本规整工具。
English: Trim leading and trailing whitespace as the most basic text-normalization helper.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
中文：判断字符串是否以前缀开头，用于路径和标题匹配。
English: Check whether a string starts with a given prefix for path and heading matching.
]]
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
中文：按统一换行符拆分文本，便于逐行解析 Markdown 内容。
English: Split text after normalizing line endings so Markdown content can be processed line by line.
]]
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
中文：浅拷贝数组，避免在规则传递和结果渲染中原地修改原始列表。
English: Create a shallow array copy so rule propagation and rendering do not mutate the source list in place.
]]
local function clone_array(items)
    local copied = {}
    for _, item in ipairs(items or {}) do
        table.insert(copied, item)
    end
    return copied
end

--[[
中文：获取当前 skill 目录，优先使用宿主注入路径。
English: Resolve the current skill directory, preferring the host-injected path.
]]
local function get_skill_dir()
    return __skill_dir_codekit_markdown_menu or __skill_dir_ast_grep or "."
end

--[[
中文：从 `codekit-ast` 入口闭包中按名称提取内部助手函数，避免重复复制路径和忽略规则逻辑。
English: Extract internal helpers from the `codekit-ast` closure by name so path and ignore logic can be reused instead of duplicated.
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
中文：懒加载 `codekit-ast` 运行时助手，确保新工具在路径解析、忽略规则和参数校验上保持一致。
English: Lazily load `codekit-ast` runtime helpers so the new tool stays aligned on path resolution, ignore rules, and argument validation.
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
        validate_path_argument = extract_upvalue_by_name(ast_entry, "validate_path_argument"),
        validate_recursive_argument = extract_upvalue_by_name(ast_entry, "validate_recursive_argument"),
        validate_noignore_argument = extract_upvalue_by_name(ast_entry, "validate_noignore_argument"),
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
中文：懒加载 LuaFileSystem，若不可用则返回 nil。
English: Lazily load LuaFileSystem and return nil when it is unavailable.
]]
local function get_lfs_module()
    if LFS_MODULE ~= nil then
        return LFS_MODULE
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
中文：规范化文件路径键，主要用于去重；Windows 下按不区分大小写处理。
English: Normalize a file-path key for deduplication, handling Windows paths case-insensitively.
]]
local function normalize_file_key(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    if vulcan.osinfo().os == "windows" then
        normalized = normalized:lower()
    end
    return normalized
end

--[[
中文：判断路径是否为绝对路径，兼容 Windows 盘符、UNC 路径与 Unix 风格绝对路径。
English: Check whether a path is absolute, supporting Windows drive paths, UNC paths, and Unix-style absolute paths.
]]
local function is_absolute_path(path)
    local normalized = tostring(path or "")
    return normalized:match("^%a:[/\\]") ~= nil
        or starts_with(normalized, "\\\\")
        or starts_with(normalized, "/")
end

--[[
中文：获取当前工作目录，优先使用 LuaFileSystem；缺失时回退到 "."。
English: Resolve the current working directory, preferring LuaFileSystem and falling back to "." when unavailable.
]]
local function get_current_working_directory()
    local lfs = get_lfs_module()
    if lfs and type(lfs.currentdir) == "function" then
        local ok, current = pcall(lfs.currentdir)
        if ok and type(current) == "string" and trim(current) ~= "" then
            return current
        end
    end
    return "."
end

--[[
中文：将相对路径解析为当前工作目录下的绝对路径；绝对路径保持原样返回。
English: Resolve a relative path against the current working directory while leaving absolute paths untouched.
]]
local function resolve_scan_path(path)
    local normalized = tostring(path or "")
    if normalized == "" or is_absolute_path(normalized) then
        return normalized
    end
    return vulcan.path_join(get_current_working_directory(), normalized)
end

--[[
中文：定位 ripgrep 可执行文件，优先使用 ast-grep 技能依赖目录中的 `rg`。
English: Resolve the ripgrep executable, preferring the `rg` binary installed in the ast-grep skill dependency directory.
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
        message = "ripgrep binary not found for codekit-markdown-menu",
        expected_path = full_path,
    }
end

--[[
中文：通过 ripgrep 列出目录下的 Markdown 文件，复用其递归与忽略规则能力。
English: List Markdown files under a directory via ripgrep so recursion and ignore-rule behavior can be reused.
]]
local function list_markdown_files_with_rg(directory_path, recursive, ignore_enabled)
    local rg_binary_path, binary_error = find_rg_binary()
    if binary_error then
        return nil, binary_error
    end

    local arguments = {
        "--files",
        directory_path,
        "-g",
        "*.md",
    }
    if not recursive then
        table.insert(arguments, "--max-depth")
        table.insert(arguments, "1")
    end
    if not ignore_enabled then
        table.insert(arguments, "--no-ignore")
        table.insert(arguments, "--hidden")
    end

    local ok, result = pcall(vulcan.exec, {
        program = rg_binary_path,
        args = arguments,
        timeout_ms = 30000,
    })
    if not ok then
        return nil, {
            error = "rg_exec_failed",
            message = tostring(result),
            dir = directory_path,
        }
    end
    if result and result.timed_out then
        return nil, {
            error = "rg_timed_out",
            message = "ripgrep execution timed out",
            dir = directory_path,
        }
    end
    if result and result.error then
        return nil, {
            error = "rg_exec_failed",
            message = tostring(result.error),
            dir = directory_path,
        }
    end
    if result and result.code and tonumber(result.code) ~= 0 then
        return nil, {
            error = "rg_exec_failed",
            message = tostring(result.stderr or result.stdout or "ripgrep execution failed"),
            dir = directory_path,
            exit_code = tonumber(result.code),
        }
    end

    local files = {}
    for _, line in ipairs(split_lines(result and result.stdout or "")) do
        local normalized = trim(line)
        if normalized ~= "" then
            local absolute_path = is_absolute_path(normalized) and normalized or vulcan.path_join(directory_path, normalized)
            table.insert(files, {
                path = absolute_path,
                source = directory_path,
            })
        end
    end
    return files, nil
end

--[[
中文：判断文件是否为 Markdown 文件；当前工具仅扫描 `.md` 扩展名。
English: Check whether a file is Markdown; this tool intentionally scans only `.md` files.
]]
local function is_markdown_file(path)
    local extension = tostring(path or ""):match("%.([^.]+)$")
    return extension ~= nil and extension:lower() == "md"
end

--[[
中文：收集单个目标路径下的 Markdown 文件，支持文件模式与目录模式。
English: Collect Markdown files under one target path, supporting both file mode and directory mode.
]]
local function collect_markdown_files_for_path(target_path, recursive, ignore_enabled, helpers)
    local collected = {}
    local scan_root = resolve_scan_path(target_path)
    local is_directory = vulcan.fs_is_dir(scan_root)

    if not vulcan.fs_exists(scan_root) then
        return nil, nil, {
            error = "path_not_found",
            message = "path does not exist",
            path = target_path,
        }
    end

    if not is_directory then
        if not is_markdown_file(scan_root) then
            return nil, nil, {
                error = "unsupported_markdown_path",
                message = "path points to a file whose extension is not .md",
                path = target_path,
            }
        end
        table.insert(collected, {
            path = scan_root,
            source = target_path,
        })
        return collected, "file", nil
    end

    local directory_files, directory_error = list_markdown_files_with_rg(scan_root, recursive, ignore_enabled)
    if directory_error then
        return nil, nil, directory_error
    end
    for _, file_info in ipairs(directory_files or {}) do
        table.insert(collected, file_info)
    end
    return collected, "directory", nil
end

--[[
中文：聚合多个输入路径的 Markdown 文件结果，允许目录与文件混用并按绝对路径去重。
English: Aggregate Markdown files across multiple input paths, allowing mixed file/directory mode and deduplicating by absolute path.
]]
local function collect_markdown_files(target_paths, recursive, ignore_enabled, helpers)
    local collected = {}
    local seen = {}
    local errors = {}

    for _, target_path in ipairs(target_paths or {}) do
        local path_files, _, collection_error = collect_markdown_files_for_path(target_path, recursive, ignore_enabled, helpers)
        if collection_error then
            table.insert(errors, collection_error)
        else
            for _, file_info in ipairs(path_files or {}) do
                local file_key = normalize_file_key(file_info.path)
                if not seen[file_key] then
                    seen[file_key] = true
                    table.insert(collected, file_info)
                    if #collected > MAX_MATCHED_FILES then
                        return nil, errors, {
                            error = "too_many_matched_files",
                            message = "Matched markdown files exceed 5000. Narrow the path scope or provide a smaller file set.",
                            limit = MAX_MATCHED_FILES,
                            matched_files = #collected,
                        }
                    end
                end
            end
        end
    end

    table.sort(collected, function(left, right)
        return tostring(left.path or "") < tostring(right.path or "")
    end)
    return collected, errors, nil
end

--[[
中文：规范化标题文本，移除末尾装饰性 `#` 和多余空白，确保目录节点简洁稳定。
English: Normalize heading text by removing trailing decorative `#` markers and extra whitespace so menu nodes stay clean and stable.
]]
local function normalize_heading_text(heading_text)
    local normalized = trim(heading_text or "")
    normalized = normalized:gsub("%s+#+%s*$", "")
    return trim(normalized)
end

--[[
中文：识别 Markdown 代码围栏行，避免把代码块中的井号误判成文档标题。
English: Detect Markdown fenced-code lines so hash signs inside code blocks are not mistaken for document headings.
]]
local function detect_fence_marker(line)
    local trimmed_line = trim(line)
    if trimmed_line:match("^```") then
        return "```"
    end
    if trimmed_line:match("^~~~") then
        return "~~~"
    end
    return nil
end

--[[
中文：从 Markdown 文件中提取 `#`、`##`、`###` 标题及其行号，并跳过代码围栏区域。
English: Extract `#`, `##`, and `###` headings with line numbers from a Markdown file while skipping fenced code blocks.
]]
local function extract_markdown_headings(file_path)
    local ok, file_content = pcall(vulcan.fs_read, file_path)
    if not ok then
        return nil, {
            error = "markdown_read_failed",
            message = tostring(file_content),
            path = file_path,
        }
    end

    local headings = {}
    local active_fence = nil
    for index, line in ipairs(split_lines(file_content)) do
        local fence_marker = detect_fence_marker(line)
        if fence_marker then
            if active_fence == nil then
                active_fence = fence_marker
            elseif active_fence == fence_marker then
                active_fence = nil
            end
        elseif active_fence == nil then
            local hashes, heading_text = tostring(line or ""):match("^%s*(#+)%s+(.+)$")
            if hashes and heading_text then
                local normalized_heading = normalize_heading_text(heading_text)
                if normalized_heading ~= "" and #hashes <= 3 then
                    table.insert(headings, {
                        level = #hashes,
                        line = index,
                        text = normalized_heading,
                    })
                end
            end
        end
    end

    return headings, nil
end

--[[
中文：将扫描统计、文件菜单和标题目录详情渲染成单段 Markdown 文本，便于模型直接阅读而无需再解析结构体包装。
English: Render scan statistics, the file menu, and heading details into a single Markdown text block so models can read it directly without unpacking a wrapper table.
]]
local function build_markdown_menu_content(documents, stats)
    local lines = {
        "# SCAN SUMMARY",
        string.format("- files_scanned: %d", tonumber(stats and stats.files_scanned) or 0),
        string.format("- files_with_headings: %d", tonumber(stats and stats.files_with_headings) or 0),
        string.format("- heading_items: %d", tonumber(stats and stats.items_found) or 0),
        string.format("- errors: %d", tonumber(stats and stats.error_count) or 0),
        "",
        "# FILE MENU",
    }

    if #(documents or {}) == 0 then
        table.insert(lines, "(no markdown files found)")
    else
        for index, document in ipairs(documents) do
            table.insert(lines, string.format("%d. %s", index, tostring(document.path or "")))
        end
    end

    table.insert(lines, "")
    table.insert(lines, "如果内容过多产生了截断，可根据目录判断需要的范围，重新调用本工具精确获取。")
    table.insert(lines, "If the result is truncated, use the file menu to narrow the path scope and call this tool again with a smaller target set.")
    table.insert(lines, "")
    table.insert(lines, "# Markdown Details")

    if #(documents or {}) == 0 then
        table.insert(lines, "> (no markdown headings found)")
        return table.concat(lines, "\n")
    end

    for index, document in ipairs(documents) do
        table.insert(lines, "")
        table.insert(lines, string.format("## [%d] %s", index, tostring(document.path or "")))
        if #(document.headings or {}) == 0 then
            table.insert(lines, "> (no # / ## / ### headings found)")
        else
            for _, heading in ipairs(document.headings or {}) do
                table.insert(lines, string.format("> L%d | %s %s", tonumber(heading.line) or 0, string.rep("#", tonumber(heading.level) or 1), tostring(heading.text or "")))
            end
        end
    end

    return table.concat(lines, "\n")
end

-- 技能入口 / Skill entry point invoked by the MCP host runtime.
return function(args)
    local helpers, helpers_error = load_ast_runtime_helpers()
    if helpers_error then
        return helpers_error
    end

    local target_paths, path_error = helpers.validate_path_argument(args and args.path)
    if path_error then
        return path_error
    end

    local recursive, recursive_error = helpers.validate_recursive_argument(args and args.recursive)
    if recursive_error then
        return recursive_error
    end

    local ignore_enabled, ignore_error = helpers.validate_noignore_argument(args and args.noignore)
    if ignore_error then
        return ignore_error
    end

    local markdown_files, collection_errors, collection_error = collect_markdown_files(target_paths, recursive, ignore_enabled, helpers)
    if collection_error then
        return collection_error
    end

    local documents = {}
    local files_with_headings = 0
    local headings_found = 0
    local read_errors = clone_array(collection_errors)

    for _, file_info in ipairs(markdown_files or {}) do
        local headings, heading_error = extract_markdown_headings(file_info.path)
        if heading_error then
            table.insert(read_errors, heading_error)
        else
            if #(headings or {}) > 0 then
                files_with_headings = files_with_headings + 1
                headings_found = headings_found + #headings
            end
            table.insert(documents, {
                path = file_info.path,
                headings = headings or {},
            })
        end
    end

    return build_markdown_menu_content(documents, {
        files_scanned = #(markdown_files or {}),
        files_with_headings = files_with_headings,
        items_found = headings_found,
        error_count = #(read_errors or {}),
    })
end
