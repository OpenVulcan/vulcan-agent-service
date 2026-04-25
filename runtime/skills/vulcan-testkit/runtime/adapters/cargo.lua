--[[
Cargo adapter for Rust build/check/test diagnostics.
Rust build/check/test 诊断的 Cargo 适配器。
]]

--- Parse Cargo compiler and Rust test output into normalized diagnostics.
--- 将 Cargo 编译器与 Rust 测试输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    for _, line in ipairs(helpers.split_lines(text)) do
        local code_message = line:match("^error%[(E%d+)%]:%s*(.+)$")
        if code_message then
            local code, message = line:match("^error%[(E%d+)%]:%s*(.+)$")
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "compile", code .. ": " .. message, {
                raw = line,
                confidence = 0.92,
                adapter = "cargo",
            }))
        elseif line:match("^error:%s+test failed, to rerun") then
            -- Cargo test summary lines are cascade hints once the failing test or panic is known.
            -- 当失败测试或 panic 已经识别后，Cargo 测试摘要行只属于级联提示。
        elseif line:match("^error:%s+") and not line:match("^error:%s+could not compile") then
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", phase == "test" and "test" or "compile", line:gsub("^error:%s+", ""), {
                raw = line,
                confidence = 0.86,
                adapter = "cargo",
            }))
        elseif line:match("^warning:%s+`.+`.+generated%s+%d+%s+warning") then
            -- Cargo warning summary lines repeat earlier warning details and are not actionable roots.
            -- Cargo warning 汇总行会重复前面的 warning 细节，不属于可行动根因。
        elseif line:match("^warning:%s+") then
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("warning", "warning", line:gsub("^warning:%s+", ""), {
                raw = line,
                confidence = 0.72,
                adapter = "cargo",
            }))
        else
            local file, row, column = line:match("^%s*%-%-%>%s+(.+):(%d+):(%d+)")
            if file then
                helpers.attach_location_to_latest(diagnostics, file, row, column)
            end

            local test_name = line:match("^test%s+([^%s]+)%s+%.%.%.%s+FAILED")
            if test_name then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", "Rust test failed: " .. test_name, {
                    test = test_name,
                    raw = line,
                    confidence = 0.9,
                    adapter = "cargo",
                }))
            end

            local panic_test, panic_file, panic_line, panic_column =
                line:match("^thread '([^']+)' panicked at (.+):(%d+):(%d+):")
            if not panic_test then
                panic_test, panic_file, panic_line, panic_column =
                    line:match("^thread '([^']+)'%s+%([^%)]+%)%s+panicked at (.+):(%d+):(%d+):")
            end
            if panic_test then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "panic", "Rust panic in " .. panic_test, {
                    test = panic_test,
                    file = panic_file,
                    line = panic_line,
                    column = panic_column,
                    raw = line,
                    confidence = 0.9,
                    adapter = "cargo",
                }))
            end
        end
    end
    return diagnostics
end
