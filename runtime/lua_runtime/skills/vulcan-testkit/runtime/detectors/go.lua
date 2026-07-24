--[[
Go log detector for Go compiler and test output.
Go 编译器与测试输出的日志 detector。
]]

--- Detect Go logs from source-location markers.
--- 通过源码位置标记识别 Go 日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    if context.lower_log:find("%.go:%d+:") then
        return {
            tool = "go-test",
            adapters = { "go-test", "generic" },
        }
    end
    return nil
end

return {
    key = "go-test",
    detect = detect,
}
