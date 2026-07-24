--[[
codekit-markdown-menu
扫描目录、文件或混合路径中的 Markdown 文件，只提取 `#`、`##`、`###` 标题及其行号，生成适合快速筛选文档的菜单视图。
Scan directories, files, or mixed path sets for Markdown files and extract only `#`, `##`, and `###` headings with line numbers, producing a menu-oriented view for fast document triage.
]]

local MAX_MATCHED_FILES = 5000
local LFS_MODULE = nil
local AST_RUNTIME_HELPERS = nil
local SHARED_LENGTH_HELPERS = nil
--[[
去除字符串首尾空白，作为最基础的文本规整工具。
Trim leading and trailing whitespace as the most basic text-normalization helper.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
判断字符串是否以前缀开头，用于路径和标题匹配。
Check whether a string starts with a given prefix for path and heading matching.
]]
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
按统一换行符拆分文本，便于逐行解析 Markdown 内容。
Split text after normalizing line endings so Markdown content can be processed line by line.
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
浅拷贝数组，避免在规则传递和结果渲染中原地修改原始列表。
Create a shallow array copy so rule propagation and rendering do not mutate the source list in place.
]]
local function clone_array(items)
    local copied = {}
    for _, item in ipairs(items or {}) do
        table.insert(copied, item)
    end
    return copied
end

--[[
获取当前 skill 目录，优先使用宿主注入路径。
Resolve the current skill directory, preferring the host-injected path.
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
懒加载共享长度规则模块，让 markdown-menu 与其他 codekit 工具复用同一套预算模型。
Lazily load the shared length-policy module so markdown-menu reuses the same budget model as other codekit tools.
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
在单次工具调用开始时初始化 markdown-menu 当前使用的预算。
Initialize the current budget used by markdown-menu at the start of one tool call.
]]
local function initialize_markdown_menu_budget()
    local helpers, helper_error = load_shared_length_helpers()
    if helper_error then
        return nil, helper_error
    end
    return helpers.initialize_client_budget(vulcan)
end

