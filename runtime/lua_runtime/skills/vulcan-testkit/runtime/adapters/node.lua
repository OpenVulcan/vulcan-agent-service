--[[
Node adapter for bounded syntax-check diagnostics.
Node 有界语法检查诊断适配器。
]]

--- Parse Node.js syntax-check output into normalized diagnostics.
--- 将 Node.js 语法检查输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    local pending_file = nil
    local pending_line = nil

    for _, line in ipairs(helpers.split_lines(text)) do
        local file, row = line:match("^%s*(.+%.[cm]?[jt]sx?):(%d+)%s*$")
        if file then
            pending_file = file
            pending_line = row
        else
            local error_name, message = line:match("^%s*([%w_%.]+Error):%s*(.+)$")
            if error_name then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "javascript", error_name .. ": " .. message, {
                    file = pending_file,
                    line = pending_line,
                    raw = line,
                    confidence = pending_file and 0.86 or 0.68,
                    adapter = "node",
                }))
            end
        end
    end

    return diagnostics
end
