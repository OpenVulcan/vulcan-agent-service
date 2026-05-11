
--[[
shared_ast_render
为 vulcan-codekit 提供 AST detail 相关 Markdown 渲染与错误编码辅助。
Provide Markdown rendering and error-encoding helpers for vulcan-codekit AST detail style outputs.
]]

--[[
归一化文本首尾空白，确保渲染判空逻辑稳定。
Trim leading and trailing whitespace so render-time empty-content checks stay stable.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end


local function render_error_lines(errors)
    local lines = {}
    for _, item in ipairs(errors or {}) do
        if type(item) == "string" then
            table.insert(lines, "- " .. item)
        elseif type(item) == "table" then
            if item.group then
                table.insert(lines, "- Group: " .. tostring(item.group))
            end
            for _, diagnostic in ipairs(item.diagnostics or {}) do
                table.insert(lines, "  - " .. tostring(diagnostic))
            end
        end
    end
    return lines
end

--[[
把 `codekit-ast-detail` 的扫描结果渲染为 Markdown 纯文本，便于 AI 直接阅读并继续选择下一步文件操作。
Render the `codekit-ast-detail` scan result as plain Markdown text so the AI can read it directly and choose the next file-level action.
]]
local function build_ast_detail_text(result)
    local lines = {
        "# AST DETAIL SUMMARY",
        string.format(
            "- files_scanned: %d | files_with_symbols: %d | items_found: %d | errors: %d",
            result.files_scanned or 0,
            result.files_with_symbols or 0,
            result.items_found or 0,
            #(result.errors or {})
        ),
    }

    local error_lines = render_error_lines(result.errors)
    if #error_lines > 0 then
        table.insert(lines, "")
        table.insert(lines, "## ERRORS")
        table.insert(lines, "")
        for _, line in ipairs(error_lines) do
            table.insert(lines, line)
        end
    end

    for index, file_result in ipairs(result.files or {}) do
        if index > 1 or #error_lines > 0 then
            table.insert(lines, "")
        end
        table.insert(
            lines,
            string.format(
                "[%s Lines:%d Symbols:%d]",
                tostring(file_result.file or "unknown"),
                tonumber(file_result.lines) or 0,
                tonumber(file_result.symbol_count) or 0
            )
        )
        if trim(file_result.content or "") == "" then
            table.insert(lines, "> No AST symbols found in this file.")
        else
            table.insert(lines, tostring(file_result.content))
        end
    end

    return table.concat(lines, "\n")
end

--[[
完成 AST detail 正文输出；超限策略不再由 Lua 决定，而是交给 MCP 宿主统一处理。
Finalize the AST detail body; overflow strategy is no longer decided by Lua and is delegated to the MCP host.
]]
local function finalize_ast_detail_content(markdown_text, summary_lines)
    return tostring(markdown_text or ""), vulcan.runtime.overflow_type.page
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


return {
    build_ast_detail_text = build_ast_detail_text,
    finalize_ast_detail_content = finalize_ast_detail_content,
    render_codekit_error_markdown = render_codekit_error_markdown,
}
