--[[
vulcan-file-read
Read one text file or directory with AI-friendly line numbers and compact start,count line rules.
读取一个文本文件或目录，并提供适合 AI 使用的行号与紧凑的 start,count 行规则。
]]

-- Default maximum lines returned when the host does not provide a file-read budget.
-- 当宿主未提供文件读取预算时使用的默认最大返回行数。
local DEFAULT_LIMIT_LINES = 200

-- Maximum safe positive integer accepted by lines_rule parsing.
-- lines_rule 解析接受的最大安全正整数。
local MAX_SAFE_RULE_INTEGER = 9007199254740991

-- Text form of MAX_SAFE_RULE_INTEGER for stable error rendering.
-- MAX_SAFE_RULE_INTEGER 的文本形式，用于稳定渲染错误信息。
local MAX_SAFE_RULE_INTEGER_TEXT = "9007199254740991"

-- Error codes that indicate the caller passed invalid tool arguments.
-- 表示调用方传入无效工具参数的错误码集合。
local PARAMETER_ERROR_CODES = {
    invalid_path = true,
    path_not_found = true,
    environment_variable_not_found = true,
    invalid_environment_variable_reference = true,
    invalid_lines_rule = true,
    range_out_of_bounds = true,
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

-- Render one stable Markdown error payload for invalid input or file failures.
-- 为无效输入或文件失败渲染稳定的 Markdown 错误结果。
local function render_error(error_code, message, details)
    local is_parameter_error = PARAMETER_ERROR_CODES[tostring(error_code or "")] == true
    local lines = {
        "# FILE READ ERROR",
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

-- Validate that a path points to one existing file or directory.
-- 校验路径指向一个已存在的文件或目录。
local function validate_target_path(value)
    if type(value) ~= "string" or trim(value) == "" then
        return nil, render_error("invalid_path", "file must be a non-empty string")
    end

    local target_path, environment_error = expand_environment_path(trim(value), "file")
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

-- Build display ranges from one or more start,count rule segments.
-- 根据一个或多个 start,count 规则片段构造展示行范围。
local function resolve_display_ranges(args, lines)
    local line_count = #lines
    local lines_rule, explicit_rule, input_error = resolve_lines_rule_input(args)
    if input_error then
        return nil, input_error
    end

    local requests, parse_error = parse_lines_rule(lines_rule)
    if parse_error then
        return nil, parse_error
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

-- Return successful read content with a host-managed truncate overflow hint.
-- 返回读取成功内容，并显式声明由宿主管理的 truncate 超限策略。
local function return_read_success(content)
    return content, vulcan.runtime.overflow_type.truncate
end

-- Tool entry point invoked by the LuaSkills runtime.
-- LuaSkills 运行时调用的工具入口。
return function(args)
    local request = type(args) == "table" and args or {}
    local numbered_error = validate_boolean_argument(request.numbered, "numbered")
    if numbered_error then
        return numbered_error
    end

    local target_path, path_error = validate_target_path(request.file)
    if path_error then
        return path_error
    end

    if vulcan.fs.is_dir(target_path) then
        local names, directory_error = read_directory_entries(target_path)
        if directory_error then
            return directory_error
        end
        return return_read_success(render_directory_listing(target_path, names))
    end

    local content, read_error = read_file(target_path)
    if read_error then
        return read_error
    end

    local lines = split_lines(content)
    local ranges, metadata_or_error = resolve_display_ranges(request, lines)
    if type(metadata_or_error) == "string" then
        return metadata_or_error
    end

    local numbered = request.numbered ~= false
    local header = render_header(target_path, content, lines, ranges or {}, metadata_or_error)
    if #lines == 0 then
        return return_read_success(header .. "\n\n(empty file)")
    end
    return return_read_success(header .. "\n" .. render_ranges(lines, ranges or {}, numbered))
end
