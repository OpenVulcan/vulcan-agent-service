--[[
shared_file
Provide shared helpers for vulcan-file create and edit entries.
为 vulcan-file 的 create 与 edit 入口提供共享辅助能力。
]]

-- Default maximum number of preview lines rendered in one diff-style block.
-- 单个类 diff 预览区块默认最多渲染的行数。
local DEFAULT_MAX_PREVIEW_LINES = 80

-- Maximum number of files accepted by one batch file operation.
-- 单次批量文件操作允许处理的最大文件数量。
local MAX_BATCH_FILES = 10

-- Stable placeholder text returned when one deleted file should be treated as binary.
-- 当某个被删除文件应按二进制处理时返回的稳定占位文本。
local BINARY_FILE_PLACEHOLDER = "Binary file"

-- Stable line limit after which delete host records switch to truncated content mode.
-- 删除宿主记录切换到截断内容模式的稳定行数阈值。
local DELETE_TRUNCATE_LINE_LIMIT = 500

-- Stable number of leading and trailing lines kept in truncated delete host records.
-- 截断删除宿主记录中稳定保留的首尾行数。
local DELETE_TRUNCATED_EDGE_LINE_COUNT = 50

-- Return whether one value is a non-empty string after trimming.
-- 返回一个值在去除首尾空白后是否为非空字符串。
--
-- Parameters:
--     value: Any value that will be converted to text.
-- 参数：
--     value：将被转换为文本的任意值。
--
-- Returns:
--     boolean: True when the converted text is not empty after trimming.
-- 返回值：
--     boolean：转换后的文本去除首尾空白后非空时返回 true。
local function has_trimmed_text(value)
    return tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", "") ~= ""
end

