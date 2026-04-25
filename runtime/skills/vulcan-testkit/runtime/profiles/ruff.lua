--[[
Ruff validation profile for lint and format-check commands.
Ruff lint 与 format-check 命令验证 profile。
]]

--- Validate Ruff arguments so mutating format/fix modes never run.
--- 校验 Ruff 参数，确保会修改文件的 format/fix 模式不会执行。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @return string|nil
local function validate(request, context)
    local spec = context.spec
    local program = context.program
    local first_arg, first_index, first_error = context.first_profile_token(request.args, spec)
    local generic_error = context.precheck_standard_profile(first_arg, first_error, spec, program)
    if generic_error then
        return generic_error
    end

    if not spec.allowed_first_args[first_arg] then
        return "`" .. program .. " " .. tostring(first_arg or "") .. "` is not in TestKit's validation allowlist."
    end
    if first_arg == "format" and not context.args_contain_from(request.args, (first_index or 1) + 1, "--check") then
        return "`" .. program .. " format` requires `--check` for TestKit validation because plain format mutates files."
    end
    return nil
end

return {
    key = "ruff",
    aliases = { "ruff" },
    adapters = { "generic" },
    allowed_first_args = {
        check = true,
        format = true,
        ["--version"] = true,
        ["-V"] = true,
    },
    blocked_any_args = {
        ["--fix"] = true,
        ["--unsafe-fixes"] = true,
        ["--watch"] = true,
    },
    validate = validate,
}
