--[[
Python adapter for compile, traceback, and syntax diagnostics.
Python 编译、traceback 与语法诊断适配器。
]]

--- Parse Python compile, traceback, and syntax error output into normalized diagnostics.
--- 将 Python 编译、traceback 与语法错误输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    local pending_file = nil
    local pending_line = nil

    for _, line in ipairs(helpers.split_lines(text)) do
        local file, row = line:match('^%s*File%s+"([^"]+)",%s+line%s+(%d+)')
        if file then
            pending_file = file
            pending_line = row
        else
            local error_name, message = line:match("^%s*([%w_%.]+Error):%s*(.+)$")
            if error_name then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "python", error_name .. ": " .. message, {
                    file = pending_file,
                    line = pending_line,
                    raw = line,
                    confidence = pending_file and 0.86 or 0.68,
                    adapter = "python",
                }))
            end

            local py_file, py_line, py_message = line:match("^(.+%.py):(%d+):%s*(.+)$")
            if py_file and not py_message:match("^%s*$") then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "python", py_message, {
                    file = py_file,
                    line = py_line,
                    raw = line,
                    confidence = 0.72,
                    adapter = "python",
                }))
            end
        end
    end

    return diagnostics
end
