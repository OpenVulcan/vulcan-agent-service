--[[
TypeScript compiler log detector.
TypeScript 编译器日志 detector。
]]

--- Detect TypeScript compiler logs from TS error markers.
--- 通过 TS 错误标记识别 TypeScript 编译器日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    local lower_log = context.lower_log
    if lower_log:find("error ts", 1, true) or lower_log:find("%.%a+%(%d+,%d+%): error ts") then
        return {
            tool = "tsc",
            adapters = { "tsc", "generic" },
        }
    end
    return nil
end

return {
    key = "tsc",
    detect = detect,
}
