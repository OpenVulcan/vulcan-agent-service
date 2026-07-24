--[[
vulcan-file-delete
Delete or preview one or more regular files with explicit directory rejection, no_apply preview control, and optional host change_set output.
以显式拒绝目录、no_apply 预览控制和可选宿主 change_set 输出删除或预览一个或多个普通文件。
]]

-- Visible Markdown title used for delete error payloads.
-- 删除错误结果使用的可见 Markdown 标题。
local ERROR_TITLE = "FILE DELETE ERROR"

-- Visible Markdown title used for delete success payloads.
-- 删除成功结果使用的可见 Markdown 标题。
local RESULT_TITLE = "FILE DELETE RESULT"

-- Visible section title used for preview blocks.
-- 预览区块使用的可见标题。
local PREVIEW_TITLE = "Preview"

-- Error codes that indicate the caller passed invalid delete arguments.
-- 表示调用方传入无效删除参数的错误码集合。
local PARAMETER_ERROR_CODES = {
    invalid_file = true,
    invalid_files_argument = true,
    too_many_files = true,
    conflicting_batch_arguments = true,
    invalid_pwd_argument = true,
    relative_path_requires_pwd = true,
    environment_variable_not_found = true,
    invalid_environment_variable_reference = true,
    invalid_apply_argument = true,
    invalid_no_apply_argument = true,
    file_not_found = true,
    directory_delete_unsupported = true,
}

-- Load the shared file helper module from the current entry directory.
-- 从当前入口目录加载共享文件辅助模块。
--
-- Parameters:
--     None.
-- 参数：
--     无。
--
-- Returns:
--     table: Shared helper table used by file mutation entries.
-- 返回值：
--     table：文件变更入口共用的辅助表。
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

-- Build one tool-local Markdown error payload through the shared helper.
-- 通过共享辅助构造一个工具本地 Markdown 错误结果。
--
-- Parameters:
--     helpers: Shared helper table.
--     error_code: Stable error identifier.
--     message: Human-readable error message.
--     details: Optional key-value details rendered as bullet lines.
-- 参数：
--     helpers：共享辅助表。
--     error_code：稳定错误标识。
--     message：可读错误信息。
--     details：以项目符号行渲染的可选键值详情。
--
-- Returns:
--     string: Markdown error payload.
-- 返回值：
--     string：Markdown 错误结果文本。
local function render_error(helpers, error_code, message, details)
    return helpers.render_error(ERROR_TITLE, PARAMETER_ERROR_CODES, error_code, message, details)
end

-- Validate the delete request and normalize the target path to absolute form.
-- 校验删除请求，并将目标路径规范化为绝对路径。
--
-- Parameters:
--     helpers: Shared helper table.
--     request: Raw single-file request table.
--     no_apply: Shared preview-only flag inherited from the root request.
--     pwd_root: Valid absolute `PWD` directory root shared by the whole call, or nil.
-- 参数：
--     helpers：共享辅助表。
--     request：原始单文件请求表。
--     no_apply：从根请求继承的统一仅预览标记。
--     pwd_root：整个调用共享的有效绝对 `PWD` 目录根路径，或 nil。
--
-- Returns:
--     table|nil: Normalized delete request on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：成功时返回规范化后的删除请求。
--     string|nil：失败时返回 Markdown 错误文本。
local function validate_single_request(helpers, request, no_apply, pwd_root)
    if type(request) ~= "table" then
        return nil, render_error(helpers, "invalid_files_argument", "each files item must be an object with file", {
            actual_type = type(request),
        })
    end
    if type(request.file) ~= "string" or helpers.trim(request.file) == "" then
        return nil, render_error(helpers, "invalid_file", "file must be a non-empty string")
    end

    local file_path, environment_error = helpers.expand_environment_path(ERROR_TITLE, PARAMETER_ERROR_CODES, helpers.trim(request.file), "file", pwd_root)
    if environment_error then
        return nil, environment_error
    end

    return {
        file = file_path,
        no_apply = no_apply == true,
    }, nil
end

