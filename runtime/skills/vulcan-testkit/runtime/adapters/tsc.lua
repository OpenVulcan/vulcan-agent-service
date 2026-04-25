--[[
TypeScript compiler adapter for typecheck diagnostics.
TypeScript 编译器 typecheck 诊断适配器。
]]

--- Parse TypeScript compiler output into normalized diagnostics.
--- 将 TypeScript 编译器输出解析为规范化诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param helpers table Shared helper functions.
--- @return table
return function(text, phase, helpers)
    local diagnostics = {}
    for _, line in ipairs(helpers.split_lines(text)) do
        local file, row, column, code, message = line:match("^(.+)%((%d+),(%d+)%)%:%s+(error%s+TS%d+)%:%s+(.+)$")
        if file then
            helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "typecheck", code .. ": " .. message, {
                file = file,
                line = row,
                column = column,
                raw = line,
                confidence = 0.94,
                adapter = "tsc",
            }))
        else
            local generic_code, generic_message = line:match("^(error%s+TS%d+)%:%s+(.+)$")
            if generic_code then
                helpers.add_diagnostic(diagnostics, helpers.diagnostic("error", "typecheck", generic_code .. ": " .. generic_message, {
                    raw = line,
                    confidence = 0.82,
                    adapter = "tsc",
                }))
            end
        end
    end
    return diagnostics
end
