--[[
Node.js log detector for JavaScript syntax-check output.
JavaScript 语法检查输出的 Node.js 日志 detector。
]]

--- Detect JavaScript logs from source-location markers.
--- 通过源码位置标记识别 JavaScript 日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    if context.lower_log:find("%.[cm]?[jt]sx?:%d+") then
        return {
            tool = "node",
            adapters = { "node", "generic" },
        }
    end
    return nil
end

return {
    key = "node",
    detect = detect,
}