-- Collect one normalized list of delete requests from single-file or batch input.
-- 从单文件或批量输入中收集一组规范化的删除请求。
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
    if request.apply ~= nil then
        return nil, nil, render_error(helpers, "invalid_apply_argument", "apply is no longer supported; use no_apply=true to preview without deleting")
    end

    -- Validate the preview-only flag before any path or file work happens.
    -- 在执行任何路径或文件处理前校验仅预览标记。
    local no_apply_error = helpers.validate_optional_boolean(ERROR_TITLE, PARAMETER_ERROR_CODES, request.no_apply, "no_apply")
    if no_apply_error then
        return nil, nil, no_apply_error
    end

    -- A true no_apply value requests preview-only behavior; omitted or false deletes by default.
    -- no_apply 为 true 时请求仅预览；省略或为 false 时默认删除。
    local no_apply = request.no_apply == true

    local pwd_root, pwd_error = helpers.resolve_pwd_root(ERROR_TITLE, PARAMETER_ERROR_CODES, request.PWD)
    if pwd_error then
        return nil, nil, pwd_error
    end

    if request.files ~= nil then
        if request.file ~= nil then
            return nil, true, render_error(helpers, "conflicting_batch_arguments", "use either file or files, not both", {
                preferred = "files",
            })
        end
        local files, files_error = helpers.validate_batch_files_array(ERROR_TITLE, PARAMETER_ERROR_CODES, request.files)
        if files_error then
            return nil, true, files_error
        end
        local normalized = {}
        for index, item in ipairs(files) do
            if item.apply ~= nil then
                return nil, true, render_error(helpers, "invalid_apply_argument", "apply is no longer supported; use root-level no_apply=true to preview without deleting", {
                    file_index = tostring(index),
                })
            end
            if item.no_apply ~= nil then
                return nil, true, render_error(helpers, "invalid_no_apply_argument", "no_apply is root-level only in batch mode", {
                    file_index = tostring(index),
                })
            end

            local item_request, item_error = validate_single_request(helpers, item, no_apply, pwd_root)
            if item_error then
                return nil, true, render_error(helpers, "invalid_files_argument", "one files item is invalid", {
                    file_index = tostring(index),
                }) .. "\n\n" .. item_error
            end
            table.insert(normalized, item_request)
        end
        return normalized, true, nil
    end

    local single_request, validation_error = validate_single_request(helpers, request, no_apply, pwd_root)
    if validation_error then
        return nil, false, validation_error
    end
    return { single_request }, false, nil
end

-- Validate that one target path exists and points to a regular file.
-- 校验目标路径存在且指向普通文件。
--
-- Parameters:
--     helpers: Shared helper table.
--     file_path: Absolute target file path.
-- 参数：
--     helpers：共享辅助表。
--     file_path：绝对目标文件路径。
--
-- Returns:
--     string|nil: Nil on success, otherwise Markdown error text.
-- 返回值：
--     string|nil：成功时返回 nil，否则返回 Markdown 错误文本。
local function validate_delete_target(helpers, file_path)
    if not vulcan.fs.exists(file_path) then
        return render_error(helpers, "file_not_found", "file does not exist", {
            file = file_path,
        })
    end
    if vulcan.fs.is_dir(file_path) then
        return render_error(helpers, "directory_delete_unsupported", "directory delete is not supported; delete only accepts regular files", {
            file = file_path,
        })
    end
    return nil
end

-- Build one stable changed-span descriptor for whole-file deletion previews.
-- 为整文件删除预览构造一个稳定的变更区间描述。
--
-- Parameters:
--     removed_lines: Number of logical lines represented in the deletion preview.
-- 参数：
--     removed_lines：删除预览中表示的逻辑行数。
--
-- Returns:
--     table: Span metadata compatible with the shared preview renderer.
-- 返回值：
--     table：与共享预览渲染器兼容的区间元数据。
local function build_delete_span(removed_lines)
    local line_count = math.max(0, tonumber(removed_lines) or 0)
    return {
        start_line = 1,
        end_line = math.max(1, line_count),
        original_start_line = 1,
        original_end_line = line_count,
        inserted_line_count = 0,
    }
end

-- Render one delete preview block, including the empty-file edge case.
-- 渲染一个删除预览区块，并覆盖空文件边界情况。
--
-- Parameters:
--     helpers: Shared helper table.
--     inspection: Delete inspection summary from shared helpers.
--     changed_span: Preview span metadata.
-- 参数：
--     helpers：共享辅助表。
--     inspection：来自共享辅助的删除检查摘要。
--     changed_span：预览区间元数据。
--
-- Returns:
--     string: Markdown preview block.
-- 返回值：
--     string：Markdown 预览区块文本。
local function render_delete_preview(helpers, inspection, changed_span)
    if (inspection.removed_lines or 0) < 1 then
        return table.concat({
            "## " .. PREVIEW_TITLE,
            "",
            "```diff",
            "-(empty file removed)",
            "```",
        }, "\n")
    end
    return helpers.render_operation_preview("overwrite", "delete", inspection.preview_content, "", changed_span, PREVIEW_TITLE, helpers.DEFAULT_MAX_PREVIEW_LINES)