-- Remove surrounding whitespace from one value converted to text.
-- 将一个值转换为文本后移除首尾空白。
--
-- Parameters:
--     value: Any value that will be rendered as text.
-- 参数：
--     value：将被渲染为文本的任意值。
--
-- Returns:
--     string: Trimmed text form of the input value.
-- 返回值：
--     string：输入值去除首尾空白后的文本形式。
local function trim(value)
    return (tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

-- Check whether one string starts with the given prefix.
-- 检查一个字符串是否以给定前缀开头。
--
-- Parameters:
--     text: Source text to inspect.
--     prefix: Prefix that must appear at the beginning.
-- 参数：
--     text：需要检查的源文本。
--     prefix：必须出现在开头的前缀。
--
-- Returns:
--     boolean: True when the text begins with the prefix.
-- 返回值：
--     boolean：当文本以前缀开头时返回 true。
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #tostring(prefix or "")) == tostring(prefix or "")
end

-- Check whether one string ends with the given suffix.
-- 检查一个字符串是否以给定后缀结尾。
--
-- Parameters:
--     text: Source text to inspect.
--     suffix: Suffix that must appear at the end.
-- 参数：
--     text：需要检查的源文本。
--     suffix：必须出现在末尾的后缀。
--
-- Returns:
--     boolean: True when the text ends with the suffix.
-- 返回值：
--     boolean：当文本以后缀结尾时返回 true。
local function ends_with(text, suffix)
    return tostring(text or ""):sub(-#tostring(suffix or "")) == tostring(suffix or "")
end

-- Join two path fragments with the host path helper when it exists.
-- 当宿主路径辅助存在时优先使用它拼接两个路径片段。
--
-- Parameters:
--     left: Left path fragment.
--     right: Right path fragment.
-- 参数：
--     left：左侧路径片段。
--     right：右侧路径片段。
--
-- Returns:
--     string: Joined path text.
-- 返回值：
--     string：拼接后的路径文本。
local function join_path(left, right)
    if vulcan and vulcan.path and type(vulcan.path.join) == "function" then
        return vulcan.path.join(left, right)
    end
    local separator = package.config:sub(1, 1)
    return tostring(left or "") .. separator .. tostring(right or "")
end

-- Translate Windows device-path spellings such as `\\?\C:\x` or `//?C:/x` into normal absolute paths.
-- 将 `\\?\C:\x` 或 `//?C:/x` 这类 Windows 设备路径写法转译为普通绝对路径。
--
-- Parameters:
--     path: Raw caller path text.
-- 参数：
--     path：调用方传入的原始路径文本。
--
-- Returns:
--     string: Path text translated into a regular host path form when needed.
-- 返回值：
--     string：在需要时转译为普通宿主路径形式后的路径文本。
local function translate_windows_device_path(path)
    local text = tostring(path or "")
    local unc_remainder = text:match("^[\\\\/][\\\\/]%?[\\\\/][Uu][Nn][Cc][\\\\/](.+)$")
    if unc_remainder then
        return "//" .. unc_remainder
    end

    local drive_remainder = text:match("^[\\\\/][\\\\/]%?[\\\\/]([A-Za-z]:.*)$")
    if drive_remainder then
        return drive_remainder
    end

    local shorthand_drive_remainder = text:match("^[\\\\/][\\\\/]%?([A-Za-z]:.*)$")
    if shorthand_drive_remainder then
        return shorthand_drive_remainder
    end

    return text
end

-- Normalize one caller path text before absolute-path checks or filesystem validation.
-- 在绝对路径检查或文件系统校验前规范化调用方路径文本。
--
-- Parameters:
--     path: Raw caller path text.
-- 参数：
--     path：调用方传入的原始路径文本。
--
-- Returns:
--     string: Trimmed path text with supported Windows device prefixes translated.
-- 返回值：
--     string：已去除首尾空白并转译受支持 Windows 设备前缀后的路径文本。
local function normalize_path_text(path)
    return translate_windows_device_path(trim(path))
end

-- Return whether one path text is exactly a Windows drive root.
-- 返回一个路径文本是否恰好是 Windows 盘符根目录。
--
-- Parameters:
--     path: Path text to inspect.
-- 参数：
--     path：需要检查的路径文本。
--
-- Returns:
--     boolean: True when the path is a drive root such as `C:\` or `D:/`.
-- 返回值：
--     boolean：当路径是 `C:\` 或 `D:/` 这类盘符根目录时返回 true。
local function is_windows_drive_root_path(path)
    return normalize_path_text(path):match("^[A-Za-z]:[\\\\/]?$") ~= nil
end

-- Return whether one path text is exactly a UNC share root.
-- 返回一个路径文本是否恰好是 UNC 共享根目录。
--
-- Parameters:
--     path: Path text to inspect.
-- 参数：
--     path：需要检查的路径文本。
--
-- Returns:
--     boolean: True when the path is a UNC root such as `//server/share`.
-- 返回值：
--     boolean：当路径是 `//server/share` 这类 UNC 根目录时返回 true。
local function is_unc_root_path(path)
    local normalized = normalize_path_text(path):gsub("\\", "/")
    normalized = normalized:gsub("/+$", "")
    return normalized:match("^//[^/]+/[^/]+$") ~= nil
end

-- Remove trailing path separators while preserving filesystem roots that need them.
-- 移除路径末尾分隔符，同时保留必须保留分隔符的文件系统根路径。
--
-- Parameters:
--     path: Path text that may contain trailing separators.
-- 参数：
--     path：可能带有尾部分隔符的路径文本。
--
-- Returns:
--     string: Path text without redundant trailing separators.
-- 返回值：
--     string：去除冗余尾部分隔符后的路径文本。
local function trim_trailing_path_separators(path)
    local normalized = normalize_path_text(path)
    if normalized == "/" or normalized == "\\" then
        return normalized
    end
    if is_windows_drive_root_path(normalized) or is_unc_root_path(normalized) then
        return normalized
    end

    local trimmed_path = normalized:gsub("[\\\\/]+$", "")
    if trimmed_path == "" then
        return normalized
    end
    return trimmed_path
end

-- Determine whether one path text is already absolute on common Windows and POSIX forms.
-- 判断一个路径文本是否已是常见 Windows 或 POSIX 绝对路径形式。
--
-- Parameters:
--     path: Path text to inspect.
-- 参数：
--     path：需要检查的路径文本。
--
-- Returns:
--     boolean: True when the path is absolute.
-- 返回值：
--     boolean：当路径为绝对路径时返回 true。
local function is_absolute_path(path)
    local text = normalize_path_text(path)
    if text:match("^[A-Za-z]:[\\/]") then
        return true
    end
    if text:match("^[\\/][\\/]") then
        return true
    end
    if starts_with(text, "/") then
        return true
    end
    return false
end

-- Resolve one caller path into an absolute path rooted at the current runtime cwd.
-- 将一个调用方路径解析为以当前运行时工作目录为基准的绝对路径。
--
-- Parameters:
--     path: Original caller path text.
-- 参数：
--     path：原始调用方路径文本。
--
-- Returns:
--     string: Absolute path text.
-- 返回值：
--     string：绝对路径文本。
local function resolve_absolute_path(path)
    local text = normalize_path_text(path)
    if is_absolute_path(text) then
        return text
    end
    local runtime_cwd = "."
    if vulcan and vulcan.runtime and type(vulcan.runtime.cwd) == "function" then
        runtime_cwd = tostring(vulcan.runtime.cwd() or ".")
    end
    return join_path(runtime_cwd, text)
end

-- Extract one parent directory path from a file path.
-- 从文件路径中提取父目录路径。
--
-- Parameters:
--     file_path: File path that should contain one final basename segment.
-- 参数：
--     file_path：应当包含最终文件名片段的文件路径。
--
-- Returns:
--     string|nil: Parent directory path when available, otherwise nil.
-- 返回值：
--     string|nil：存在父目录时返回目录路径，否则返回 nil。
local function extract_parent_dir(file_path)
    return tostring(file_path or ""):match("^(.*[\\/])[^\\/]+$")
end

-- Render one stable Markdown error payload for file tools.
-- 为文件工具渲染一个稳定的 Markdown 错误结果。
--
-- Parameters:
--     error_title: Visible Markdown title such as FILE EDIT ERROR.
--     parameter_error_codes: Set-like table of parameter error codes.
--     error_code: Stable error identifier.
--     message: Human-readable error message.
--     details: Optional key-value details rendered as bullet lines.
-- 参数：
--     error_title：可见的 Markdown 标题，例如 FILE EDIT ERROR。
--     parameter_error_codes：参数错误码集合表。
--     error_code：稳定错误标识。
--     message：可读错误信息。
--     details：作为项目符号渲染的可选键值详情。
--
-- Returns:
--     string: Markdown error payload.
-- 返回值：
--     string：Markdown 错误结果文本。
local function render_error(error_title, parameter_error_codes, error_code, message, details)
    local is_parameter_error = parameter_error_codes[tostring(error_code or "")] == true
    local lines = {
        "# " .. tostring(error_title or "FILE ERROR"),
        "",
        "- error: `" .. tostring(error_code or "unknown_error") .. "`",
        "- type: `" .. (is_parameter_error and "parameter_error" or "runtime_error") .. "`",
        "- message: " .. tostring(message or "unknown file error"),
    }
    if is_parameter_error then
        table.insert(lines, "- correction: adjust the tool arguments and call again")
    end
    for key, value in pairs(details or {}) do
        table.insert(lines, "- " .. tostring(key) .. ": `" .. tostring(value) .. "`")
    end
    return table.concat(lines, "\n")
end

-- Expand `${env:NAME}` placeholders before later path resolution steps.
-- 在后续路径解析步骤前展开 `${env:NAME}` 占位符。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     path: Caller path text to expand.
--     field_name: Argument name used in error output.
--     allow_empty: When true, an empty resolved string is returned instead of raising an error.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     path：需要展开的调用方路径文本。
--     field_name：错误输出中使用的参数名。
--     allow_empty：为 true 时，解析后为空字符串不会报错，而是直接返回空字符串。
--
-- Returns:
--     string|nil: Expanded path text on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：成功时返回展开后的路径文本。
--     string|nil：失败时返回 Markdown 错误文本。
local function expand_environment_text(error_title, parameter_error_codes, path, field_name, allow_empty)
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
        return nil, render_error(error_title, parameter_error_codes, "environment_variable_not_found", "environment variable referenced in path is not defined", {
            field = tostring(field_name or "path"),
            variable = tostring(unresolved_variable),
            path = source,
        })
    end
    if expanded:find("${env:", 1, true) ~= nil then
        return nil, render_error(error_title, parameter_error_codes, "invalid_environment_variable_reference", "environment variable path placeholder must use ${env:NAME} syntax", {
            field = tostring(field_name or "path"),
            path = source,
        })
    end
    local resolved = trim(expanded)
    if resolved == "" then
        if allow_empty == true then
            return "", nil
        end
        return nil, render_error(error_title, parameter_error_codes, "invalid_file", "file must resolve to a non-empty string", {
            field = tostring(field_name or "path"),
        })
    end
    return resolved, nil
end

-- Resolve one optional `PWD` convention value into a validated absolute directory root.
-- 将一个可选 `PWD` 公约参数解析为已校验的绝对目录根路径。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     pwd_value: Caller-provided `PWD` value.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     pwd_value：调用方传入的 `PWD` 值。
--
-- Returns:
--     string|nil: Valid absolute directory root when `PWD` is usable, otherwise nil.
--     string|nil: Markdown error text on malformed `PWD`.
-- 返回值：
--     string|nil：`PWD` 可用时返回有效绝对目录根路径，否则返回 nil。
--     string|nil：`PWD` 结构非法时返回 Markdown 错误文本。
local function resolve_pwd_root(error_title, parameter_error_codes, pwd_value)
    if pwd_value == nil then
        return nil, nil
    end
    if type(pwd_value) ~= "string" then
        return nil, render_error(error_title, parameter_error_codes, "invalid_pwd_argument", "PWD must be a string when provided", {
            field = "PWD",
            actual_type = type(pwd_value),
        })
    end

    local expanded_pwd, pwd_error = expand_environment_text(error_title, parameter_error_codes, pwd_value, "PWD", true)
    if pwd_error then
        return nil, pwd_error
    end
    if not has_trimmed_text(expanded_pwd) then
        return nil, nil
    end

    local normalized_pwd = trim_trailing_path_separators(expanded_pwd)
    if not is_absolute_path(normalized_pwd) then
        return nil, render_error(error_title, parameter_error_codes, "invalid_pwd_argument", "PWD must be an absolute directory path when provided", {
            field = "PWD",
            pwd = normalized_pwd,
        })
    end
    if not vulcan.fs.exists(normalized_pwd) or not vulcan.fs.is_dir(normalized_pwd) then
        return nil, nil
    end
    return normalized_pwd, nil
end

-- Resolve one caller path using absolute-path rules plus the optional `PWD` root convention.
-- 使用绝对路径规则与可选 `PWD` 根目录公约解析调用方路径。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     path: Caller path text to resolve.
--     field_name: Argument name rendered in error output.
--     pwd_root: Validated absolute `PWD` directory root, or nil.
--     empty_error_code: Error code used when the resolved path becomes empty.
--     empty_message: Human-readable message used when the resolved path becomes empty.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     path：需要解析的调用方路径文本。
--     field_name：错误输出中使用的参数名。
--     pwd_root：已校验的绝对 `PWD` 目录根路径，或 nil。
--     empty_error_code：解析后路径为空时使用的错误码。
--     empty_message：解析后路径为空时使用的可读错误信息。
--
-- Returns:
--     string|nil: Absolute path resolved from the caller input.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：从调用方输入解析出的绝对路径。
--     string|nil：失败时返回 Markdown 错误文本。
local function resolve_input_path(error_title, parameter_error_codes, path, field_name, pwd_root, empty_error_code, empty_message)
    local expanded_path, environment_error = expand_environment_text(error_title, parameter_error_codes, path, field_name)
    if environment_error then
        return nil, environment_error
    end

    local normalized_path = normalize_path_text(expanded_path)
    if normalized_path == "" then
        return nil, render_error(error_title, parameter_error_codes, tostring(empty_error_code or "invalid_file"), tostring(empty_message or "file must resolve to a non-empty string"), {
            field = tostring(field_name or "path"),
        })
    end
    if is_absolute_path(normalized_path) then
        return normalized_path, nil
    end
    if has_trimmed_text(pwd_root) then
        return join_path(pwd_root, normalized_path), nil
    end
    return nil, render_error(error_title, parameter_error_codes, "relative_path_requires_pwd", "relative paths require a valid PWD; otherwise pass an absolute path", {
        field = tostring(field_name or "path"),
        path = normalized_path,
    })
end

-- Expand `${env:NAME}` placeholders and resolve the final path with the optional `PWD` convention root.
-- 展开 `${env:NAME}` 占位符，并使用可选 `PWD` 公约根目录解析最终路径。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     path: Caller path text to resolve.
--     field_name: Argument name used in error output.
--     pwd_root: Validated absolute `PWD` directory root, or nil.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     path：需要解析的调用方路径文本。
--     field_name：错误输出中使用的参数名。
--     pwd_root：已校验的绝对 `PWD` 目录根路径，或 nil。
--
-- Returns:
--     string|nil: Absolute resolved path on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     string|nil：成功时返回绝对解析路径。
--     string|nil：失败时返回 Markdown 错误文本。
local function expand_environment_path(error_title, parameter_error_codes, path, field_name, pwd_root)
    return resolve_input_path(
        error_title,
        parameter_error_codes,
        path,
        field_name,
        pwd_root,
        "invalid_file",
        "file must resolve to a non-empty string"
    )
end

-- Validate an optional boolean argument and report type mistakes clearly.
-- 校验一个可选布尔参数，并清晰报告类型错误。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     value: Candidate boolean value.
--     argument_name: Argument name rendered in the error message.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     value：待校验的布尔候选值。
--     argument_name：错误信息中展示的参数名。
--
-- Returns:
--     string|nil: Markdown error text when the value is invalid, otherwise nil.
-- 返回值：
--     string|nil：值非法时返回 Markdown 错误文本，否则返回 nil。
local function validate_optional_boolean(error_title, parameter_error_codes, value, argument_name)
    if value == nil or type(value) == "boolean" then
        return nil
    end
    return render_error(error_title, parameter_error_codes, "invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be a boolean when provided", {
        argument = tostring(argument_name),
        value = tostring(value),
        actual_type = type(value),
    })
end

-- Parse one integer argument without silently accepting strings or fractional numbers.
-- 解析一个整数参数，避免静默接受字符串或小数。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     value: Candidate integer value.
--     argument_name: Argument name rendered in the error message.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     value：待解析的整数候选值。
--     argument_name：错误信息中展示的参数名。
--
-- Returns:
--     number|nil: Parsed integer value on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     number|nil：成功时返回解析后的整数值。
--     string|nil：失败时返回 Markdown 错误文本。
local function parse_integer_argument(error_title, parameter_error_codes, value, argument_name)
    if type(value) ~= "number" or value ~= math.floor(value) then
        return nil, render_error(error_title, parameter_error_codes, "invalid_" .. tostring(argument_name) .. "_argument", argument_name .. " must be an integer number", {
            argument = tostring(argument_name),
            value = tostring(value),
            actual_type = type(value),
        })
    end
    return value, nil
end

-- Validate one batch `files` array shape and enforce the shared maximum file limit.
-- 校验批量 `files` 数组形状，并执行共享的最大文件数量限制。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     files: Candidate batch file array.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     files：候选批量文件数组。
--
-- Returns:
--     table|nil: Original array-style batch table on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：成功时返回原始数组形式的批量表。
--     string|nil：失败时返回 Markdown 错误文本。
local function validate_batch_files_array(error_title, parameter_error_codes, files)
    if type(files) ~= "table" then
        return nil, render_error(error_title, parameter_error_codes, "invalid_files_argument", "files must be an array of file request objects", {
            actual_type = type(files),
        })
    end
    local count = 0
    for index, _ in ipairs(files) do
        count = index
    end
    if count == 0 then
        if next(files) ~= nil then
            return nil, render_error(error_title, parameter_error_codes, "invalid_files_argument", "files must be an array of file request objects", {
                actual_type = "table",
            })
        end
        return nil, render_error(error_title, parameter_error_codes, "invalid_files_argument", "files must contain at least one file request object")
    end
    if count > MAX_BATCH_FILES then
        return nil, render_error(error_title, parameter_error_codes, "too_many_files", "files may contain at most 10 items", {
            limit = tostring(MAX_BATCH_FILES),
            actual_count = tostring(count),
        })
    end
    return files, nil
end

-- Detect the newline sequence that should be preserved for generated content.
-- 检测生成内容时应保留的换行序列。
--
-- Parameters:
--     content: Existing file text used as the preservation baseline.
-- 参数：
--     content：作为保留基准的现有文件文本。
--
-- Returns:
--     string: `\r\n` when Windows newlines are detected, otherwise `\n`.
-- 返回值：
--     string：检测到 Windows 换行时返回 `\r\n`，否则返回 `\n`。
local function detect_newline_sequence(content)
    if tostring(content or ""):find("\r\n", 1, true) then
        return "\r\n"
    end
    return "\n"
end

-- Normalize incoming text to the selected newline sequence.
-- 将输入文本规范化为选定的换行序列。
--
-- Parameters:
--     content: Source text to normalize.
--     newline: Target newline sequence.
-- 参数：
--     content：待规范化的源文本。
--     newline：目标换行序列。
--
-- Returns:
--     string: Text with normalized newline characters.
-- 返回值：
--     string：换行字符已规范化后的文本。
local function normalize_newlines(content, newline)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    if newline == "\r\n" then
        normalized = normalized:gsub("\n", "\r\n")
    end
    return normalized
end

-- Split text into logical lines while preserving the trailing-newline flag.
-- 将文本拆分为逻辑行，同时保留尾随换行标记。
--
-- Parameters:
--     content: Text content to split.
-- 参数：
--     content：需要拆分的文本内容。
--
-- Returns:
--     table: Array-style logical line table.
--     boolean: True when the original content ended with a newline.
-- 返回值：
--     table：数组形式的逻辑行表。
--     boolean：原内容以换行结尾时返回 true。
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
--
-- Parameters:
--     lines: Array-style logical lines.
--     final_newline: Whether the output should end with one newline.
--     newline: Target newline sequence.
-- 参数：
--     lines：数组形式的逻辑行。
--     final_newline：输出是否应以一个换行结尾。
--     newline：目标换行序列。
--
-- Returns:
--     string: Reconstructed text content.
-- 返回值：
--     string：重新构造后的文本内容。
local function join_lines(lines, final_newline, newline)
    local text = table.concat(lines or {}, newline)
    if final_newline and text ~= "" then
        text = text .. newline
    end
    return text
end

-- Split inserted content into logical lines for line-based operations.
-- 将插入内容拆分为用于按行操作的逻辑行。
--
-- Parameters:
--     content: Inserted or replacement text content.
-- 参数：
--     content：插入或替换的文本内容。
--
-- Returns:
--     table: Array-style logical line table.
-- 返回值：
--     table：数组形式的逻辑行表。
local function split_insert_content(content)
    local lines, had_final_newline = split_lines_with_final_newline(content)
    if #lines == 0 and had_final_newline then
        return { "" }
    end
    return lines
end

-- Build one same-directory sidecar path for temporary writes and rollback backups.
-- 构造一个同目录旁路路径，用于临时写入与回滚备份。
--
-- Parameters:
--     file_path: Target file path.
--     label: Short sidecar label such as tmp or bak.
-- 参数：
--     file_path：目标文件路径。
--     label：旁路短标签，例如 tmp 或 bak。
--
-- Returns:
--     string: Generated sidecar file path.
-- 返回值：
--     string：生成的旁路文件路径。
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

-- Detect the current host OS name with a runtime-backed fallback.
-- 使用运行时支持的后备逻辑检测当前宿主操作系统名称。
--
-- Parameters:
--     None.
-- 参数：
--     无。
--
-- Returns:
--     string: Lowercase host OS name such as windows, linux, or macos.
-- 返回值：
--     string：小写宿主操作系统名称，例如 windows、linux 或 macos。
local function detect_host_os()
    if vulcan and vulcan.os and type(vulcan.os.info) == "function" then
        local ok, info = pcall(vulcan.os.info)
        if ok and type(info) == "table" and type(info.os) == "string" and trim(info.os) ~= "" then
            return trim(info.os):lower()
        end
    end
    return package.config:sub(1, 1) == "\\" and "windows" or "unix"
end

-- Execute one host process command for compatibility fallback filesystem operations.
-- 执行一个宿主进程命令，用于兼容性回退文件系统操作。
--
-- Parameters:
--     program: Host program name.
--     args: Array-style argument list.
-- 参数：
--     program：宿主程序名。
--     args：数组形式的参数列表。
--
-- Returns:
--     boolean: True on success.
--     string|nil: Failure message when the process call fails.
-- 返回值：
--     boolean：成功时返回 true。
--     string|nil：进程调用失败时返回失败信息。
local function execute_host_process(program, args)
    if not (vulcan and vulcan.process and type(vulcan.process.exec) == "function") then
        return false, "vulcan.process.exec is unavailable"
    end
    local ok, result = pcall(vulcan.process.exec, {
        program = tostring(program or ""),
        args = args or {},
        timeout_ms = 30000,
    })
    if not ok then
        return false, tostring(result)
    end
    if type(result) ~= "table" then
        return false, "process execution did not return a result table"
    end
    if result.ok == true and result.success == true and (result.code == nil or tonumber(result.code) == 0) then
        return true, nil
    end
    local stderr_text = trim(result.stderr or "")
    local error_text = trim(result.error or "")
    local stdout_text = trim(result.stdout or "")
    local message = stderr_text
    if message == "" then
        message = error_text
    end
    if message == "" then
        message = stdout_text
    end
    if message == "" then
        message = "process execution failed"
    end
    if result.timed_out == true then
        message = message .. " (timed out)"
    end
    return false, message
end

-- Remove one sidecar file and ignore cleanup failures.
-- 删除一个旁路文件并忽略清理失败。
--
-- Parameters:
--     file_path: Candidate sidecar file path.
-- 参数：
--     file_path：候选旁路文件路径。
--
-- Returns:
--     None.
-- 返回值：
--     无。
--
-- Important:
--     Prefer `vulcan.fs.remove` when available because the host runtime provides
--     more stable cross-platform filesystem behavior than Lua `os.remove`.
-- 重要说明：
--     如果可用，应优先使用 `vulcan.fs.remove`，因为宿主运行时提供的
--     跨平台文件系统行为比 Lua `os.remove` 更稳定。
local function safe_remove_file(file_path)
    if type(file_path) ~= "string" or file_path == "" then
        return
    end
    if vulcan.fs.exists(file_path) then
        if vulcan and vulcan.fs and type(vulcan.fs.remove) == "function" then
            pcall(vulcan.fs.remove, file_path)
            return
        end
        local host_os = detect_host_os()
        if host_os == "windows" then
            pcall(execute_host_process, "powershell", {
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "& { param($path) if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force } }",
                file_path,
            })
            return
        end
        pcall(os.remove, file_path)
    end
end

-- Rename one file and normalize platform-specific failure messages.
-- 重命名一个文件，并规范化不同平台上的失败信息。
--
-- Parameters:
--     source_path: Existing source path.
--     target_path: Target path after the rename.
-- 参数：
--     source_path：现有源路径。
--     target_path：重命名后的目标路径。
--
-- Returns:
--     boolean: True on success.
--     string|nil: Failure message when the rename fails.
-- 返回值：
--     boolean：成功时返回 true。
--     string|nil：重命名失败时返回失败信息。
--
-- Important:
--     Prefer `vulcan.fs.rename` when available so host-managed Unicode and
--     cross-platform path behavior stays consistent with the runtime contract.
-- 重要说明：
--     如果可用，应优先使用 `vulcan.fs.rename`，从而让宿主管理的 Unicode
--     与跨平台路径行为保持与运行时契约一致。
local function rename_file(source_path, target_path)
    if vulcan and vulcan.fs and type(vulcan.fs.rename) == "function" then
        local ok, renamed_or_error = pcall(vulcan.fs.rename, source_path, target_path)
        if not ok then
            return false, tostring(renamed_or_error)
        end
        if renamed_or_error == true then
            return true, nil
        end
        return false, "rename failed"
    end
    local host_os = detect_host_os()
    if host_os == "windows" then
        return execute_host_process("powershell", {
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "& { param($src, $dst) Move-Item -LiteralPath $src -Destination $dst -Force }",
            source_path,
            target_path,
        })
    end
    local ok, renamed, message = pcall(os.rename, source_path, target_path)
    if not ok then
        return false, tostring(renamed)
    end
    if not renamed then
        return false, tostring(message or "rename failed")
    end
    return true, nil
end

-- Write file content through one temp-file swap with best-effort rollback.
-- 通过一次临时文件替换写入文件内容，并尽最大努力执行回滚。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     file_path: Final target file path.
--     content: Final text content to persist.
--     original_content: Original file content used for rollback.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     file_path：最终目标文件路径。
--     content：需要落盘的最终文本内容。
--     original_content：用于回滚的原始文件内容。
--
-- Returns:
--     string|nil: Markdown error text on failure, otherwise nil.
-- 返回值：
--     string|nil：失败时返回 Markdown 错误文本，否则返回 nil。
local function write_file(error_title, parameter_error_codes, file_path, content, original_content)
    local temp_path = build_sidecar_file_path(file_path, "tmp")
    local backup_path = build_sidecar_file_path(file_path, "bak")
    local existed_before = vulcan.fs.exists(file_path)

    local temp_ok, temp_error = pcall(vulcan.fs.write, temp_path, content)
    if not temp_ok then
        safe_remove_file(temp_path)
        return render_error(error_title, parameter_error_codes, "temp_write_failed", tostring(temp_error), {
            file = file_path,
            temp_file = temp_path,
        })
    end

    if existed_before then
        local backed_up, backup_error = rename_file(file_path, backup_path)
        if not backed_up then
            safe_remove_file(temp_path)
            return render_error(error_title, parameter_error_codes, "backup_creation_failed", tostring(backup_error), {
                file = file_path,
                backup_file = backup_path,
            })
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
        return render_error(error_title, parameter_error_codes, "temp_swap_failed", tostring(swap_error), {
            file = file_path,
            temp_file = temp_path,
            backup_file = backup_path,
        })
    end

    if existed_before then
        safe_remove_file(backup_path)
    end
    return nil
end

-- Delete one file path through the host filesystem layer and normalize failure messages.
-- 通过宿主文件系统层删除一个文件路径，并规范化失败信息。
--
-- Parameters:
--     error_title: Visible Markdown title for errors.
--     parameter_error_codes: Set-like table of parameter error codes.
--     file_path: Final target file path.
-- 参数：
--     error_title：错误使用的可见 Markdown 标题。
--     parameter_error_codes：参数错误码集合表。
--     file_path：最终目标文件路径。
--
-- Returns:
--     string|nil: Markdown error text on failure, otherwise nil.
-- 返回值：
--     string|nil：失败时返回 Markdown 错误文本，否则返回 nil。
local function delete_file(error_title, parameter_error_codes, file_path)
    if vulcan and vulcan.fs and type(vulcan.fs.remove) == "function" then
        local ok, removed_or_error = pcall(vulcan.fs.remove, file_path)
        if not ok then
            return render_error(error_title, parameter_error_codes, "file_delete_failed", tostring(removed_or_error), {
                file = file_path,
            })
        end
        if removed_or_error == true then
            return nil
        end
        return render_error(error_title, parameter_error_codes, "file_delete_failed", "delete failed", {
            file = file_path,
        })
    end
    local host_os = detect_host_os()
    if host_os == "windows" then
        local removed, remove_error = execute_host_process("powershell", {
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "& { param($path) Remove-Item -LiteralPath $path -Force }",
            file_path,
        })
        if not removed then
            return render_error(error_title, parameter_error_codes, "file_delete_failed", tostring(remove_error or "delete failed"), {
                file = file_path,
            })
        end
        return nil
    end
    local ok, removed, message = pcall(os.remove, file_path)
    if not ok then
        return render_error(error_title, parameter_error_codes, "file_delete_failed", tostring(removed), {
            file = file_path,
        })
    end
    if not removed then
        return render_error(error_title, parameter_error_codes, "file_delete_failed", tostring(message or "delete failed"), {
            file = file_path,
        })
    end
    return nil
end

-- Build one joined line slice from a logical line table using canonical `\\n` separators.
-- 使用 canonical `\\n` 分隔符从逻辑行表中构造一个拼接后的行片段。
--
-- Parameters:
--     lines: Source logical line table.
--     start_line: First 1-based line to include.
--     end_line: Last 1-based line to include.
-- 参数：
--     lines：源逻辑行表。
--     start_line：需要包含的首个 1-based 行号。
--     end_line：需要包含的最后一个 1-based 行号。
--
-- Returns:
--     string: Joined line slice text.
-- 返回值：
--     string：拼接后的行片段文本。
local function build_delete_content_slice(lines, start_line, end_line)
    local buffer = {}
    local safe_start = math.max(1, tonumber(start_line) or 1)
    local safe_end = math.min(#(lines or {}), tonumber(end_line) or 0)
    if safe_start > safe_end then
        return ""
    end
    for line_number = safe_start, safe_end do
        table.insert(buffer, tostring(lines[line_number] or ""))
    end
    return table.concat(buffer, "\n")
end

-- Build canonical delete-record content fields that match the official full or truncated modes.
-- 构造与官方 full 或 truncated 模式一致的 canonical 删除记录内容字段。
--
-- Parameters:
--     content: Deleted file content or one stable placeholder string.
--     total_line_count: Total logical line count represented by the delete record.
-- 参数：
--     content：被删除文件内容或一个稳定占位字符串。
--     total_line_count：删除记录表示的总逻辑行数。
--
-- Returns:
--     table: Canonical delete content field table.
-- 返回值：
--     table：canonical 删除内容字段表。
local function build_delete_content_fields(content, total_line_count)
    local normalized_content = tostring(content or "")
    local lines = select(1, split_lines_with_final_newline(normalized_content))
    local line_count = tonumber(total_line_count)
    if line_count == nil or line_count < 0 then
        line_count = #lines
    end
    if line_count > DELETE_TRUNCATE_LINE_LIMIT then
        local head_count = math.min(line_count, DELETE_TRUNCATED_EDGE_LINE_COUNT)
        local tail_count = math.min(DELETE_TRUNCATED_EDGE_LINE_COUNT, math.max(0, line_count - head_count))
        return {
            content_mode = "truncated",
            total_line_count = line_count,
            content_head = build_delete_content_slice(lines, 1, head_count),
            content_tail = tail_count > 0 and build_delete_content_slice(lines, line_count - tail_count + 1, line_count) or "",
        }
    end
    return {
        content_mode = "full",
        total_line_count = line_count,
        content = normalized_content,
    }
end

-- Read one binary sample from disk for lightweight file-type inspection.
-- 从磁盘读取一段二进制样本，用于轻量文件类型判断。
--
-- Parameters:
--     file_path: Absolute file path to inspect.
--     byte_limit: Maximum number of bytes to read.
-- 参数：
--     file_path：需要检查的绝对文件路径。
--     byte_limit：最多读取的字节数。
--
-- Returns:
--     string|nil: Binary sample text on success.
--     string|nil: Open or read error text on failure.
-- 返回值：
--     string|nil：成功时返回二进制样本文本。
--     string|nil：失败时返回打开或读取错误文本。
local function read_binary_sample(file_path, byte_limit)
    local handle, open_error = io.open(file_path, "rb")
    if not handle then
        return nil, tostring(open_error or "failed to open file")
    end
    local ok, data_or_error = pcall(function()
        return handle:read(tonumber(byte_limit) or 4096)
    end)
    handle:close()
    if not ok then
        return nil, tostring(data_or_error)
    end
    return tostring(data_or_error or ""), nil
end

-- Decide whether one sampled byte sequence should be treated as binary rather than line-oriented text.
-- 判断一段采样字节序列是否应被视为二进制，而不是按行处理的文本。
--
-- Parameters:
--     sample: Raw byte sample text.
-- 参数：
--     sample：原始字节样本文本。
--
-- Returns:
--     boolean: True when the sample looks binary.
-- 返回值：
--     boolean：样本看起来像二进制时返回 true。
local function sample_looks_binary(sample)
    local text = tostring(sample or "")
    if text == "" then
        return false
    end
    if text:find("\0", 1, true) ~= nil then
        return true
    end
    local suspicious = 0
    for index = 1, #text do
        local byte = text:byte(index)
        local looks_textual = byte == 9 or byte == 10 or byte == 13 or (byte >= 32 and byte <= 126) or byte >= 128
        if not looks_textual then
            suspicious = suspicious + 1
        end
    end
    return suspicious / #text > 0.30
end

-- Inspect one file before deletion and summarize whether it should be treated as text or binary.
-- 在删除前检查一个文件，并汇总它应按文本还是二进制处理。
--
-- Parameters:
--     file_path: Absolute file path to inspect.
-- 参数：
--     file_path：需要检查的绝对文件路径。
--
-- Returns:
--     table|nil: Inspection summary with content_type, content, removed_lines, and preview_content.
--     string|nil: Error text on failure.
-- 返回值：
--     table|nil：包含 content_type、content、removed_lines 与 preview_content 的检查摘要。
--     string|nil：失败时返回错误文本。
local function inspect_file_for_delete(file_path)
    if detect_host_os() == "windows" then
        local ok, content_or_error = pcall(vulcan.fs.read, file_path)
        if not ok or type(content_or_error) ~= "string" then
            return {
                content_type = "binary",
                content = BINARY_FILE_PLACEHOLDER,
                removed_lines = 1,
                preview_content = BINARY_FILE_PLACEHOLDER,
            }, nil
        end
        local content = tostring(content_or_error or "")
        local lines = select(1, split_lines_with_final_newline(content))
        return {
            content_type = "text",
            content = content,
            removed_lines = #lines,
            preview_content = content,
        }, nil
    end
    local sample, sample_error = read_binary_sample(file_path, 4096)
    if sample_error then
        return nil, sample_error
    end
    if sample_looks_binary(sample) then
        return {
            content_type = "binary",
            content = BINARY_FILE_PLACEHOLDER,
            removed_lines = 1,
            preview_content = BINARY_FILE_PLACEHOLDER,
        }, nil
    end
    local ok, content_or_error = pcall(vulcan.fs.read, file_path)
    if not ok or type(content_or_error) ~= "string" then
        return {
            content_type = "binary",
            content = BINARY_FILE_PLACEHOLDER,
            removed_lines = 1,
            preview_content = BINARY_FILE_PLACEHOLDER,
        }, nil
    end
    local content = tostring(content_or_error or "")
    local lines = select(1, split_lines_with_final_newline(content))
    return {
        content_type = "text",
        content = content,
        removed_lines = #lines,
        preview_content = content,
    }, nil
end

-- Append one preview line while respecting the preview line budget.
-- 在遵守预览行数预算的前提下追加一行预览内容。
--
-- Parameters:
--     output: Output line table to mutate.
--     rendered_count: Number of lines already rendered.
--     line: Preview line text to append.
--     max_preview_lines: Maximum preview line budget.
-- 参数：
--     output：需要原地追加的输出行表。
--     rendered_count：已经渲染的行数。
--     line：需要追加的预览行文本。
--     max_preview_lines：允许的最大预览行预算。
--
-- Returns:
--     number: Updated rendered line count.
--     boolean: True when more lines may still be appended.
-- 返回值：
--     number：更新后的已渲染行数。
--     boolean：仍允许继续追加时返回 true。
local function append_preview_line(output, rendered_count, line, max_preview_lines)
    if rendered_count >= max_preview_lines then
        return rendered_count, false
    end
    table.insert(output, line)
    return rendered_count + 1, true
end

-- Render one inclusive context range from a line table into one preview block.
-- 将一个行表中的闭区间上下文渲染到预览区块中。
--
-- Parameters:
--     output: Output line table to mutate.
--     rendered_count: Number of lines already rendered.
--     lines: Source logical line table.
--     start_line: First 1-based line number to render.
--     end_line: Last 1-based line number to render.
--     prefix: Prefix such as space, plus, or minus.
--     max_preview_lines: Maximum preview line budget.
--     display_start_line: Optional visible first 1-based line number.
-- 参数：
--     output：需要原地追加的输出行表。
--     rendered_count：已经渲染的行数。
--     lines：源逻辑行表。
--     start_line：需要渲染的首个 1-based 行号。
--     end_line：需要渲染的最后一个 1-based 行号。
--     prefix：前缀，例如空格、加号或减号。
--     max_preview_lines：允许的最大预览行预算。
--     display_start_line：可选的可见首个 1-based 行号。
--
-- Returns:
--     number: Updated rendered line count.
--     boolean: True when rendering can continue.
-- 返回值：
--     number：更新后的已渲染行数。
--     boolean：仍可继续渲染时返回 true。
local function render_context_range(output, rendered_count, lines, start_line, end_line, prefix, max_preview_lines, display_start_line)
    local safe_start = math.max(1, start_line)
    local safe_end = math.min(#lines, end_line)
    local source_base = tonumber(start_line) or safe_start
    local display_base = tonumber(display_start_line) or source_base
    for line_number = safe_start, safe_end do
        local display_line_number = display_base + (line_number - source_base)
        local keep_going
        rendered_count, keep_going = append_preview_line(output, rendered_count, string.format("%sL%d: %s", prefix, display_line_number, lines[line_number] or ""), max_preview_lines)
        if not keep_going then
            return rendered_count, false
        end
    end
    return rendered_count, true
end

-- Render one operation-oriented preview so inserts and deletions remain visually clear.
-- 渲染一个面向操作的预览，确保插入与删除在视觉上保持清晰。
--
-- Parameters:
--     request_mode: Edit mode such as overwrite or append.
--     request_content: Original requested content text used for empty-preview messaging.
--     original_content: Original file content.
--     edited_content: Edited file content.
--     changed_span: Span metadata describing old and new line ranges.
--     preview_title: Visible preview section title.
--     max_preview_lines: Maximum preview line budget.
-- 参数：
--     request_mode：编辑模式，例如 overwrite 或 append。
--     request_content：用于空预览提示的原始请求内容。
--     original_content：原始文件内容。
--     edited_content：编辑后的文件内容。
--     changed_span：描述旧行范围与新行范围的区间元数据。
--     preview_title：可见预览区标题。
--     max_preview_lines：允许的最大预览行预算。
--
-- Returns:
--     string: Markdown preview block.
-- 返回值：
--     string：Markdown 预览区块文本。
local function render_operation_preview(request_mode, request_content, original_content, edited_content, changed_span, preview_title, max_preview_lines)
    local original_lines = select(1, split_lines_with_final_newline(original_content))
    local edited_lines = select(1, split_lines_with_final_newline(edited_content))
    local original_start = changed_span.original_start_line or changed_span.start_line
    local original_end = changed_span.original_end_line or changed_span.end_line
    local edited_start = changed_span.start_line or 1
    local edited_end = changed_span.end_line or edited_start
    local inserted_line_count = tonumber(changed_span.inserted_line_count) or 0
    local before_context_start = original_start - 3
    local before_context_end = original_start - 1
    local after_context_start = original_end + 1
    local after_context_end = original_end + 3
    local after_context_display_start = edited_start + inserted_line_count
    local output = {
        "## " .. tostring(preview_title or "Preview"),
        "",
        "```diff",
    }
    if request_mode == "insert_after" then
        before_context_start = original_start - 2
        before_context_end = original_start
    elseif request_mode == "insert_before" then
        after_context_start = original_start
        after_context_end = original_start + 2
    end

    local preview_limit = tonumber(max_preview_lines) or DEFAULT_MAX_PREVIEW_LINES
    local rendered = 0
    local keep_going
    rendered, keep_going = render_context_range(output, rendered, original_lines, before_context_start, before_context_end, " ", preview_limit)

    if keep_going and (request_mode == "replace_range" or request_mode == "overwrite") then
        rendered, keep_going = render_context_range(output, rendered, original_lines, original_start, original_end, "-", preview_limit)
    end

    if keep_going and inserted_line_count > 0 then
        rendered, keep_going = render_context_range(output, rendered, edited_lines, edited_start, edited_end, "+", preview_limit)
    end

    if keep_going and request_mode ~= "append" then
        rendered, keep_going = render_context_range(output, rendered, original_lines, after_context_start, after_context_end, " ", preview_limit, after_context_display_start)
    end

    if not keep_going then
        table.insert(output, "... preview truncated ...")
    end
    if rendered == 0 then
        if tostring(request_content or "") == "" then
            table.insert(output, "(no inserted lines)")
        else
            table.insert(output, "(no visible line changes)")
        end
    end
    table.insert(output, "```")
    return table.concat(output, "\n")
end

-- Build one contiguous context string from a logical line table around one range boundary.
-- 从逻辑行表中围绕某个区间边界构造一个连续上下文字符串。
--
-- Parameters:
--     lines: Source logical line table.
--     start_line: First 1-based line to include.
--     end_line: Last 1-based line to include.
--     newline: Newline sequence used to rejoin lines.
-- 参数：
--     lines：源逻辑行表。
--     start_line：需要包含的首个 1-based 行号。
--     end_line：需要包含的最后一个 1-based 行号。
--     newline：重新拼接时使用的换行序列。
--
-- Returns:
--     string: Joined contiguous context string, or an empty string when no lines are selected.
-- 返回值：
--     string：拼接后的连续上下文字符串；未选中任何行时返回空字符串。
local function build_context_string(lines, start_line, end_line, newline)
    if not lines or #lines == 0 then
        return ""
    end
    local safe_start = math.max(1, start_line or 1)
    local safe_end = math.min(#lines, end_line or 0)
    if safe_start > safe_end then
        return ""
    end
    local buffer = {}
    for line_number = safe_start, safe_end do
        table.insert(buffer, lines[line_number] or "")
    end
    return table.concat(buffer, newline or "\n")
end

-- Build ordered `{ line, content }` entries from one logical line table range.
-- 从一个逻辑行表区间构造有序的 `{ line, content }` 记录。
--
-- Parameters:
--     lines: Source logical line table.
--     start_line: First 1-based line to include.
--     end_line: Last 1-based line to include.
-- 参数：
--     lines：源逻辑行表。
--     start_line：需要包含的首个 1-based 行号。
--     end_line：需要包含的最后一个 1-based 行号。
--
-- Returns:
--     table: Array-style ordered line entry list.
-- 返回值：
--     table：数组形式的有序行记录列表。
local function build_line_entries(lines, start_line, end_line)
    local output = {}
    if not lines or #lines == 0 then
        return output
    end
    local safe_start = math.max(1, start_line or 1)
    local safe_end = math.min(#lines, end_line or 0)
    if safe_start > safe_end then
        return output
    end
    for line_number = safe_start, safe_end do
        table.insert(output, {
            line = line_number,
            content = tostring(lines[line_number] or ""),
        })
    end
    return output
end

-- Build one canonical modify file record for a host `change_set` result.
-- 为宿主 `change_set` 结果构造一个 canonical 的 modify 文件记录。
--
-- Parameters:
--     file_path: Absolute target file path.
--     original_content: Original file content.
--     edited_content: Edited file content.
--     changed_span: Span metadata describing old and new line ranges.
-- 参数：
--     file_path：绝对目标文件路径。
--     original_content：原始文件内容。
--     edited_content：编辑后的文件内容。
--     changed_span：描述旧行范围与新行范围的区间元数据。
--
-- Returns:
--     table: Canonical `change_set.files[]` modify record.
-- 返回值：
--     table：canonical `change_set.files[]` modify 记录。
local function build_modify_file_record(file_path, original_content, edited_content, changed_span)
    local newline = detect_newline_sequence(original_content ~= "" and original_content or edited_content)
    local original_lines = select(1, split_lines_with_final_newline(original_content))
    local edited_lines = select(1, split_lines_with_final_newline(edited_content))
    local original_start = changed_span.original_start_line or changed_span.start_line
    local original_end = changed_span.original_end_line or changed_span.end_line
    local edited_start = changed_span.start_line or 1
    local edited_end = changed_span.end_line or (changed_span.start_line or 1)
    local inserted_line_count = tonumber(changed_span.inserted_line_count) or 0
    local after_context_start = edited_start + inserted_line_count
    local before_context = build_context_string(original_lines, original_start - 3, original_start - 1, newline)
    local after_context = build_context_string(edited_lines, after_context_start, after_context_start + 2, newline)
    local deleted_lines = build_line_entries(original_lines, original_start, original_end)
    local inserted_lines = {}
    if inserted_line_count > 0 then
        inserted_lines = build_line_entries(edited_lines, edited_start, edited_end)
    end
    if #deleted_lines == 0 and #inserted_lines == 0 then
        return nil
    end
    return {
        change = "modify",
        path = tostring(file_path or ""),
        hunks = {
            {
                before = before_context,
                delete = deleted_lines,
                insert = inserted_lines,
                after = after_context,
            },
        },
    }
end

-- Build one canonical create file record for a host `change_set` result.
-- 为宿主 `change_set` 结果构造一个 canonical 的 create 文件记录。
--
-- Parameters:
--     file_path: Absolute target file path.
--     content: Final created file content.
-- 参数：
--     file_path：绝对目标文件路径。
--     content：最终创建出的文件内容。
--
-- Returns:
--     table: Canonical `change_set.files[]` create record.
-- 返回值：
--     table：canonical `change_set.files[]` create 记录。
local function build_create_file_record(file_path, content)
    return {
        change = "create",
        path = tostring(file_path or ""),
        content = tostring(content or ""),
    }
end

-- Build one canonical delete file record for a host `change_set` result.
-- 为宿主 `change_set` 结果构造一个 canonical 的 delete 文件记录。
--
-- Parameters:
--     file_path: Absolute target file path.
--     content: Deleted file content or one stable binary placeholder.
--     total_line_count: Total logical line count represented by the delete content.
-- 参数：
--     file_path：绝对目标文件路径。
--     content：被删除文件内容，或稳定的二进制占位文本。
--     total_line_count：删除内容表示的总逻辑行数。
--
-- Returns:
--     table: Canonical `change_set.files[]` delete record.
-- 返回值：
--     table：canonical `change_set.files[]` delete 记录。
local function build_delete_file_record(file_path, content, total_line_count)
    local record = {
        change = "delete",
        path = tostring(file_path or ""),
    }
    for key, value in pairs(build_delete_content_fields(content, total_line_count)) do
        record[key] = value
    end
    return record
end

-- Resolve one normalized snapshot of the current host structured-result capability.
-- 解析当前宿主结构化结果能力的标准化快照。
--
-- Parameters:
--     None.
-- 参数：
--     无。
--
-- Returns:
--     table: Capability snapshot with enabled, allows_change_set, and max_payload_bytes fields.
-- 返回值：
--     table：包含 enabled、allows_change_set 与 max_payload_bytes 字段的能力快照。
local function resolve_host_result_capability()
    local host_result = nil
    if vulcan and vulcan.context and type(vulcan.context.host_result) == "table" then
        host_result = vulcan.context.host_result
    end
    local enabled = type(host_result) == "table" and host_result.enabled == true
    local allowed_kinds = type(host_result) == "table" and host_result.allowed_kinds or nil
    local allow_change_set = true
    if type(allowed_kinds) == "table" then
        local saw_any = false
        allow_change_set = false
        for _, item in ipairs(allowed_kinds) do
            if type(item) == "string" and trim(item) ~= "" then
                saw_any = true
                if trim(item) == "change_set" then
                    allow_change_set = true
                end
            end
        end
        if not saw_any then
            allow_change_set = true
        end
    end
    local max_payload_bytes = nil
    if type(host_result) == "table" then
        local numeric_limit = tonumber(host_result.max_payload_bytes)
        if numeric_limit and numeric_limit > 0 then
            max_payload_bytes = math.floor(numeric_limit)
        end
    end
    return {
        enabled = enabled,
        allows_change_set = enabled and allow_change_set,
        max_payload_bytes = max_payload_bytes,
    }
end

-- Finalize one optional host `change_set` result while respecting host capability and payload size limits.
-- 在遵守宿主能力与载荷大小限制的前提下完成一个可选的宿主 `change_set` 结果。
--
-- Parameters:
--     capability: Capability snapshot from resolve_host_result_capability.
--     payload: Candidate `change_set` payload object.
-- 参数：
--     capability：来自 resolve_host_result_capability 的能力快照。
--     payload：候选 `change_set` 载荷对象。
--
-- Returns:
--     table|nil: Host result wrapper `{ kind = "change_set", payload = ... }` when accepted.
-- 返回值：
--     table|nil：被接受时返回 `{ kind = "change_set", payload = ... }` 宿主结果包装对象。
local function finalize_change_set_host_result(capability, payload)
    if type(capability) ~= "table" or capability.enabled ~= true or capability.allows_change_set ~= true then
        return nil
    end
    if type(payload) ~= "table" then
        return nil
    end
    if capability.max_payload_bytes ~= nil and vulcan and vulcan.json and type(vulcan.json.encode) == "function" then
        local ok, encoded = pcall(vulcan.json.encode, payload)
        if not ok or type(encoded) ~= "string" then
            return nil
        end
        if #encoded > capability.max_payload_bytes then
            return nil
        end
    end
    return {
        kind = "change_set",
        payload = payload,
    }
end

-- Build one aggregated host `change_set` result from multiple file records.
-- 根据多个文件记录构造一个聚合宿主 `change_set` 结果。
--
-- Parameters:
--     capability: Capability snapshot from resolve_host_result_capability.
--     apply: Whether the operation has been written to disk.
--     summary: Human-readable summary string.
--     file_records: Array-style canonical file record list.
-- 参数：
--     capability：来自 resolve_host_result_capability 的能力快照。
--     apply：操作是否已经落盘。
--     summary：人类可读摘要字符串。
--     file_records：数组形式的 canonical 文件记录列表。
--
-- Returns:
--     table|nil: Host result wrapper when accepted, otherwise nil.
-- 返回值：
--     table|nil：被接受时返回宿主结果包装对象，否则返回 nil。
local function build_change_set_host_result(capability, apply, summary, file_records)
    local files = {}
    for _, record in ipairs(file_records or {}) do
        if type(record) == "table" then
            table.insert(files, record)
        end
    end
    if #files == 0 then
        return nil
    end
    local payload = {
        mode = apply and "applied" or "preview",
        summary = tostring(summary or ""),
        files = files,
    }
    return finalize_change_set_host_result(capability, payload)
end

-- Build one host `change_set` wrapper for a single-file create operation.
-- 为单文件创建操作构造一个宿主 `change_set` 包装结果。
--
-- Parameters:
--     capability: Capability snapshot from resolve_host_result_capability.
--     file_path: Absolute target file path.
--     content: Final file content.
--     apply: Whether the operation has been written to disk.
-- 参数：
--     capability：来自 resolve_host_result_capability 的能力快照。
--     file_path：绝对目标文件路径。
--     content：最终文件内容。
--     apply：操作是否已经落盘。
--
-- Returns:
--     table|nil: Host result wrapper when accepted, otherwise nil.
-- 返回值：
--     table|nil：被接受时返回宿主结果包装对象，否则返回 nil。
local function build_create_host_result(capability, file_path, content, apply)
    return build_change_set_host_result(capability, apply, (apply and "Applied" or "Previewed") .. " creation of 1 file.", {
        build_create_file_record(file_path, content),
    })
end

-- Build one host `change_set` wrapper for a single-file edit operation.
-- 为单文件编辑操作构造一个宿主 `change_set` 包装结果。
--
-- Parameters:
--     capability: Capability snapshot from resolve_host_result_capability.
--     file_path: Absolute target file path.
--     original_content: Original file content.
--     edited_content: Edited file content.
--     changed_span: Span metadata describing old and new line ranges.
--     apply: Whether the operation has been written to disk.
--     change_type: Either create or modify.
-- 参数：
--     capability：来自 resolve_host_result_capability 的能力快照。
--     file_path：绝对目标文件路径。
--     original_content：原始文件内容。
--     edited_content：编辑后的文件内容。
--     changed_span：描述旧行范围与新行范围的区间元数据。
--     apply：操作是否已经落盘。
--     change_type：create 或 modify。
--
-- Returns:
--     table|nil: Host result wrapper when accepted, otherwise nil.
-- 返回值：
--     table|nil：被接受时返回宿主结果包装对象，否则返回 nil。
local function build_edit_host_result(capability, file_path, original_content, edited_content, changed_span, apply, change_type)
    local file_record = nil
    if change_type == "create" then
        file_record = build_create_file_record(file_path, edited_content)
    else
        file_record = build_modify_file_record(file_path, original_content, edited_content, changed_span)
    end
    if type(file_record) ~= "table" then
        return nil
    end
    return build_change_set_host_result(capability, apply, (apply and "Applied" or "Previewed") .. " 1 file edit.", {
        file_record,
    })
end

return {
    DEFAULT_MAX_PREVIEW_LINES = DEFAULT_MAX_PREVIEW_LINES,
    MAX_BATCH_FILES = MAX_BATCH_FILES,
    BINARY_FILE_PLACEHOLDER = BINARY_FILE_PLACEHOLDER,
    DELETE_TRUNCATE_LINE_LIMIT = DELETE_TRUNCATE_LINE_LIMIT,
    DELETE_TRUNCATED_EDGE_LINE_COUNT = DELETE_TRUNCATED_EDGE_LINE_COUNT,
    trim = trim,
    has_trimmed_text = has_trimmed_text,
    starts_with = starts_with,
    ends_with = ends_with,
    join_path = join_path,
    translate_windows_device_path = translate_windows_device_path,
    normalize_path_text = normalize_path_text,
    trim_trailing_path_separators = trim_trailing_path_separators,
    is_absolute_path = is_absolute_path,
    resolve_absolute_path = resolve_absolute_path,
    extract_parent_dir = extract_parent_dir,
    render_error = render_error,
    expand_environment_text = expand_environment_text,
    resolve_pwd_root = resolve_pwd_root,
    resolve_input_path = resolve_input_path,
    expand_environment_path = expand_environment_path,
    validate_optional_boolean = validate_optional_boolean,
    parse_integer_argument = parse_integer_argument,
    validate_batch_files_array = validate_batch_files_array,
    detect_newline_sequence = detect_newline_sequence,
    normalize_newlines = normalize_newlines,
    split_lines_with_final_newline = split_lines_with_final_newline,
    join_lines = join_lines,
    split_insert_content = split_insert_content,
    build_sidecar_file_path = build_sidecar_file_path,
    safe_remove_file = safe_remove_file,
    rename_file = rename_file,
    write_file = write_file,
    delete_file = delete_file,
    inspect_file_for_delete = inspect_file_for_delete,
    render_operation_preview = render_operation_preview,
    build_modify_file_record = build_modify_file_record,
    build_create_file_record = build_create_file_record,
    build_delete_file_record = build_delete_file_record,
    resolve_host_result_capability = resolve_host_result_capability,
    finalize_change_set_host_result = finalize_change_set_host_result,
    build_change_set_host_result = build_change_set_host_result,
    build_create_host_result = build_create_host_result,
    build_edit_host_result = build_edit_host_result,
}
