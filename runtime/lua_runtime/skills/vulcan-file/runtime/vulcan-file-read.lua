--[[
vulcan-file-read
Read one text file or directory with AI-friendly line numbers and compact start,count line rules.
读取一个文本文件或目录，并提供适合 AI 使用的行号与紧凑的 start,count 行规则。
]]

-- Default maximum lines returned when the host does not provide a file-read budget.
-- 当宿主未提供文件读取预算时使用的默认最大返回行数。
local DEFAULT_LIMIT_LINES = 200

-- Maximum number of files accepted by one batch read call.
-- 单次批量读取调用允许处理的最大文件数量。
local MAX_BATCH_FILES = 10

-- Maximum safe positive integer accepted by lines_rule parsing.
-- lines_rule 解析接受的最大安全正整数。
local MAX_SAFE_RULE_INTEGER = 9007199254740991

-- Text form of MAX_SAFE_RULE_INTEGER for stable error rendering.
-- MAX_SAFE_RULE_INTEGER 的文本形式，用于稳定渲染错误信息。
local MAX_SAFE_RULE_INTEGER_TEXT = "9007199254740991"

-- Visible Markdown title used for read error payloads.
-- 读取错误结果使用的可见 Markdown 标题。
local ERROR_TITLE = "FILE READ ERROR"

-- Error codes that indicate the caller passed invalid tool arguments.
-- 表示调用方传入无效工具参数的错误码集合。
local PARAMETER_ERROR_CODES = {
    invalid_path = true,
    invalid_files_argument = true,
    too_many_files = true,
    conflicting_batch_arguments = true,
    invalid_pwd_argument = true,
    relative_path_requires_pwd = true,
    path_not_found = true,
    environment_variable_not_found = true,
    invalid_environment_variable_reference = true,
    invalid_lines_rule = true,
    invalid_segments_argument = true,
    range_out_of_bounds = true,
    conflicting_range_arguments = true,
    invalid_numbered_argument = true,
}

