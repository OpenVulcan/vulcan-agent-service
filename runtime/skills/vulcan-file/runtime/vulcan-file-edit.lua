--[[
vulcan-file-edit
Preview or apply simple text edits to one file with a deliberately small tool surface.
通过刻意精简的工具面预览或应用一个文件的简单文本编辑。
]]

-- Maximum changed lines rendered in the preview block.
-- 预览区块中最多渲染的变更行数。
local MAX_PREVIEW_LINES = 80

-- Supported edit modes accepted by this tool.
-- 本工具接受的编辑模式集合。
local SUPPORTED_MODES = {
    overwrite = true,
    append = true,
    replace_range = true,
    insert_before = true,
    insert_after = true,
}

-- Error codes that indicate the caller passed invalid tool arguments.
-- 表示调用方传入无效工具参数的错误码集合。
local PARAMETER_ERROR_CODES = {
    invalid_file = true,
    file_is_directory = true,
    file_not_found = true,
    environment_variable_not_found = true,
    invalid_environment_variable_reference = true,
    invalid_mode = true,
    invalid_content = true,
    empty_content_noop = true,
    invalid_apply_argument = true,
    invalid_start_line_argument = true,
    invalid_end_line_argument = true,
    invalid_line_argument = true,
    invalid_line = true,
    invalid_range = true,
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

-- Render one stable Markdown error payload for invalid edits or file failures.
-- 为无效编辑或文件失败渲染稳定的 Markdown 错误结果。
local function render_error(error_code, message, details)
    local is_parameter_error = PARAMETER_ERROR_CODES[tostring(error_code or "")] == true
    local lines = {
        "# FILE EDIT ERROR",
        "",
        "- error: `" .. tostring(error_code or "unknown_error") .. "`",
        "- type: `" .. (is_parameter_error and "parameter_error" or "runtime_error") .. "`",
        "- message: " .. tostring(message or "unknown file edit error"),
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

-- Validate an optional boolean argument and report type mistakes clearly.
-- 校验可选布尔参数，并清晰报告类型错误。
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

-- Parse an integer argument without silently accepting strings or fractional numbers.
-- 解析整数参数，避免静默接受字符串或小数。
local function parse_integer_argument(value, argument_name)
    if type(value) ~= "number" or value ~= math.floor(value) then
        return nil, render_error("invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be an integer number", {
            argument = argument_name,
            value = tostring(value),
            actual_type = type(value),
        })
    end
    return value, nil
end

-- Validate the common edit request fields.
-- 校验通用编辑请求字段。
local function validate_request(args)
    local request = type(args) == "table" and args or {}
    if type(request.file) ~= "string" or trim(request.file) == "" then
        return nil, render_error("invalid_file", "file must be a non-empty string")
    end
    if type(request.mode) ~= "string" or not SUPPORTED_MODES[request.mode] then
        return nil, render_error("invalid_mode", "mode must be overwrite, append, replace_range, insert_before, or insert_after")
    end
    if type(request.content) ~= "string" then
        return nil, render_error("invalid_content", "content must be a string")
    end
    if (request.mode == "append" or request.mode == "insert_before" or request.mode == "insert_after") and request.content == "" then
        return nil, render_error("empty_content_noop", "append and insert modes require non-empty content")
    end
    local file_path, environment_error = expand_environment_path(trim(request.file), "file")
    if environment_error then
        return nil, environment_error
    end
    file_path = trim(file_path)
    if file_path == "" then
        return nil, render_error("invalid_file", "file must resolve to a non-empty string")
    end
    local apply_error = validate_optional_boolean(request.apply, "apply")
    if apply_error then
        return nil, apply_error
    end
    return {
        file = file_path,
        mode = request.mode,
        content = request.content,
        start_line = request.start_line,
        end_line = request.end_line,
        line = request.line,
        apply = request.apply == true,
    }, nil
end

-- Read an existing file or return an empty string for overwrite creation.
-- 读取现有文件，或在覆盖创建场景中返回空字符串。
local function read_existing_file(file_path, mode)
    if vulcan.fs.exists(file_path) then
        if vulcan.fs.is_dir(file_path) then
            return nil, render_error("file_is_directory", "file must point to a regular file", { file = file_path })
        end
        local ok, content = pcall(vulcan.fs.read, file_path)
        if not ok then
            return nil, render_error("file_read_failed", tostring(content), { file = file_path })
        end
        return tostring(content or ""), nil
    end

    if mode == "overwrite" then
        return "", nil
    end
    return nil, render_error("file_not_found", "file does not exist", { file = file_path })
end

-- Detect the newline sequence that should be preserved for generated content.
-- 检测生成内容时应保留的换行序列。
local function detect_newline_sequence(content)
    if tostring(content or ""):find("\r\n", 1, true) then
        return "\r\n"
    end
    return "\n"
end

-- Normalize incoming content to the target file newline sequence.
-- 将输入内容规范化为目标文件的换行序列。
local function normalize_newlines(content, newline)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    if newline == "\r\n" then
        normalized = normalized:gsub("\n", "\r\n")
    end
    return normalized
end

-- Split text into logical lines and preserve whether it ended with a newline.
-- 将文本拆分为逻辑行，并保留其是否以换行结尾的信息。
local function split_lines_with_final_newline(content)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    local had_final_newline = normalized ~= "" and ends_with(normalized, "\n")
    if normalized == "" then
        return {}, false
    end
    if had_final_newline then
        normalized = normalized:sub(1, -2)
    end
    local lines = {}
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end
    return lines, had_final_newline
end

-- Join logical lines back into text while preserving final-newline intent.
-- 将逻辑行重新拼接为文本，并保留末尾换行意图。
local function join_lines(lines, final_newline, newline)
    local text = table.concat(lines or {}, newline)
    if final_newline and text ~= "" then
        text = text .. newline
    end
    return text
end

-- Split inserted content into logical lines for line-based operations.
-- 将插入内容拆分为适合行级操作的逻辑行。
local function split_insert_content(content)
    local lines, had_final_newline = split_lines_with_final_newline(content)
    if #lines == 0 and had_final_newline then
        return { "" }
    end
    return lines
end

-- Validate a 1-based line number for insert operations.
-- 校验插入操作使用的 1-based 行号。
local function validate_insert_line(value, line_count)
    if line_count < 1 then
        return nil, render_error("invalid_line", "insert_before and insert_after require an existing line; use append or overwrite for empty files")
    end
    local line, line_argument_error = parse_integer_argument(value, "line")
    if line_argument_error then
        return nil, line_argument_error
    end
    if line == nil or line < 1 or line > line_count then
        return nil, render_error("invalid_line", "line must point to an existing 1-based line")
    end
    return line, nil
end

-- Validate a 1-based closed line range for replacement operations.
-- 校验替换操作使用的 1-based 闭区间行范围。
local function validate_replace_range(start_value, end_value, line_count)
    if start_value == nil or end_value == nil then
        return nil, nil, render_error("invalid_range", "replace_range requires start_line and end_line")
    end
    local start_line, start_argument_error = parse_integer_argument(start_value, "start_line")
    if start_argument_error then
        return nil, nil, start_argument_error
    end
    local end_line, end_argument_error = parse_integer_argument(end_value, "end_line")
    if end_argument_error then
        return nil, nil, end_argument_error
    end
    if start_line < 1 or end_line < start_line or end_line > line_count then
        return nil, nil, render_error("invalid_range", "start_line/end_line must describe an existing 1-based line range")
    end
    return start_line, end_line, nil
end

-- Build a same-directory sidecar path for temporary writes and rollback backups.
-- 构造同目录旁路文件路径，用于临时写入和回滚备份。
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

-- Remove one sidecar file and ignore cleanup failures.
-- 删除一个旁路文件，并忽略清理失败。
local function safe_remove_file(file_path)
    if type(file_path) ~= "string" or file_path == "" then
        return
    end
    if vulcan.fs.exists(file_path) then
        pcall(os.remove, file_path)
    end
end

-- Rename one file and normalize platform-specific failure messages.
-- 重命名一个文件，并规范化不同平台的失败信息。
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

-- Apply the requested edit to text and return the new text plus changed line span.
-- 对文本应用请求的编辑，并返回新文本与变更行范围。
local function build_edited_content(request, original_content)
    local newline = detect_newline_sequence(original_content)
    local normalized_insert = normalize_newlines(request.content, newline)
    local original_lines, had_final_newline = split_lines_with_final_newline(original_content)
    local edit_lines = split_insert_content(normalized_insert)

    if request.mode == "overwrite" then
        return normalized_insert, {
            start_line = 1,
            end_line = math.max(1, #split_insert_content(normalized_insert)),
            original_start_line = 1,
            original_end_line = math.max(1, #original_lines),
            inserted_line_count = #split_insert_content(normalized_insert),
        }, nil
    end

    if request.mode == "append" then
        local prefix = original_content
        if prefix ~= "" and not ends_with(prefix:gsub("\r\n", "\n"), "\n") then
            prefix = prefix .. newline
        end
        return prefix .. normalized_insert, {
            start_line = #original_lines + 1,
            end_line = #original_lines + math.max(1, #edit_lines),
            original_start_line = #original_lines + 1,
            original_end_line = #original_lines,
            inserted_line_count = #edit_lines,
        }, nil
    end

    if request.mode == "replace_range" then
        local start_line, end_line, range_error = validate_replace_range(request.start_line, request.end_line, #original_lines)
        if range_error then
            return nil, nil, range_error
        end
        for index = end_line, start_line, -1 do
            table.remove(original_lines, index)
        end
        for index = #edit_lines, 1, -1 do
            table.insert(original_lines, start_line, edit_lines[index])
        end
        return join_lines(original_lines, had_final_newline, newline), {
            start_line = start_line,
            end_line = start_line + math.max(0, #edit_lines - 1),
            original_start_line = start_line,
            original_end_line = end_line,
            inserted_line_count = #edit_lines,
        }, nil
    end

    local line, line_error = validate_insert_line(request.line, #original_lines)
    if line_error then
        return nil, nil, line_error
    end

    local insert_index = request.mode == "insert_before" and line or line + 1
    for index = #edit_lines, 1, -1 do
        table.insert(original_lines, insert_index, edit_lines[index])
    end
    return join_lines(original_lines, had_final_newline, newline), {
        start_line = insert_index,
        end_line = insert_index + math.max(0, #edit_lines - 1),
        original_start_line = line,
        original_end_line = line,
        inserted_line_count = #edit_lines,
    }, nil
end

-- Append one preview line while respecting the preview line budget.
-- 在遵守预览行数预算的前提下追加一行预览内容。
local function append_preview_line(output, rendered_count, line)
    if rendered_count >= MAX_PREVIEW_LINES then
        return rendered_count, false
    end
    table.insert(output, line)
    return rendered_count + 1, true
end

-- Render one inclusive context range from a line table.
-- 从行表中渲染一个闭区间上下文范围。
local function render_context_range(output, rendered_count, lines, start_line, end_line, prefix)
    local safe_start = math.max(1, start_line)
    local safe_end = math.min(#lines, end_line)
    for line_number = safe_start, safe_end do
        local keep_going
        rendered_count, keep_going = append_preview_line(output, rendered_count, string.format("%sL%d: %s", prefix, line_number, lines[line_number] or ""))
        if not keep_going then
            return rendered_count, false
        end
    end
    return rendered_count, true
end

-- Render an operation-oriented preview so inserts and deletions do not look like shifted replacements.
-- 渲染面向操作的预览，避免插入和删除被错位显示成替换。
local function render_preview(request, original_content, edited_content, changed_span)
    local original_lines = select(1, split_lines_with_final_newline(original_content))
    local edited_lines = select(1, split_lines_with_final_newline(edited_content))
    local original_start = changed_span.original_start_line or changed_span.start_line
    local original_end = changed_span.original_end_line or changed_span.end_line
    local edited_start = changed_span.start_line
    local edited_end = changed_span.end_line
    local before_context_start = original_start - 3
    local before_context_end = original_start - 1
    local after_context_start = original_end + 1
    local after_context_end = original_end + 3
    local output = {
        "## Preview",
        "",
        "```diff",
    }

    if request.mode == "insert_after" then
        before_context_start = original_start - 2
        before_context_end = original_start
    elseif request.mode == "insert_before" then
        after_context_start = original_start
        after_context_end = original_start + 2
    end

    local rendered = 0
    local keep_going
    rendered, keep_going = render_context_range(output, rendered, original_lines, before_context_start, before_context_end, " ")

    if keep_going and (request.mode == "replace_range" or request.mode == "overwrite") then
        rendered, keep_going = render_context_range(output, rendered, original_lines, original_start, original_end, "-")
    end

    if keep_going and (changed_span.inserted_line_count or 0) > 0 then
        rendered, keep_going = render_context_range(output, rendered, edited_lines, edited_start, edited_end, "+")
    end

    if keep_going and request.mode ~= "append" then
        rendered, keep_going = render_context_range(output, rendered, original_lines, after_context_start, after_context_end, " ")
    end

    if not keep_going then
        table.insert(output, "... preview truncated ...")
    end

    if rendered == 0 then
        if request.content == "" then
            table.insert(output, "(no inserted lines)")
        else
            table.insert(output, "(no visible line changes)")
        end
    end

    table.insert(output, "```")
    return table.concat(output, "\n")
end

-- Render the final edit result including whether the write was applied.
-- 渲染最终编辑结果，并说明是否已经写入。
local function render_result(request, original_content, edited_content, changed_span)
    local status = request.apply and "APPLIED" or "PREVIEW_ONLY"
    local original_lines = select(1, split_lines_with_final_newline(original_content))
    local edited_lines = select(1, split_lines_with_final_newline(edited_content))
    -- Only render an original span when the original file actually contained that line range.
    -- 仅当原文件真实包含该行范围时才渲染原始范围。
    local original_start_line = changed_span.original_start_line or changed_span.start_line
    local original_end_line = changed_span.original_end_line or changed_span.end_line
    local original_span = "none"
    if original_start_line >= 1 and original_start_line <= original_end_line and original_end_line <= #original_lines then
        original_span = string.format("L%d-L%d", original_start_line, original_end_line)
    end
    local edited_span = "none"
    if (changed_span.inserted_line_count or 0) > 0 then
        edited_span = string.format("L%d-L%d", changed_span.start_line, changed_span.end_line)
    end
    local lines = {
        "# FILE EDIT RESULT",
        "",
        "- status: `" .. status .. "`",
        "- file: `" .. request.file .. "`",
        "- mode: `" .. request.mode .. "`",
        "- original_lines: `" .. tostring(#original_lines) .. "`",
        "- edited_lines: `" .. tostring(#edited_lines) .. "`",
        "- original_span: `" .. original_span .. "`",
        "- edited_span: `" .. edited_span .. "`",
        "- changed_span: `original " .. original_span .. " -> edited " .. edited_span .. "`",
        "",
        render_preview(request, original_content, edited_content, changed_span),
    }
    return table.concat(lines, "\n")
end

-- Write edited content through a temp-file swap with rollback best effort.
-- 通过临时文件替换写入编辑内容，并尽最大努力回滚失败。
local function write_file(file_path, content, original_content)
    local temp_path = build_sidecar_file_path(file_path, "tmp")
    local backup_path = build_sidecar_file_path(file_path, "bak")
    local existed_before = vulcan.fs.exists(file_path)

    local temp_ok, temp_error = pcall(vulcan.fs.write, temp_path, content)
    if not temp_ok then
        safe_remove_file(temp_path)
        return render_error("temp_write_failed", tostring(temp_error), { file = file_path, temp_file = temp_path })
    end

    if existed_before then
        local backed_up, backup_error = rename_file(file_path, backup_path)
        if not backed_up then
            safe_remove_file(temp_path)
            return render_error("backup_creation_failed", tostring(backup_error), { file = file_path, backup_file = backup_path })
        end
    end

    local swapped, swap_error = rename_file(temp_path, file_path)
    if not swapped then
        safe_remove_file(temp_path)
        if existed_before then
            local restored = rename_file(backup_path, file_path)
            if not restored then
                pcall(vulcan.fs.write, file_path, original_content)
            end
        end
        return render_error("temp_swap_failed", tostring(swap_error), { file = file_path, temp_file = temp_path, backup_file = backup_path })
    end

    if existed_before then
        safe_remove_file(backup_path)
    end

    return nil
end

-- Tool entry point invoked by the LuaSkills runtime.
-- LuaSkills 运行时调用的工具入口。
return function(args)
    local request, validation_error = validate_request(args)
    if validation_error then
        return validation_error
    end

    local original_content, read_error = read_existing_file(request.file, request.mode)
    if read_error then
        return read_error
    end

    local edited_content, changed_span, edit_error = build_edited_content(request, original_content)
    if edit_error then
        return edit_error
    end

    if request.apply then
        local write_error = write_file(request.file, edited_content, original_content)
        if write_error then
            return write_error
        end
    end

    return render_result(request, original_content, edited_content, changed_span)
end
