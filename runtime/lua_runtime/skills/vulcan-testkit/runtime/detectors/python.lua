--[[
Python log detector for compile errors and tracebacks.
Python 编译错误与 traceback 的日志 detector。
]]

--- Detect Python logs from source locations or traceback markers.
--- 通过源码位置或 traceback 标记识别 Python 日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    local lower_log = context.lower_log
    if lower_log:find("%.py:%d+:") or lower_log:find("traceback %(most recent call last%)") then
        return {
            tool = "python",
            adapters = { "python", "pytest", "generic" },
        }
    end
    return nil
end

return {
    key = "python",
    detect = detect,
}
