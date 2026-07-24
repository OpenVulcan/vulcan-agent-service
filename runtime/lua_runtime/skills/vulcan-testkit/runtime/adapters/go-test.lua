--[[
Go test adapter for JSON and text diagnostics.
Go test JSON 与文本诊断适配器。
]]

--- Parse Go test JSON or text output into normalized diagnostics.
--- 将 Go test JSON 或文本输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    local active_test = nil
    for _, line in ipairs(helpers.split_lines(text)) do
        if helpers.starts_with(helpers.trim(line), "{") then
            local ok, event = pcall(helpers.json_decode, line)
            if ok and type(event) == "table" then
                if event.Test and event.Action == "run" then
                    active_test = event.Test
                elseif event.Test and event.Action == "fail" then
                    helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", "Go test failed: " .. tostring(event.Test), {
                        test = event.Test,
                        raw = line,
                        confidence = 0.92,
                        adapter = "go-test",
                    }))
                elseif type(event.Output) == "string" then
                    local file, row, message = event.Output:match("^%s*(.+%.go):(%d+):%s*(.+)$")
                    if file then
                        helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "go", message, {
                            file = file,
                            line = row,
                            test = event.Test or active_test,
                            raw = event.Output,
                            confidence = 0.82,
                            adapter = "go-test",
                        }))
                    end
                end
            end
        else
            local fail_test = line:match("^%-%-%-%s+FAIL:%s+([^%s]+)")
            if fail_test then
                active_test = fail_test
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "test_failure", "Go test failed: " .. fail_test, {
                    test = fail_test,
                    raw = line,
                    confidence = 0.9,
                    adapter = "go-test",
                }))
            end

            local go_file, go_line, go_message = line:match("^%s*(.+%.go):(%d+):%s*(.+)$")
            if go_file then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "go", go_message, {
                    file = go_file,
                    line = go_line,
                    test = active_test,
                    raw = line,
                    confidence = 0.82,
                    adapter = "go-test",
                }))
            end
        end
    end
    return diagnostics
end
