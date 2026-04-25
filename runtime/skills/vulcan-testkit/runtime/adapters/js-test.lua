--[[
JavaScript test adapter for Jest and Vitest-like diagnostics.
Jest 与 Vitest 类 JavaScript 测试诊断适配器。
]]

--- Parse common Jest and Vitest output into normalized diagnostics.
--- 将常见 Jest 与 Vitest 输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    local active_file = nil
    for _, line in ipairs(helpers.split_lines(text)) do
        local fail_file = line:match("^%s*FAIL%s+(.+)$")
        if fail_file then
            active_file = helpers.trim(fail_file)
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", "JavaScript test file failed: " .. active_file, {
                file = active_file,
                raw = line,
                confidence = 0.82,
                adapter = "js-test",
            }))
        end

        local test_name = line:match("^%s*[×✕x]%s+(.+)$")
        if test_name then
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", "JavaScript test failed: " .. helpers.trim(test_name), {
                file = active_file,
                test = helpers.trim(test_name),
                raw = line,
                confidence = 0.78,
                adapter = "js-test",
            }))
        end

        local file, row, column = line:match("^%s*(.+%.[jt]sx?):(%d+):(%d+)")
        if file then
            helpers.attach_location_to_latest(diagnostics, file, row, column)
        end
    end
    return diagnostics
end
