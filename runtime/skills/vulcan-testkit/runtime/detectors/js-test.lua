--[[
JavaScript test runner log detector for Vitest and Jest output.
Vitest 与 Jest 输出的 JavaScript 测试日志 detector。
]]

--- Detect JavaScript test runner logs from runner markers.
--- 通过测试运行器标记识别 JavaScript 测试日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    local lower_log = context.lower_log
    if lower_log:find("vitest", 1, true) then
        return {
            tool = "vitest",
            adapters = { "js-test", "generic" },
        }
    end
    if lower_log:find("jest", 1, true) then
        return {
            tool = "jest",
            adapters = { "js-test", "generic" },
        }
    end
    return nil
end

return {
    key = "js-test",
    detect = detect,
}