end

-- Prepare one validated delete operation before preview rendering or disk writes.
-- 在预览渲染或落盘删除前准备一次已校验的删除操作。
--
-- Parameters:
--     helpers: Shared helper table.
--     request: Normalized delete request.
-- 参数：
--     helpers：共享辅助表。
--     request：规范化后的删除请求。
--
-- Returns:
--     table|nil: Prepared operation summary on success.
--     string|nil: Markdown error text on failure.
-- 返回值：
--     table|nil：成功时返回已准备的操作摘要。
--     string|nil：失败时返回 Markdown 错误文本。
local function prepare_operation(helpers, request)
    local target_error = validate_delete_target(helpers, request.file)
    if target_error then
        return nil, target_error
    end

    local inspection, inspection_error = helpers.inspect_file_for_delete(request.file)
    if inspection_error then
        return nil, render_error(helpers, "file_read_failed", tostring(inspection_error), {
            file = request.file,
        })
    end

    local changed_span = build_delete_span(inspection.removed_lines)
    return {
        request = request,
        inspection = inspection,
        changed_span = changed_span,
        file_record = helpers.build_delete_file_record(request.file, inspection.content, inspection.removed_lines),
    }, nil
end

-- Apply all prepared delete operations in request order.
-- 按请求顺序落盘执行全部已准备的删除操作。
--
-- Parameters:
--     helpers: Shared helper table.
--     operations: Array-style prepared operation list.
-- 参数：
--     helpers：共享辅助表。
--     operations：数组形式的已准备操作列表。
--
-- Returns:
--     string|nil: Markdown error text on failure, otherwise nil.
-- 返回值：
--     string|nil：失败时返回 Markdown 错误文本，否则返回 nil。
local function apply_operations(helpers, operations)
    for _, operation in ipairs(operations or {}) do
        local delete_error = helpers.delete_file(ERROR_TITLE, PARAMETER_ERROR_CODES, operation.request.file)
        if delete_error then
            return delete_error
        end
    end
    return nil
end

-- Render the final delete result while preserving a stable Markdown contract for single-file calls.
-- 在保持单文件调用稳定 Markdown 契约的前提下渲染最终删除结果。
--
-- Parameters:
--     helpers: Shared helper table.
--     operation: Prepared single-file delete operation.
-- 参数：
--     helpers：共享辅助表。
--     operation：已准备的单文件删除操作。
--
-- Returns:
--     string: Markdown success payload.
-- 返回值：
--     string：Markdown 成功结果文本。
local function render_single_result(helpers, operation)
    local removed_lines = tonumber(operation.inspection.removed_lines) or 0
    local removed_span = removed_lines > 0 and string.format("L1-L%d", removed_lines) or "none"
    local lines = {
        "# " .. RESULT_TITLE,
        "",
        "- status: `" .. (operation.request.no_apply and "PREVIEW_ONLY" or "APPLIED") .. "`",
        "- file: `" .. operation.request.file .. "`",
        "- content_type: `" .. tostring(operation.inspection.content_type or "text") .. "`",
        "- removed_lines: `" .. tostring(removed_lines) .. "`",
        "- removed_span: `" .. removed_span .. "`",
        "- delete_scope: `regular_file_only`",
        "",
        render_delete_preview(helpers, operation.inspection, operation.changed_span),
    }
    return table.concat(lines, "\n")
end

-- Render a compact per-file batch section for delete results.
-- 为删除结果渲染紧凑的按文件批量分节。
--
-- Parameters:
--     helpers: Shared helper table.
--     operation: Prepared delete operation.
--     file_index: 1-based file index in the batch.
--     total_files: Total number of files in the batch.
-- 参数：
--     helpers：共享辅助表。
--     operation：已准备的删除操作。
--     file_index：批量中的 1-based 文件序号。
--     total_files：批量中的文件总数。
--
-- Returns:
--     string: Markdown section for one batch item.
-- 返回值：
--     string：单个批量项的 Markdown 分节文本。
local function render_batch_section(helpers, operation, file_index, total_files)
    local removed_lines = tonumber(operation.inspection.removed_lines) or 0
    local removed_span = removed_lines > 0 and string.format("L1-L%d", removed_lines) or "none"
    local lines = {
        "## File " .. tostring(file_index) .. "/" .. tostring(total_files),
        "",
        "- file: `" .. operation.request.file .. "`",
        "- content_type: `" .. tostring(operation.inspection.content_type or "text") .. "`",
        "- removed_lines: `" .. tostring(removed_lines) .. "`",
        "- removed_span: `" .. removed_span .. "`",
        "- delete_scope: `regular_file_only`",
        "",
        render_delete_preview(helpers, operation.inspection, operation.changed_span),
    }
    return table.concat(lines, "\n")
