--[[
Node.js validation profile for syntax and built-in test commands.
Node.js 语法检查与内置测试命令验证 profile。
]]

--- Validate Node.js arguments so direct script execution and runtime modes do not run.
--- 校验 Node.js 参数，防止直接脚本执行与运行时模式被执行。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @return string|nil
local function validate(request, context)
    local spec = context.spec
    local program = context.program
    local first_arg, _, first_error = context.first_profile_token(request.args, spec)
    if first_error == "unknown_option" then
        return "`" .. program .. " " .. tostring(first_arg) .. "` is not a recognized validation profile option."
    end
    if first_arg == "--check" or first_arg == "-c" or first_arg == "--test" then
        return nil
    end
    if first_arg == "--version" or first_arg == "-v" then
        return nil
    end
    return "`" .. program .. " " .. tostring(first_arg or "") .. "` is not in TestKit's validation allowlist."
end

return {
    key = "node",
    aliases = { "node" },
    adapters = { "node", "generic" },
    allowed_first_args = {
        ["--check"] = true,
        ["--test"] = true,
        ["-c"] = true,
        ["--version"] = true,
        ["-v"] = true,
    },
    blocked_any_args = {
        ["--eval"] = true,
        ["-e"] = true,
        ["--inspect"] = true,
        ["--inspect-brk"] = true,
        ["--run"] = true,
        ["--test-reporter-destination"] = true,
        ["--test-update-snapshots"] = true,
        ["--watch"] = true,
    },
    flag_args = {
        ["--enable-source-maps"] = true,
        ["--no-warnings"] = true,
        ["--trace-warnings"] = true,
    },
    validate = validate,
}
