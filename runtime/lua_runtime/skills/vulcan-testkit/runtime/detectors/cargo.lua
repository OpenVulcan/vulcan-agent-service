--[[
Cargo log detector for Rust compiler and test output.
Rust 编译器与测试输出的 Cargo 日志 detector。
]]

--- Detect Cargo-like logs from compiler summary markers.
--- 通过编译器摘要标记识别 Cargo 风格日志。
--- @param context table Detection context.
--- @return table|nil
local function detect(context)
    local lower_log = context.lower_log
    if lower_log:find("could not compile", 1, true) or lower_log:find("error[e", 1, true) then
        return {
            tool = "cargo",
            adapters = { "cargo", "generic" },
        }
    end
    return nil
end

return {
    key = "cargo",
    detect = detect,
}