-- Remove surrounding whitespace from a value converted to text.
-- 将值转换为文本后移除首尾空白。
local function trim(value)
    return (tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

-- Check whether one string ends with the given suffix.
-- 检查一个字符串是否以指定后缀结尾。
local function ends_with(text, suffix)
    return tostring(text or ""):sub(-#suffix) == suffix
end

-- Read one nested table value without throwing when intermediate nodes are missing.
-- 在中间节点缺失时安全读取嵌套表值。
local function get_nested_value(root, ...)
    local current = root
    local keys = { ... }
    for _, key in ipairs(keys) do
        if type(current) ~= "table" then
            return nil
        end
        current = current[key]
    end
    return current
end

-- Resolve the current file-read line budget from the host context.
-- 从宿主上下文解析当前文件读取行数预算。
local function resolve_line_budget()
    local budget_lines = tonumber(get_nested_value(vulcan, "context", "client_budget", "file_read", "lines"))
    if budget_lines and budget_lines > 0 then
        return math.floor(budget_lines)
    end
    return DEFAULT_LIMIT_LINES
end

-- Load the shared file helper module from the current entry directory.
-- 从当前入口目录加载共享文件辅助模块。
--
-- Parameters:
--     None.
-- 参数：
--     无。
--
-- Returns:
--     table: Shared helper table used for path and validation helpers.
-- 返回值：
--     table：用于路径与校验辅助的共享 helper 表。
local function load_shared_file_helpers()
    local entry_dir = tostring(vulcan.context.entry_dir or ".")
    local helper_path = vulcan.path.join(entry_dir, "shared_file.lua")
    local chunk, load_error = loadfile(helper_path)
    if not chunk then
        error("Failed to load shared_file.lua: " .. tostring(load_error))
    end

    local ok, helpers = pcall(chunk)
    if not ok or type(helpers) ~= "table" then
        error("shared_file.lua did not return a helper table: " .. tostring(helpers))
    end
    return helpers
end

-- Render one stable Markdown error payload for invalid input or file failures.
-- 为无效输入或文件失败渲染稳定的 Markdown 错误结果。
local function render_error(error_code, message, details)
    local is_parameter_error = PARAMETER_ERROR_CODES[tostring(error_code or "")] == true
    local lines = {
        "# " .. ERROR_TITLE,
        "",
        "- error: `" .. tostring(error_code or "unknown_error") .. "`",
        "- type: `" .. (is_parameter_error and "parameter_error" or "runtime_error") .. "`",
        "- message: " .. tostring(message or "unknown file read error"),
    }
    if is_parameter_error then
        table.insert(lines, "- correction: adjust the tool arguments and call again")
    end
    for key, value in pairs(details or {}) do
        table.insert(lines, "- " .. tostring(key) .. ": `" .. tostring(value) .. "`")
    end
    return table.concat(lines, "\n")
end

-- Resolve one caller path with `${env:NAME}` expansion plus the optional `PWD` convention root.
-- 使用 `${env:NAME}` 展开与可选 `PWD` 公约根目录解析一个调用方路径。
--
-- Parameters:
--     helpers: Shared helper table.
--     path: Caller path text.
--     field_name: Argument name rendered in errors.
--     pwd_root: Valid absolute `PWD` directory root, or nil.
-- 参数：
--     helpers：共享辅助表。
--     path：调用方路径文本。
--     field_name：错误中展示的参数名。
--     pwd_root：有效绝对 `PWD` 目录根路径，或 nil。
--
-- Returns:
--     string|nil: Absolute resolved path on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：成功时返回绝对解析路径。
--     string|nil：失败时返回 Markdown 错误文本。
local function expand_environment_path(helpers, path, field_name, pwd_root)
    return helpers.resolve_input_path(
        ERROR_TITLE,
        PARAMETER_ERROR_CODES,
        path,
        field_name,
        pwd_root,
        "invalid_path",
        "file must resolve to a non-empty string"
    )
end

-- Parse a positive integer text value without accepting overflowing Lua numbers.
-- 解析正整数文本，并拒绝会溢出 Lua 安全数字范围的值。
local function parse_positive_integer_text(value, field_name, segment, segment_index)
    local text = tostring(value or "")
    if not text:match("^%d+$") then
        return nil, render_error("invalid_lines_rule", "lines_rule start and count must be positive integers", {
            field = tostring(field_name),
            segment = tostring(segment),
            segment_index = tostring(segment_index),
            value = text,
        })
    end

    local normalized_text = text:gsub("^0+", "")
    if normalized_text == "" then
        normalized_text = "0"
    end
    if #normalized_text > 16 then
        return nil, render_error("invalid_lines_rule", "lines_rule integer is too large to be represented safely", {
            field = tostring(field_name),
            segment = tostring(segment),
            segment_index = tostring(segment_index),
            value = text,
            max = MAX_SAFE_RULE_INTEGER_TEXT,
        })
    end

    local number_value = tonumber(normalized_text)
    if not number_value or number_value < 1 or number_value > MAX_SAFE_RULE_INTEGER then
        return nil, render_error("invalid_lines_rule", "lines_rule start and count must be positive safe integers", {
            field = tostring(field_name),
            segment = tostring(segment),
            segment_index = tostring(segment_index),
            value = text,
            max = MAX_SAFE_RULE_INTEGER_TEXT,
        })
    end

    return math.floor(number_value), nil
end

-- Parse one positive integer value from a structured segments item.
-- 从结构化 segments 项中解析一个正整数值。
local function parse_segment_integer(value, field_name, segment_index)
    local value_type = type(value)
    local value_text = trim(value)
    if value_type ~= "number" and value_type ~= "string" then
        return nil, render_error("invalid_segments_argument", "segments start and count must be positive integers", {
            argument = tostring(field_name),
            segment_index = tostring(segment_index),
            value = tostring(value),
            actual_type = value_type,
        })
    end

    if value_type == "number" then
        if value ~= math.floor(value) or value < 1 or value > MAX_SAFE_RULE_INTEGER then
            return nil, render_error("invalid_segments_argument", "segments start and count must be positive safe integers", {
                argument = tostring(field_name),
                segment_index = tostring(segment_index),
                value = tostring(value),
                max = MAX_SAFE_RULE_INTEGER_TEXT,
            })
        end
        return math.floor(value), nil
    end

    if not value_text:match("^%d+$") then
        return nil, render_error("invalid_segments_argument", "segments start and count must be positive integers", {
            argument = tostring(field_name),
            segment_index = tostring(segment_index),
            value = value_text,
            actual_type = value_type,
        })
    end

    local normalized_text = value_text:gsub("^0+", "")
    if normalized_text == "" then
        normalized_text = "0"
    end
    if #normalized_text > 16 then
        return nil, render_error("invalid_segments_argument", "segments integer is too large to be represented safely", {
            argument = tostring(field_name),
            segment_index = tostring(segment_index),
            value = value_text,
            max = MAX_SAFE_RULE_INTEGER_TEXT,
        })
    end

    local number_value = tonumber(normalized_text)
    if not number_value or number_value < 1 or number_value > MAX_SAFE_RULE_INTEGER then
        return nil, render_error("invalid_segments_argument", "segments start and count must be positive safe integers", {
            argument = tostring(field_name),
            segment_index = tostring(segment_index),
            value = value_text,
            max = MAX_SAFE_RULE_INTEGER_TEXT,
        })
    end
    return math.floor(number_value), nil
end

-- Validate that a path points to one existing file or directory.
-- 校验路径指向一个已存在的文件或目录。
--
-- Parameters:
--     helpers: Shared helper table.
--     value: Caller-provided file or directory path.
--     pwd_root: Valid absolute `PWD` directory root, or nil.
-- 参数：
--     helpers：共享辅助表。
--     value：调用方传入的文件或目录路径。
--     pwd_root：有效绝对 `PWD` 目录根路径，或 nil。
--
-- Returns:
--     string|nil: Existing resolved file or directory path on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：成功时返回已存在的解析后文件或目录路径。
--     string|nil：失败时返回 Markdown 错误文本。
local function validate_target_path(helpers, value, pwd_root)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, render_error("invalid_path", "file must be a non-empty string")
    end

    local target_path, environment_error = expand_environment_path(helpers, trim(value), "file", pwd_root)
    if environment_error then
        return nil, environment_error
    end
    target_path = trim(target_path)
    if target_path == "" then
        return nil, render_error("invalid_path", "file must resolve to a non-empty string")
    end
    if not vulcan.fs.exists(target_path) then
        return nil, render_error("path_not_found", "path does not exist", { file = target_path })
    end
    return target_path, nil
end

-- Read direct directory entries through the host filesystem bridge.
-- 通过宿主文件系统桥接读取目录直接子项。
local function read_directory_entries(directory_path)
    local ok, entries = pcall(vulcan.fs.list, directory_path)
    if not ok then
        return nil, render_error("directory_read_failed", tostring(entries), { file = directory_path })
    end
    if type(entries) ~= "table" then
        return nil, render_error("directory_read_invalid", "vulcan.fs.list did not return a table", { file = directory_path })
    end

    local names = {}
    for _, entry in ipairs(entries) do
        table.insert(names, tostring(entry))
    end
    table.sort(names)
    return names, nil
end

-- Render one sorted directory listing using a compact Name-only format.
-- 使用紧凑的单列 Name 格式渲染排序后的目录列表。
local function render_directory_listing(directory_path, names)
    local lines = {
        string.format("[Directory:%s Entries:%d Sort:Name]", directory_path, #names),
        "Name",
        "----",
    }
    for _, name in ipairs(names) do
        table.insert(lines, name)
    end
    return table.concat(lines, "\n")
end

-- Read the complete file content through the host filesystem bridge.
-- 通过宿主文件系统桥接读取完整文件内容。
local function read_file(file_path)
    local ok, content = pcall(vulcan.fs.read, file_path)
    if not ok then
        return nil, render_error("file_read_failed", tostring(content), { file = file_path })
    end
    if type(content) ~= "string" then
        return nil, render_error("file_read_invalid", "vulcan.fs.read did not return a string", { file = file_path })
    end
    return content, nil
end

-- Detect the dominant newline style of a text payload.
-- 检测文本内容的主要换行风格。
local function detect_newline_style(content)
    local crlf_count = 0
    for _ in tostring(content or ""):gmatch("\r\n") do
        crlf_count = crlf_count + 1
    end

    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    local lf_count = 0
    for _ in normalized:gmatch("\n") do
        lf_count = lf_count + 1
    end

    local bare_lf_count = lf_count - crlf_count
    if crlf_count > 0 and bare_lf_count > 0 then
        return "mixed"
    end
    if crlf_count > 0 then
        return "CRLF"
    end
    return "LF"
end

-- Split text into logical lines without creating an extra line for a trailing newline.
-- 将文本拆分为逻辑行，并避免因为末尾换行额外生成一行。
local function split_lines(content)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    if normalized == "" then
        return {}
    end
    if ends_with(normalized, "\n") then
        normalized = normalized:sub(1, -2)
    end
    local lines = {}
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end
    return lines
end

-- Resolve the caller-provided line rule or build the default budget-backed rule.
-- 解析调用方传入的行规则，或构造由预算驱动的默认规则。
local function resolve_lines_rule_input(args)
    local raw_rule = args.lines_rule
    if raw_rule == nil or trim(raw_rule) == "" then
        return "1," .. tostring(resolve_line_budget()), false, nil
    end
    if type(raw_rule) ~= "string" then
        return nil, false, render_error("invalid_lines_rule", "lines_rule must be a string in start,count format", {
            lines_rule = tostring(raw_rule),
        })
    end
    return raw_rule, true, nil
end

-- Parse one start,count segment from a lines_rule string.
-- 从 lines_rule 字符串中解析一个 start,count 片段。
local function parse_rule_segment(segment, segment_index)
    local start_text, count_text = trim(segment):match("^(%d+)%s*,%s*(%d+)$")
    if not start_text or not count_text then
        return nil, render_error("invalid_lines_rule", "each lines_rule segment must use start,count format", {
            segment = tostring(segment),
            segment_index = tostring(segment_index),
        })
    end

    local start_line, start_error = parse_positive_integer_text(start_text, "start", segment, segment_index)
    if start_error then
        return nil, start_error
    end
    local line_count, count_error = parse_positive_integer_text(count_text, "count", segment, segment_index)
    if count_error then
        return nil, count_error
    end

    return {
        start_line = start_line,
        requested_count = line_count,
        segment = trim(segment),
        segment_index = segment_index,
    }, nil
end

-- Parse one structured `{ start, count }` segment object into one read request.
-- 将一个结构化 `{ start, count }` 片段对象解析为单个读取请求。
local function parse_structured_segment(segment, segment_index)
    if type(segment) ~= "table" then
        return nil, render_error("invalid_segments_argument", "each segments item must be an object with start and count", {
            segment_index = tostring(segment_index),
            actual_type = type(segment),
        })
    end

    local start_line, start_error = parse_segment_integer(segment.start, "start", segment_index)
    if start_error then
        return nil, start_error
    end
    local line_count, count_error = parse_segment_integer(segment.count, "count", segment_index)
    if count_error then
        return nil, count_error
    end

    return {
        start_line = start_line,
        requested_count = line_count,
        segment = string.format("%d,%d", start_line, line_count),
        segment_index = segment_index,
    }, nil
end

-- Parse a full lines_rule value into ordered read requests.
-- 将完整 lines_rule 值解析为按顺序执行的读取请求。
local function parse_lines_rule(lines_rule)
    -- Reject the old ambiguous prose token before normal newline normalization hides the caller intent.
    -- 在换行归一化前拒绝旧的歧义文字标记，避免调用意图被隐藏。
    if tostring(lines_rule or ""):lower():find("newline", 1, true) ~= nil then
        return nil, render_error("invalid_lines_rule", "lines_rule segments must be separated with LF newline (\\n) inside the JSON string, not the literal word newline", {
            example = "5,10\\n25,30",
            lines_rule = tostring(lines_rule or ""),
        })
    end

    local normalized_rule = tostring(lines_rule or ""):gsub("\r\n", "\n"):gsub("\r", "\n")
    local requests = {}
    local segment_index = 0

    for segment in (normalized_rule .. "\n"):gmatch("(.-)\n") do
        local cleaned_segment = trim(segment)
        if cleaned_segment ~= "" then
            segment_index = segment_index + 1
            local request, segment_error = parse_rule_segment(cleaned_segment, segment_index)
            if segment_error then
                return nil, segment_error
            end
            table.insert(requests, request)
        end
    end

    if #requests == 0 then
        return nil, render_error("invalid_lines_rule", "lines_rule must contain at least one start,count segment")
    end
    return requests, nil
end

-- Parse the preferred structured `segments` array into ordered read requests.
-- 将首选的结构化 `segments` 数组解析为有序读取请求。
local function parse_segments(segments)
    if type(segments) ~= "table" then
        return nil, render_error("invalid_segments_argument", "segments must be an array of {start, count} objects", {
            actual_type = type(segments),
        })
    end

    local requests = {}
    local segment_count = 0
    for segment_index, segment in ipairs(segments) do
        segment_count = segment_count + 1
        local request, segment_error = parse_structured_segment(segment, segment_index)
        if segment_error then
            return nil, segment_error
        end
        table.insert(requests, request)
    end

    if segment_count == 0 then
        if next(segments) ~= nil then
            return nil, render_error("invalid_segments_argument", "segments must be an array of {start, count} objects", {
                actual_type = "table",
            })
        end
        return nil, render_error("invalid_segments_argument", "segments must contain at least one range object")
    end
    return requests, nil
end

-- Resolve range selection from structured segments, legacy lines_rule, or the default budget rule.
-- 从结构化 segments、旧版 lines_rule 或默认预算规则中解析展示范围选择。
local function resolve_range_requests(args)
    local has_segments = args.segments ~= nil
    local has_lines_rule = args.lines_rule ~= nil and trim(args.lines_rule) ~= ""
    if has_segments and has_lines_rule then
        return nil, true, render_error("conflicting_range_arguments", "segments and lines_rule cannot be provided together", {
            preferred = "segments",
        })
    end

    if has_segments then
        local requests, segments_error = parse_segments(args.segments)
        if segments_error then
            return nil, true, segments_error
        end
        return requests, true, nil
    end

    local lines_rule, explicit_rule, input_error = resolve_lines_rule_input(args)
    if input_error then
        return nil, explicit_rule, input_error
    end

    local requests, parse_error = parse_lines_rule(lines_rule)
    if parse_error then
        return nil, explicit_rule, parse_error
    end
    return requests, explicit_rule, nil
end

-- Build display ranges from one or more start,count rule segments.
-- 根据一个或多个 start,count 规则片段构造展示行范围。
local function resolve_display_ranges(args, lines)
    local line_count = #lines
    local requests, explicit_rule, request_error = resolve_range_requests(args)
    if request_error then
        return nil, request_error
    end

    if line_count == 0 then
        if explicit_rule then
            local first_request = requests[1]
            return nil, render_error("range_out_of_bounds", "file is empty; lines_rule cannot be resolved", {
                segment = first_request.segment,
                start_line = tostring(first_request.start_line),
                count = tostring(first_request.requested_count),
                total_lines = tostring(line_count),
            })
        end
        return {}, { segment_count = 0, clipped = false, explicit_rule = false }
    end

    local ranges = {}
    local clipped = false
    for _, request in ipairs(requests) do
        if request.start_line > line_count then
            return nil, render_error("range_out_of_bounds", "lines_rule start is beyond the end of the file", {
                segment = request.segment,
                segment_index = tostring(request.segment_index),
                start_line = tostring(request.start_line),
                count = tostring(request.requested_count),
                total_lines = tostring(line_count),
            })
        end

        local requested_end = request.start_line + request.requested_count - 1
        local end_line = math.min(requested_end, line_count)
        local segment_clipped = explicit_rule and requested_end > line_count
        clipped = clipped or segment_clipped
        table.insert(ranges, {
            start_line = request.start_line,
            end_line = end_line,
            requested_count = request.requested_count,
            clipped = segment_clipped,
            segment = request.segment,
            segment_index = request.segment_index,
        })
    end

    return ranges, { segment_count = #ranges, clipped = clipped, explicit_rule = explicit_rule }
end

-- Render one file header with useful metadata for AI navigation.
-- 渲染包含 AI 导航所需元信息的文件头。
local function render_header(file_path, content, lines, ranges, metadata)
    local visible_ranges = {}
    for _, range in ipairs(ranges or {}) do
        table.insert(visible_ranges, "L" .. tostring(range.start_line) .. "-L" .. tostring(range.end_line))
    end
    if #visible_ranges == 0 then
        table.insert(visible_ranges, "none")
    end

    return string.format(
        "[File:%s Lines:%d Bytes:%d Newline:%s Showing:%s Segments:%d Clipped:%s]",
        file_path,
        #lines,
        #content,
        detect_newline_style(content),
        table.concat(visible_ranges, ","),
        metadata and metadata.segment_count or #(ranges or {}),
        metadata and metadata.clipped and "true" or "false"
    )
end

-- Render one optional segment separator for multi-segment reads.
-- 为多段读取渲染一个可选的片段分隔线。
local function render_segment_separator(range, total_ranges)
    if total_ranges <= 1 then
        return nil
    end
    return string.format(
        "--- L%d+%d Showing:L%d-L%d Clipped:%s ---",
        range.start_line,
        range.requested_count,
        range.start_line,
        range.end_line,
        range.clipped and "true" or "false"
    )
end

-- Render selected line ranges with optional line-number prefixes.
-- 使用可选行号前缀渲染选中的行范围。
local function render_ranges(lines, ranges, numbered)
    local output = {}
    local total_ranges = #(ranges or {})
    for range_index, range in ipairs(ranges or {}) do
        if range_index > 1 then
            table.insert(output, "")
        end

        local separator = render_segment_separator(range, total_ranges)
        if separator then
            table.insert(output, separator)
        end

        for line_number = range.start_line, range.end_line do
            local line = lines[line_number] or ""
            if numbered then
                table.insert(output, string.format("L%d: %s", line_number, line))
            else
                table.insert(output, line)
            end
        end
    end
    return table.concat(output, "\n")
end

-- Validate optional boolean switches so mistaken JSON types are reported clearly.
-- 校验可选布尔开关，确保错误 JSON 类型会被清晰报告。
local function validate_boolean_argument(value, argument_name)
    if value == nil or type(value) == "boolean" then
        return nil
    end
    return render_error("invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be a boolean when provided", {
        argument = argument_name,
        value = tostring(value),
        actual_type = type(value),
    })
end

-- Validate one batch `files` array shape and enforce the shared maximum file limit.
-- 校验批量 `files` 数组形状，并执行共享的最大文件数量限制。
--
-- Parameters:
--     files: Candidate batch file array.
-- 参数：
--     files：候选批量文件数组。
--
-- Returns:
--     table|nil: Original array-style batch table on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：成功时返回原始数组形式的批量表。
--     string|nil：失败时返回 Markdown 错误文本。
local function validate_batch_files_array(files)
    if type(files) ~= "table" then
        return nil, render_error("invalid_files_argument", "files must be an array of file request objects", {
            actual_type = type(files),
        })
    end

    local count = 0
    for index, _ in ipairs(files) do
        count = index
    end
    if count == 0 then
        if next(files) ~= nil then
            return nil, render_error("invalid_files_argument", "files must be an array of file request objects", {
                actual_type = "table",
            })
        end
        return nil, render_error("invalid_files_argument", "files must contain at least one file request object")
    end
    if count > MAX_BATCH_FILES then
        return nil, render_error("too_many_files", "files may contain at most 10 items", {
            limit = tostring(MAX_BATCH_FILES),
            actual_count = tostring(count),
        })
    end
    return files, nil
end

-- Normalize one single-file read request with inherited numbered defaults.
-- 使用继承的 numbered 默认值规范化一次单文件读取请求。
--
-- Parameters:
--     helpers: Shared helper table.
--     request: Raw single-file read request table.
--     default_numbered: Default line-number flag inherited from the root request.
--     pwd_root: Valid absolute `PWD` directory root shared by the whole call, or nil.
-- 参数：
--     helpers：共享辅助表。
--     request：原始单文件读取请求表。
--     default_numbered：从根请求继承的默认行号标记。
--     pwd_root：整个调用共享的有效绝对 `PWD` 目录根路径，或 nil。
--
-- Returns:
--     table|nil: Normalized read request.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：规范化后的读取请求。
--     string|nil：失败时返回 Markdown 错误文本。
local function normalize_read_request(helpers, request, default_numbered, pwd_root)
    if type(request) ~= "table" then
        return nil, render_error("invalid_files_argument", "each files item must be an object with file and optional range settings", {
            actual_type = type(request),
        })
    end

    local numbered_error = validate_boolean_argument(request.numbered, "numbered")
    if numbered_error then
        return nil, numbered_error
    end

    local target_path, path_error = validate_target_path(helpers, request.file, pwd_root)
    if path_error then
        return nil, path_error
    end

    local numbered = default_numbered ~= false
    if request.numbered ~= nil then
        numbered = request.numbered ~= false
    end
    return {
        file = target_path,
        segments = request.segments,
        lines_rule = request.lines_rule,
        numbered = numbered,
    }, nil
end

-- Collect one normalized list of read requests from single-file or batch input.
-- 从单文件或批量输入中收集一组规范化的读取请求。
--
-- Parameters:
--     helpers: Shared helper table.
--     args: Raw entry argument table from LuaSkills runtime.
-- 参数：
--     helpers：共享辅助表。
--     args：LuaSkills 运行时传入的原始参数表。
--
-- Returns:
--     table|nil: Array-style normalized request list.
--     boolean|nil: True when the caller used batch `files` mode.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：数组形式的规范化请求列表。
--     boolean|nil：调用方使用批量 `files` 模式时返回 true。
--     string|nil：失败时返回 Markdown 错误文本。
local function collect_requests(helpers, args)
    local request = type(args) == "table" and args or {}
    local numbered_error = validate_boolean_argument(request.numbered, "numbered")
    if numbered_error then
        return nil, nil, numbered_error
    end

    local pwd_root, pwd_error = helpers.resolve_pwd_root(ERROR_TITLE, PARAMETER_ERROR_CODES, request.PWD)
    if pwd_error then
        return nil, nil, pwd_error
    end

    if request.files ~= nil then
        if request.file ~= nil or request.segments ~= nil or request.lines_rule ~= nil then
            return nil, true, render_error("conflicting_batch_arguments", "use either file/segments/lines_rule or files, not both", {
                preferred = "files",
            })
        end
        local files, files_error = validate_batch_files_array(request.files)
        if files_error then
            return nil, true, files_error
        end
        local normalized = {}
        local default_numbered = request.numbered ~= false
        for index, item in ipairs(files) do
            local item_request, item_error = normalize_read_request(helpers, item, default_numbered, pwd_root)
            if item_error then
                return nil, true, render_error("invalid_files_argument", "one files item is invalid", {
                    file_index = tostring(index),
                }) .. "\n\n" .. item_error
            end
            table.insert(normalized, item_request)
        end
        return normalized, true, nil
    end

    local single_request, request_error = normalize_read_request(helpers, request, request.numbered ~= false, pwd_root)
    if request_error then
        return nil, false, request_error
    end
    return { single_request }, false, nil
end

-- Return successful read content with a host-managed truncate overflow hint.
-- 返回读取成功内容，并显式声明由宿主管理的 truncate 超限策略。
local function return_read_success(content)
    return content, vulcan.runtime.overflow_type.truncate
end

-- Execute one normalized single-file read request.
-- 执行一次规范化后的单文件读取请求。
--
-- Parameters:
--     request: Normalized read request with resolved path and range settings.
-- 参数：
--     request：带有已解析路径和范围设置的规范化读取请求。
--
-- Returns:
--     string|nil: Successful read payload.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：成功时返回读取结果文本。
--     string|nil：失败时返回 Markdown 错误文本。
local function execute_single_read(request)
    if vulcan.fs.is_dir(request.file) then
        local names, directory_error = read_directory_entries(request.file)
        if directory_error then
            return nil, directory_error
        end
        return render_directory_listing(request.file, names), nil
    end

    local content, read_error = read_file(request.file)
    if read_error then
        return nil, read_error
    end

    local lines = split_lines(content)
    local ranges, metadata_or_error = resolve_display_ranges(request, lines)
    if type(metadata_or_error) == "string" then
        return nil, metadata_or_error
    end

    local header = render_header(request.file, content, lines, ranges or {}, metadata_or_error)
    if #lines == 0 then
        return header .. "\n\n(empty file)", nil
    end
    return header .. "\n" .. render_ranges(lines, ranges or {}, request.numbered ~= false), nil
end

-- Render one combined batch read payload with lightweight separators between file results.
-- 使用轻量分隔线渲染一个合并后的批量读取结果。
--
-- Parameters:
--     outputs: Array-style successful read payload list.
-- 参数：
--     outputs：数组形式的成功读取结果文本列表。
--
-- Returns:
--     string: Combined batch read payload.
-- 返回值：
--     string：合并后的批量读取结果文本。
local function render_batch_result(outputs)
    local lines = {
        string.format("[BatchFileRead Files:%d Limit:%d]", #(outputs or {}), MAX_BATCH_FILES),
    }
    for index, item in ipairs(outputs or {}) do
        table.insert(lines, "")
        table.insert(lines, string.format("--- File %d/%d ---", index, #outputs))
        table.insert(lines, tostring(item or ""))
    end
    return table.concat(lines, "\n")
end

-- Tool entry point invoked by the LuaSkills runtime.
-- LuaSkills 运行时调用的工具入口。
return function(args)
    local helpers = load_shared_file_helpers()
    local requests, is_batch, request_error = collect_requests(helpers, args)
    if request_error then
        return request_error
    end

    local outputs = {}
    for _, request in ipairs(requests or {}) do
        local output, execution_error = execute_single_read(request)
        if execution_error then
            return execution_error
        end
        table.insert(outputs, output)
    end

    if is_batch then
        return return_read_success(render_batch_result(outputs))
    end
    return return_read_success(outputs[1] or "")
end
