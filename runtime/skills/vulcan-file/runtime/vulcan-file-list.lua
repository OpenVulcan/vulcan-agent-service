--[[
vulcan-file-list
Return a compact, directory-grouped file-name map for AI navigation.
返回紧凑的按目录分组文件名地图，供 AI 快速导航。
]]

-- Default maximum number of matched files rendered by one list call.
-- 单次列表调用默认最多渲染的匹配文件数量。
local DEFAULT_LIMIT = 1000

-- Maximum visual width for one compact directory line before wrapping.
-- 单个紧凑目录行换行前的最大可视宽度。
local MAX_LINE_WIDTH = 96

-- Maximum accepted list limit for one tool call.
-- 单次工具调用接受的最大列表上限。
local MAX_LIMIT = 100000

-- Directory names ignored by default to keep the result useful and compact.
-- 默认忽略的目录名，用于保持结果有用且紧凑。
local DEFAULT_IGNORED_DIRS = {
    [".git"] = true,
    [".idea"] = true,
    [".vscode"] = true,
    ["build"] = true,
    ["dist"] = true,
    ["node_modules"] = true,
    ["output"] = true,
    ["target"] = true,
    ["vendor"] = true,
}

-- Error codes that indicate the caller passed invalid tool arguments.
-- 表示调用方传入无效工具参数的错误码集合。
local PARAMETER_ERROR_CODES = {
    invalid_path = true,
    path_not_found = true,
    path_not_directory = true,
    environment_variable_not_found = true,
    invalid_environment_variable_reference = true,
    invalid_pattern_argument = true,
    invalid_recursive_argument = true,
    invalid_noignore_argument = true,
    invalid_limit_argument = true,
}