--[[
从 `codekit-ast-detail` 入口闭包中按名称提取内部助手函数，避免重复复制路径和忽略规则逻辑。
Extract internal helpers from the `codekit-ast-detail` closure by name so path and ignore logic can be reused instead of duplicated.
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
懒加载 `codekit-ast-detail` 运行时助手，确保新工具在路径解析、忽略规则和参数校验上保持一致。
Lazily load `codekit-ast-detail` runtime helpers so the new tool stays aligned on path resolution, ignore rules, and argument validation.
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
        validate_path_argument = extract_upvalue_by_name(ast_entry, "validate_path_argument"),
        validate_recursive_argument = extract_upvalue_by_name(ast_entry, "validate_recursive_argument"),
        validate_noignore_argument = extract_upvalue_by_name(ast_entry, "validate_noignore_argument"),
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
懒加载 LuaFileSystem，若不可用则返回 nil。
Lazily load LuaFileSystem and return nil when it is unavailable.
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
规范化文件路径键，主要用于去重；Windows 下按不区分大小写处理。
Normalize a file-path key for deduplication, handling Windows paths case-insensitively.
]]
local function normalize_file_key(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    if vulcan.os.info().os == "windows" then
        normalized = normalized:lower()
    end
    return normalized
end

--[[
提取路径对应的父目录，并保留目录末尾分隔符，便于在文件菜单中直接作为目录标题显示。
Extract the parent directory of a path while preserving the trailing separator so it can be rendered directly as a directory heading in the file menu.
]]
local function extract_parent_directory(path)
    local normalized = tostring(path or ""):gsub("[\\/]+$", "")
    local parent = normalized:match("^(.*[\\/])[^\\/]+$")
    return parent or ""
end

--[[
提取路径中的文件名部分，供 `FILE MENU` 在目录标题下逐行列出文件名使用。
Extract only the file-name portion of a path so `FILE MENU` can list filenames beneath each directory heading.
]]
local function extract_file_name(path)
    local normalized = tostring(path or ""):gsub("[\\/]+$", "")
    local file_name = normalized:match("([^\\/]+)$")
    return file_name or normalized
end

--[[
判断路径是否为绝对路径，兼容 Windows 盘符、UNC 路径与 Unix 风格绝对路径。
Check whether a path is absolute, supporting Windows drive paths, UNC paths, and Unix-style absolute paths.
]]
local function is_absolute_path(path)
    local normalized = tostring(path or "")
    return normalized:match("^%a:[/\\]") ~= nil
        or starts_with(normalized, "\\\\")
        or starts_with(normalized, "/")
end

--[[
获取当前工作目录，优先使用 LuaFileSystem；缺失时回退到 "."。
Resolve the current working directory, preferring LuaFileSystem and falling back to "." when unavailable.
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
将相对路径解析为当前工作目录下的绝对路径；绝对路径保持原样返回。
Resolve a relative path against the current working directory while leaving absolute paths untouched.
]]
local function resolve_scan_path(path)
    local normalized = tostring(path or "")
    if normalized == "" or is_absolute_path(normalized) then
        return normalized
    end
    return vulcan.path.join(get_current_working_directory(), normalized)
end

--[[
Resolve the ripgrep executable from the host-injected dependency root instead of reconstructing host paths locally.
从宿主注入的依赖根目录解析 ripgrep 可执行文件，而不是在 skill 内部重建宿主路径。
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

--[[
通过 ripgrep 列出目录下的 Markdown 文件，复用其递归与忽略规则能力。
List Markdown files under a directory via ripgrep so recursion and ignore-rule behavior can be reused.
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

    local host_exec = get_host_exec_function()
    if type(host_exec) ~= "function" then
        return nil, {
            error = "rg_exec_failed",
            message = "host process exec is not available",
            dir = directory_path,
        }
    end

    local ok, result = pcall(host_exec, {
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
            local absolute_path = is_absolute_path(normalized) and normalized or vulcan.path.join(directory_path, normalized)
            table.insert(files, {
                path = absolute_path,
                source = directory_path,
            })
        end
    end
    return files, nil
end

--[[
判断文件是否为 Markdown 文件；当前工具仅扫描 `.md` 扩展名。
Check whether a file is Markdown; this tool intentionally scans only `.md` files.
]]
local function is_markdown_file(path)
    local extension = tostring(path or ""):match("%.([^.]+)$")
    return extension ~= nil and extension:lower() == "md"
end

--[[
收集单个目标路径下的 Markdown 文件，支持文件模式与目录模式。
Collect Markdown files under one target path, supporting both file mode and directory mode.
]]
local function collect_markdown_files_for_path(target_path, recursive, ignore_enabled, helpers)
    local collected = {}
    local scan_root = resolve_scan_path(target_path)
    local is_directory = vulcan.fs.is_dir(scan_root)

    if not vulcan.fs.exists(scan_root) then
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
聚合多个输入路径的 Markdown 文件结果，允许目录与文件混用并按绝对路径去重。
Aggregate Markdown files across multiple input paths, allowing mixed file/directory mode and deduplicating by absolute path.
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
规范化标题文本，移除末尾装饰性 `#` 和多余空白，确保目录节点简洁稳定。
Normalize heading text by removing trailing decorative `#` markers and extra whitespace so menu nodes stay clean and stable.
]]
local function normalize_heading_text(heading_text)
    local normalized = trim(heading_text or "")
    normalized = normalized:gsub("%s+#+%s*$", "")
    return trim(normalized)
end

--[[
识别 Markdown 代码围栏行，避免把代码块中的井号误判成文档标题。
Detect Markdown fenced-code lines so hash signs inside code blocks are not mistaken for document headings.
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
从 Markdown 文件中提取 `#`、`##`、`###` 标题及其行号，并返回文件总行数，同时跳过代码围栏区域。
Extract `#`, `##`, and `###` headings with line numbers from a Markdown file, return the total line count, and skip fenced code blocks.
]]
local function extract_markdown_headings(file_path)
    local ok, file_content = pcall(vulcan.fs.read, file_path)
    if not ok then
        return nil, 0, {
            error = "markdown_read_failed",
            message = tostring(file_content),
            path = file_path,
        }
    end

    local headings = {}
    local active_fence = nil
    local file_lines = split_lines(file_content)
    for index, line in ipairs(file_lines) do
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

    return headings, #file_lines, nil
end

--[[
将扫描统计、文件菜单和标题目录详情渲染成单段 Markdown 文本，便于模型直接阅读而无需再解析结构体包装。
Render scan statistics, the file menu, and heading details into a single Markdown text block so models can read it directly without unpacking a wrapper table.
]]
local function build_markdown_menu_content(documents, stats)
    local lines = {
        "# SCAN SUMMARY",
        string.format(
            "- files_scanned: %d | files_with_headings: %d | heading_items: %d | errors: %d",
            tonumber(stats and stats.files_scanned) or 0,
            tonumber(stats and stats.files_with_headings) or 0,
            tonumber(stats and stats.items_found) or 0,
            tonumber(stats and stats.error_count) or 0
        ),
        "",
        "# FILE MENU",
        "If the result is truncated, use the file menu to narrow the path scope and call this tool again with a smaller target set.",
    }

    if #(documents or {}) == 0 then
        table.insert(lines, "(no markdown files found)")
    else
        local grouped_menu_items = {}
        local grouped_menu_order = {}
        for _, document in ipairs(documents) do
            local directory_path = extract_parent_directory(document.path or "")
            if directory_path == "" then
                directory_path = "."
            end
            local directory_key = normalize_file_key(directory_path)
            if not grouped_menu_items[directory_key] then
                grouped_menu_items[directory_key] = {
                    directory = directory_path,
                    files = {},
                }
                table.insert(grouped_menu_order, directory_key)
            end
            table.insert(grouped_menu_items[directory_key].files, extract_file_name(document.path or ""))
        end

        for order_index, directory_key in ipairs(grouped_menu_order) do
            local grouped_item = grouped_menu_items[directory_key]
            table.insert(lines, "> " .. tostring(grouped_item.directory or "."))
            for _, file_name in ipairs(grouped_item.files or {}) do
                table.insert(lines, tostring(file_name or ""))
            end
            if order_index < #grouped_menu_order then
                table.insert(lines, "")
            end
        end
    end
    table.insert(lines, "")
    table.insert(lines, "# Markdown Details")

    if #(documents or {}) == 0 then
        table.insert(lines, "(no markdown headings found)")
        return table.concat(lines, "\n")
    end

    for index, document in ipairs(documents) do
        if index > 1 then
            table.insert(lines, "")
        end
        table.insert(
            lines,
            string.format(
                "[%s Lines:%d]",
                tostring(document.path or ""),
                tonumber(document.line_count) or 0
            )
        )
        if #(document.headings or {}) == 0 then
            table.insert(lines, "(no # / ## / ### headings found)")
        else
            for _, heading in ipairs(document.headings or {}) do
                table.insert(
                    lines,
                    string.format(
                        "L%d: %s %s",
                        tonumber(heading.line) or 0,
                        string.rep("#", tonumber(heading.level) or 1),
                        tostring(heading.text or "")
                    )
                )
            end
        end
    end

    return table.concat(lines, "\n")
end

--[[
完成 markdown-menu 正文输出；是否直接返回原文还是按统一截断策略处理，由宿主统一决定。
Finalize the markdown-menu body; whether it stays inline or is truncated under the unified policy is decided by the host.
]]
local function finalize_markdown_menu_content(markdown_text)
    return tostring(markdown_text or "")
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

-- 技能入口 / Skill entry point invoked by the MCP host runtime.
return function(args)
    local _, budget_error = initialize_markdown_menu_budget()
    if budget_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", budget_error)
    end

    local helpers, helpers_error = load_ast_runtime_helpers()
    if helpers_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", helpers_error)
    end

    local target_paths, path_error = helpers.validate_path_argument(args and args.path)
    if path_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", path_error)
    end

    local recursive, recursive_error = helpers.validate_recursive_argument(args and args.recursive)
    if recursive_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", recursive_error)
    end

    local ignore_enabled, ignore_error = helpers.validate_noignore_argument(args and args.noignore)
    if ignore_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", ignore_error)
    end

    local markdown_files, collection_errors, collection_error = collect_markdown_files(target_paths, recursive, ignore_enabled, helpers)
    if collection_error then
        return render_codekit_error_markdown("CodeKit Markdown Menu Error", collection_error)
    end

    local documents = {}
    local files_with_headings = 0
    local headings_found = 0
    local read_errors = clone_array(collection_errors)

    for _, file_info in ipairs(markdown_files or {}) do
        local headings, line_count, heading_error = extract_markdown_headings(file_info.path)
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
                line_count = line_count or 0,
            })
        end
    end

    return finalize_markdown_menu_content(build_markdown_menu_content(documents, {
        files_scanned = #(markdown_files or {}),
        files_with_headings = files_with_headings,
        items_found = headings_found,
        error_count = #(read_errors or {}),
    }))
end