end

-- Render the final batch delete result with one top-level summary and per-file sections.
-- 使用一个顶层摘要和逐文件分节渲染最终批量删除结果。
--
-- Parameters:
--     helpers: Shared helper table.
--     operations: Array-style prepared operation list.
--     apply: Whether the batch has been written to disk.
-- 参数：
--     helpers：共享辅助表。
--     operations：数组形式的已准备操作列表。
--     apply：该批量是否已落盘。
--
-- Returns:
--     string: Markdown batch success payload.
-- 返回值：
--     string：Markdown 批量成功结果文本。
local function render_batch_result(helpers, operations, apply)
    local lines = {
        "# " .. RESULT_TITLE,
        "",
        "- status: `" .. (apply and "APPLIED" or "PREVIEW_ONLY") .. "`",
        "- files: `" .. tostring(#(operations or {})) .. "`",
        "- limit: `" .. tostring(helpers.MAX_BATCH_FILES or 10) .. "`",
        "- delete_scope: `regular_file_only`",
    }
    for index, operation in ipairs(operations or {}) do
        table.insert(lines, "")
        table.insert(lines, render_batch_section(helpers, operation, index, #operations))
    end
    return table.concat(lines, "\n")
end

-- Build one aggregated host `change_set` result for batch and single delete calls.
-- 为批量和单文件删除调用构造一个聚合宿主 `change_set` 结果。
--
-- Parameters:
--     helpers: Shared helper table.
--     capability: Capability snapshot from resolve_host_result_capability.
--     operations: Array-style prepared operation list.
--     apply: Whether the operation has been written to disk.
-- 参数：
--     helpers：共享辅助表。
--     capability：来自 resolve_host_result_capability 的能力快照。
--     operations：数组形式的已准备操作列表。
--     apply：操作是否已经落盘。
--
-- Returns:
--     table|nil: Optional host structured change_set result.
-- 返回值：
--     table|nil：可选的宿主结构化 change_set 结果。
local function build_host_result(helpers, capability, operations, apply)
    local file_records = {}
    for _, operation in ipairs(operations or {}) do
        table.insert(file_records, operation.file_record)
    end
    local summary = string.format("%s deletion of %d file%s.", apply and "Applied" or "Previewed", #file_records, #file_records == 1 and "" or "s")
    return helpers.build_change_set_host_result(capability, apply, summary, file_records)
end

-- Run the delete entry with shared helpers and optional host change_set output.
-- 使用共享辅助与可选宿主 change_set 输出执行删除入口。
--
-- Parameters:
--     args: Raw entry argument table from LuaSkills runtime.
-- 参数：
--     args：LuaSkills 运行时传入的原始参数表。
--
-- Returns:
--     string: Markdown primary result for AI and text consumers.
--     nil: No overflow mode is used by this tool.
--     nil: No template hint is used by this tool.
--     table|nil: Optional host structured change_set result.
-- 返回值：
--     string：面向 AI 与文本消费方的 Markdown 主结果。
--     nil：本工具不使用 overflow mode。
--     nil：本工具不使用 template hint。
--     table|nil：可选的宿主结构化 change_set 结果。
return function(args)
    local helpers = load_shared_file_helpers()
    local requests, is_batch, validation_error = collect_requests(helpers, args)
    if validation_error then
        return validation_error
    end

    local operations = {}
    for _, request in ipairs(requests or {}) do
        local operation, operation_error = prepare_operation(helpers, request)
        if operation_error then
            return operation_error
        end
        table.insert(operations, operation)
    end

    -- Default to deleting unless the caller explicitly requested preview-only behavior.
    -- 除非调用方显式请求仅预览，否则默认删除。
    local apply = not (requests[1] and requests[1].no_apply == true)
    if apply then
        local delete_error = apply_operations(helpers, operations)
        if delete_error then
            return delete_error
        end
    end

    local capability = helpers.resolve_host_result_capability()
    local host_result = build_host_result(helpers, capability, operations, apply)
    if is_batch then
        return render_batch_result(helpers, operations, apply), nil, nil, host_result
    end
    return render_single_result(helpers, operations[1]), nil, nil, host_result
end
