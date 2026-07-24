--[[
Pytest log detector for Python test output.
Python 测试输出的 Pytest 日志 detector。
]]

--- Detect Pytest logs from stable report markers.
--- 通过稳定报告标记识别 Pytest 日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    local lower_log = context.lower_log
    if lower_log:find("pytest", 1, true) or lower_log:find("short test summary info", 1, true) then
        return {
            tool = "pytest",
            adapters = { "pytest", "generic" },
        }
    end
    return nil
end

return {
    key = "pytest",
    detect = detect,
}