-- Remove surrounding whitespace from a value converted to text.
-- 将值转换为文本后移除首尾空白。
local function trim(value)
    return (tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

-- Check whether one string starts with a prefix.
-- 检查字符串是否以指定前缀开头。
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

-- Render one stable Markdown error payload for list failures.
-- 为列表失败渲染稳定的 Markdown 错误结果。
local function render_error(error_code, message, details)
    local is_parameter_error = PARAMETER_ERROR_CODES[tostring(error_code or "")] == true
    local lines = {
        "# FILE LIST ERROR",
        "",
        "- error: `" .. tostring(error_code or "unknown_error") .. "`",
        "- type: `" .. (is_parameter_error and "parameter_error" or "runtime_error") .. "`",
        "- message: " .. tostring(message or "unknown file list error"),
    }
    if is_parameter_error then
        table.insert(lines, "- correction: adjust the tool arguments and call again")
    end
    for key, value in pairs(details or {}) do
        table.insert(lines, "- " .. tostring(key) .. ": `" .. tostring(value) .. "`")
    end
    return table.concat(lines, "\n")
end

-- Expand `${env:NAME}` placeholders in a caller-provided path before filesystem access.
-- 在访问文件系统之前展开调用方路径中的 `${env:NAME}` 占位符。
-- Parameters: path is the caller path text and field_name is the argument name rendered in errors; returns expanded path or Markdown error text.
-- 参数：path 为调用方路径文本，field_name 为错误中展示的参数名；返回展开后的路径或 Markdown 错误文本。
local function expand_environment_path(path, field_name)
    local source = tostring(path or "")
    local unresolved_variable = nil
    local expanded = source:gsub("%${env:([^}]+)}", function(variable_name)
        local normalized_name = trim(variable_name)
        if normalized_name == "" then
            unresolved_variable = variable_name
            return ""
        end

        local environment_value = os.getenv(normalized_name)
        if environment_value == nil then
            unresolved_variable = normalized_name
            return ""
        end
        return environment_value
    end)

    if unresolved_variable ~= nil then
        return nil, render_error("environment_variable_not_found", "environment variable referenced in path is not defined", {
            field = tostring(field_name or "path"),
            variable = tostring(unresolved_variable),
            path = source,
        })
    end
    if expanded:find("${env:", 1, true) ~= nil then
        return nil, render_error("invalid_environment_variable_reference", "environment variable path placeholder must use ${env:NAME} syntax", {
            field = tostring(field_name or "path"),
            path = source,
        })
    end
    return expanded, nil
end

-- Normalize path separators to forward slashes for stable grouping.
-- 将路径分隔符规范化为正斜杠，保证分组稳定。
local function normalize_slashes(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    normalized = normalized:gsub("/+", "/")
    normalized = normalized:gsub("/$", "")
    return normalized
end

-- Normalize a path into the forward-slash form used by ignore matching.
-- 将路径规范化为 ignore 匹配使用的正斜杠形式。
local function normalize_ignore_path(path)
    local normalized = normalize_slashes(path)
    normalized = normalized:gsub("^%./", "")
    return normalized
end

-- Join two path fragments using the host path helper when available.
-- 优先使用宿主路径辅助函数拼接两个路径片段。
local function join_path(left, right)
    if vulcan and vulcan.path and type(vulcan.path.join) == "function" then
        return vulcan.path.join(left, right)
    end
    local separator = package.config:sub(1, 1)
    return tostring(left or "") .. separator .. tostring(right or "")
end

-- Return the last path segment used for ignore checks.
-- 返回用于忽略规则判断的最后一个路径片段。
local function basename(path)
    local normalized = normalize_slashes(path)
    return normalized:match("([^/]+)$") or normalized
end

-- Validate that the requested root is an existing directory.
-- 校验请求根路径是一个已存在目录。
local function validate_root_path(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, render_error("invalid_path", "path must be a non-empty directory path")
    end

    local root_path, environment_error = expand_environment_path(trim(value), "path")
    if environment_error then
        return nil, environment_error
    end
    root_path = trim(root_path)
    if root_path == "" then
        return nil, render_error("invalid_path", "path must resolve to a non-empty directory path")
    end
    if not vulcan.fs.exists(root_path) then
        return nil, render_error("path_not_found", "path does not exist", { path = root_path })
    end
    if not vulcan.fs.is_dir(root_path) then
        return nil, render_error("path_not_directory", "path must point to a directory", { path = root_path })
    end
    return root_path, nil
end

-- Convert one shell-style glob into a Lua pattern.
-- 将 shell 风格 glob 转换为 Lua pattern。
local function glob_to_pattern(glob)
    local source = trim(glob)
    if source == "" then
        source = "*"
    end

    local output = { "^" }
    for index = 1, #source do
        local char = source:sub(index, index)
        if char == "*" then
            table.insert(output, ".*")
        elseif char == "?" then
            table.insert(output, ".")
        elseif char:match("[%w_]") then
            table.insert(output, char)
        else
            table.insert(output, "%" .. char)
        end
    end
    table.insert(output, "$")
    return table.concat(output)
end

-- Decide whether a file name matches the requested glob pattern.
-- 判断文件名是否匹配请求的 glob 模式。
local function matches_pattern(file_name, compiled_pattern)
    return tostring(file_name or ""):match(compiled_pattern) ~= nil
end

-- Compute a stable relative path from the scan root.
-- 基于扫描根目录计算稳定的相对路径。
local function relative_path(root_path, full_path)
    local root = normalize_slashes(root_path)
    local full = normalize_slashes(full_path)
    if full == root then
        return "."
    end
    if starts_with(full, root .. "/") then
        return full:sub(#root + 2)
    end
    return full
end

-- Return the path of a target relative to one ignore-rule base directory.
-- 返回目标路径相对于某个 ignore 规则基目录的路径。
local function relative_ignore_path(base_directory, full_path)
    local normalized_base = normalize_ignore_path(base_directory)
    local normalized_full = normalize_ignore_path(full_path)
    if normalized_full == normalized_base then
        return ""
    end
    if starts_with(normalized_full, normalized_base .. "/") then
        return normalized_full:sub(#normalized_base + 2)
    end
    return normalized_full
end

-- Split text into lines after normalizing CRLF to LF.
-- 统一 CRLF 为 LF 后按行拆分文本。
local function split_lines(content)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    if normalized == "" then
        return {}
    end
    if normalized:sub(-1) == "\n" then
        normalized = normalized:sub(1, -2)
    end
    local lines = {}
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end
    return lines
end

-- Match a simplified gitignore-style glob against a normalized path.
-- 用简化版 gitignore glob 匹配一个规范化路径。
local function match_ignore_glob(text, pattern)
    local source = tostring(text or "")
    local glob = tostring(pattern or "")
    local source_length = #source
    local pattern_length = #glob
    local memo = {}

    local function visit(source_index, pattern_index)
        local cache_key = tostring(source_index) .. ":" .. tostring(pattern_index)
        if memo[cache_key] ~= nil then
            return memo[cache_key]
        end

        local matched = false
        if pattern_index > pattern_length then
            matched = source_index > source_length
        else
            local current = glob:sub(pattern_index, pattern_index)
            local next_char = glob:sub(pattern_index + 1, pattern_index + 1)
            if current == "*" and next_char == "*" then
                local next_index = pattern_index + 2
                while glob:sub(next_index, next_index) == "*" do
                    next_index = next_index + 1
                end
                matched = visit(source_index, next_index)
                if not matched then
                    for offset = source_index, source_length do
                        if visit(offset + 1, pattern_index) then
                            matched = true
                            break
                        end
                    end
                end
            elseif current == "*" then
                matched = visit(source_index, pattern_index + 1)
                local offset = source_index
                while not matched and offset <= source_length and source:sub(offset, offset) ~= "/" do
                    matched = visit(offset + 1, pattern_index + 1)
                    offset = offset + 1
                end
            elseif current == "?" then
                matched = source_index <= source_length
                    and source:sub(source_index, source_index) ~= "/"
                    and visit(source_index + 1, pattern_index + 1)
                    or false
            else
                matched = source_index <= source_length
                    and source:sub(source_index, source_index) == current
                    and visit(source_index + 1, pattern_index + 1)
                    or false
            end
        end

        memo[cache_key] = matched
        return matched
    end

    return visit(1, 1)
end

-- Parse one `.gitignore` or `.ignore` rule line.
-- 解析 `.gitignore` 或 `.ignore` 中的一行规则。
local function parse_ignore_rule(line, base_directory)
    local normalized_line = trim(line)
    if normalized_line == "" or starts_with(normalized_line, "#") then
        return nil
    end

    local negative = false
    if starts_with(normalized_line, "!") then
        negative = true
        normalized_line = trim(normalized_line:sub(2))
    end
    if normalized_line == "" then
        return nil
    end

    local directory_only = normalized_line:sub(-1) == "/"
    if directory_only then
        normalized_line = normalized_line:sub(1, -2)
    end

    local anchored = starts_with(normalized_line, "/")
    if anchored then
        normalized_line = normalized_line:sub(2)
    end

    normalized_line = normalize_ignore_path(normalized_line)
    if normalized_line == "" then
        return nil
    end

    return {
        base_directory = normalize_ignore_path(base_directory),
        pattern = normalized_line,
        negative = negative,
        directory_only = directory_only,
        anchored = anchored,
        has_slash = normalized_line:find("/", 1, true) ~= nil,
    }
end

-- Load `.gitignore` and `.ignore` rules from one directory using a request-local cache.
-- 使用单次请求内缓存从目录加载 `.gitignore` 与 `.ignore` 规则。
local function load_directory_ignore_rules(directory_path, ignore_rule_cache)
    local request_cache = ignore_rule_cache or {}
    local cache_key = normalize_ignore_path(directory_path)
    if request_cache[cache_key] then
        return request_cache[cache_key]
    end

    local collected = {}
    for _, ignore_file_name in ipairs({ ".gitignore", ".ignore" }) do
        local ignore_file_path = join_path(directory_path, ignore_file_name)
        if vulcan.fs.exists(ignore_file_path) and not vulcan.fs.is_dir(ignore_file_path) then
            local ok, content = pcall(vulcan.fs.read, ignore_file_path)
            if ok then
                for _, line in ipairs(split_lines(content or "")) do
                    local parsed_rule = parse_ignore_rule(line, directory_path)
                    if parsed_rule then
                        table.insert(collected, parsed_rule)
                    end
                end
            end
        end
    end

    request_cache[cache_key] = collected
    return collected
end

-- Decide whether one entry should be skipped by built-in or file-based ignore rules.
-- 判断一个目录项是否应被内建规则或 ignore 文件规则跳过。
local function should_ignore_entry(full_path, entry_name, is_directory, active_ignore_rules, ignore_enabled)
    if not ignore_enabled then
        return false
    end

    if is_directory and DEFAULT_IGNORED_DIRS[tostring(entry_name or ""):lower()] then
        return true
    end

    local ignored = false
    for _, rule in ipairs(active_ignore_rules or {}) do
        if not (rule.directory_only and not is_directory) then
            local candidate = (rule.anchored or rule.has_slash)
                and relative_ignore_path(rule.base_directory, full_path)
                or normalize_ignore_path(entry_name)
            if candidate ~= "" and match_ignore_glob(candidate, rule.pattern) then
                ignored = not rule.negative
            end
        end
    end
    return ignored
end

-- Record one file into a directory-grouped result table.
-- 将一个文件记录到按目录分组的结果表。
local function add_grouped_file(groups, root_path, full_path)
    local rel = relative_path(root_path, full_path)
    local dir, file_name = rel:match("^(.*)/([^/]+)$")
    dir = dir or "."
    file_name = file_name or rel
    if groups[dir] == nil then
        groups[dir] = {}
    end
    table.insert(groups[dir], file_name)
end

-- Read one directory entry list with stable sorting.
-- 读取一个目录的条目列表并稳定排序。
local function list_directory(directory_path)
    local ok, entries = pcall(vulcan.fs.list, directory_path)
    if not ok then
        return nil, render_error("directory_read_failed", tostring(entries), { path = directory_path })
    end
    if type(entries) ~= "table" then
        return nil, render_error("directory_read_invalid", "vulcan.fs.list did not return a table", { path = directory_path })
    end

    table.sort(entries, function(left, right)
        return tostring(left) < tostring(right)
    end)
    return entries, nil
end

-- Recursively collect matching files while respecting ignore rules and limits.
-- 递归收集匹配文件，并遵守忽略规则与数量上限。
local function collect_files(root_path, directory_path, request, state, active_ignore_rules)
    if state.truncated then
        return nil
    end

    local current_ignore_rules = active_ignore_rules or {}
    if not request.noignore then
        local directory_rules = load_directory_ignore_rules(directory_path, state.ignore_rule_cache)
        if #directory_rules > 0 then
            current_ignore_rules = { table.unpack(current_ignore_rules) }
            for _, rule in ipairs(directory_rules) do
                table.insert(current_ignore_rules, rule)
            end
        end
    end

    local entries, list_error = list_directory(directory_path)
    if list_error then
        return list_error
    end

    for _, entry in ipairs(entries) do
        if state.truncated then
            return nil
        end
        local entry_name = tostring(entry)
        local full_path = join_path(directory_path, entry_name)
        local is_directory = vulcan.fs.is_dir(full_path)
        local ignored = should_ignore_entry(
            full_path,
            entry_name,
            is_directory,
            current_ignore_rules,
            not request.noignore
        )

        if ignored then
            state.ignored_entries = state.ignored_entries + 1
        elseif is_directory then
            if request.recursive then
                local child_error = collect_files(root_path, full_path, request, state, current_ignore_rules)
                if child_error then
                    return child_error
                end
                if state.truncated then
                    return nil
                end
            end
        elseif matches_pattern(entry_name, request.compiled_pattern) then
            add_grouped_file(state.groups, root_path, full_path)
            state.matched_files = state.matched_files + 1
            if state.matched_files >= request.limit then
                state.truncated = true
                return nil
            end
        end
    end

    return nil
end

-- Render one compact directory group with wrapped file names.
-- 渲染一个带自动换行的紧凑目录分组。
local function render_group(dir, files)
    table.sort(files)
    local output = {}
    local prefix = dir == "." and "./" or (dir .. "/")
    local current = prefix

    for _, file_name in ipairs(files) do
        local token = tostring(file_name)
        if #current + #token + 1 > MAX_LINE_WIDTH and current ~= prefix then
            table.insert(output, current)
            current = "  " .. token
        else
            if current == prefix then
                current = current .. " " .. token
            else
                current = current .. " " .. token
            end
        end
    end

    if current ~= "" then
        table.insert(output, current)
    end
    return output
end

-- Render the final compact file map.
-- 渲染最终紧凑文件地图。
local function render_file_map(root_path, request, state)
    local dirs = {}
    for dir, _ in pairs(state.groups) do
        table.insert(dirs, dir)
    end
    table.sort(dirs)

    local lines = {
        string.format(
            "[FileList:%s Pattern:%s Recursive:%s Ignore:%s Files:%d Truncated:%s Ignored:%d]",
            root_path,
            request.pattern,
            tostring(request.recursive),
            tostring(not request.noignore),
            state.matched_files,
            tostring(state.truncated),
            state.ignored_entries
        ),
    }

    if #dirs == 0 then
        table.insert(lines, "(no matching files)")
        return table.concat(lines, "\n")
    end

    for _, dir in ipairs(dirs) do
        for _, rendered_line in ipairs(render_group(dir, state.groups[dir])) do
            table.insert(lines, rendered_line)
        end
    end

    return table.concat(lines, "\n")
end

-- Validate an optional boolean argument and report type mistakes as parameter errors.
-- 校验可选布尔参数，并将类型错误报告为参数错误。
local function validate_optional_boolean(value, argument_name)
    if value == nil or type(value) == "boolean" then
        return nil
    end
    return render_error("invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be a boolean when provided", {
        argument = argument_name,
        value = tostring(value),
        actual_type = type(value),
    })
end

-- Validate an optional string argument and report type mistakes as parameter errors.
-- 校验可选字符串参数，并将类型错误报告为参数错误。
local function validate_optional_string(value, argument_name)
    if value == nil or type(value) == "string" then
        return nil
    end
    return render_error("invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be a string when provided", {
        argument = argument_name,
        value = tostring(value),
        actual_type = type(value),
    })
end

-- Validate and normalize the optional list limit.
-- 校验并规范化可选的列表数量上限。
local function validate_limit_argument(value)
    if value == nil then
        return DEFAULT_LIMIT, nil
    end
    if type(value) ~= "number" or value ~= math.floor(value) or value < 1 or value > MAX_LIMIT then
        return nil, render_error("invalid_limit_argument", "limit must be a positive integer within the allowed maximum", {
            argument = "limit",
            value = tostring(value),
            actual_type = type(value),
            max = tostring(MAX_LIMIT),
        })
    end
    return value, nil
end

-- Parse and validate one list request.
-- 解析并校验一次列表请求。
local function parse_request(args)
    local input = type(args) == "table" and args or {}
    local pattern_error = validate_optional_string(input.pattern, "pattern")
    if pattern_error then
        return nil, pattern_error
    end
    local recursive_error = validate_optional_boolean(input.recursive, "recursive")
    if recursive_error then
        return nil, recursive_error
    end
    local noignore_error = validate_optional_boolean(input.noignore, "noignore")
    if noignore_error then
        return nil, noignore_error
    end
    local limit, limit_error = validate_limit_argument(input.limit)
    if limit_error then
        return nil, limit_error
    end

    local root_path, path_error = validate_root_path(input.path or input.file or ".")
    if path_error then
        return nil, path_error
    end

    local pattern = type(input.pattern) == "string" and trim(input.pattern) or "*"
    if pattern == "" then
        pattern = "*"
    end

    return {
        root_path = root_path,
        pattern = pattern,
        compiled_pattern = glob_to_pattern(pattern),
        recursive = input.recursive ~= false,
        noignore = input.noignore == true,
        limit = limit,
    }, nil
end

-- Return successful list content with a host-managed truncate overflow hint.
-- 返回列表成功内容，并显式声明由宿主管理的 truncate 超限策略。
local function return_list_success(content)
    return content, vulcan.runtime.overflow_type.truncate
end

-- Tool entry point invoked by the LuaSkills runtime.
-- LuaSkills 运行时调用的工具入口。
return function(args)
    local request, request_error = parse_request(args)
    if request_error then
        return request_error
    end

    local state = {
        groups = {},
        ignored_entries = 0,
        ignore_rule_cache = {},
        matched_files = 0,
        truncated = false,
    }

    local collect_error = collect_files(request.root_path, request.root_path, request, state, {})
    if collect_error then
        return collect_error
    end

    return return_list_success(render_file_map(request.root_path, request, state))
end
