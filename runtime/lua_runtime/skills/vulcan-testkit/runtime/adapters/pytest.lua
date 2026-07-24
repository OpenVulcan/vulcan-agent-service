--[[
Pytest adapter for Python test diagnostics.
Python pytest 测试诊断适配器。
]]

--- Parse pytest output and Python tracebacks into normalized diagnostics.
--- 将 pytest 输出与 Python traceback 解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    for _, line in ipairs(helpers.split_lines(text)) do
        local failed_ref, failed_message = line:match("^FAILED%s+([^%s]+)%s+%-%s+(.+)$")
        if failed_ref then
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", failed_message, {
                test = failed_ref,
                raw = line,
                confidence = 0.9,
                adapter = "pytest",
            }))
        else
            local error_ref, error_message = line:match("^ERROR%s+([^%s]+)%s+%-%s+(.+)$")
            if error_ref then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_error", error_message, {
                    test = error_ref,
                    raw = line,
                    confidence = 0.88,
                    adapter = "pytest",
                }))
            end

            local traceback_file, traceback_line = line:match('^%s*File%s+"([^"]+)",%s+line%s+(%d+)')
            if traceback_file then
                helpers.attach_location_to_latest(diagnostics, traceback_file, traceback_line, nil)
            end

            local py_file, py_line, py_message = line:match("^(.+%.py):(%d+):%s*(.+)$")
            if py_file and not py_message:match("^%s*$") then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "python", py_message, {
                    file = py_file,
                    line = py_line,
                    raw = line,
                    confidence = 0.72,
                    adapter = "pytest",
                }))
            end
        end
    end
    return diagnostics
end
