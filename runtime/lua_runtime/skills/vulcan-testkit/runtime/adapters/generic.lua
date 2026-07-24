--[[
Generic fallback adapter for simple error and warning lines.
简单 error 与 warning 行的通用兜底适配器。
]]

--- Parse generic error and warning lines as a fallback adapter.
--- 作为兜底适配器解析通用错误与警告行。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    for _, line in ipairs(helpers.split_lines(text)) do
        local compact = helpers.trim(line)
        local lower_line = compact:lower()
        if compact ~= "" then
            local matched_source = false
            local file, row, column, rest = compact:match("^(.+):(%d+):(%d+):%s*(.+)$")
            if file and (lower_line:find("error", 1, true) or lower_line:find("failed", 1, true)) then
                matched_source = true
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "generic", rest, {
                    file = file,
                    line = row,
                    column = column,
                    raw = line,
                    confidence = 0.62,
                    adapter = "generic",
                }))
            else
                local line_file, line_row, line_rest = compact:match("^(.+):(%d+):%s*(.+)$")
                if line_file and (lower_line:find("error", 1, true) or lower_line:find("failed", 1, true)) then
                    matched_source = true
                    helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "generic", line_rest, {
                        file = line_file,
                        line = line_row,
                        raw = line,
                        confidence = 0.58,
                        adapter = "generic",
                    }))
                end
            end
            if not matched_source and (lower_line:find("error", 1, true)
                or lower_line:find("failed", 1, true)
                or lower_line:find("panic", 1, true)
                or lower_line:find("exception", 1, true))
            then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "generic", compact, {
                    raw = line,
                    confidence = 0.52,
                    adapter = "generic",
                }))
            elseif not matched_source and lower_line:find("warning", 1, true) then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("warning", "warning", compact, {
                    raw = line,
                    confidence = 0.45,
                    adapter = "generic",
                }))
            end
        end
    end
    return diagnostics
end
