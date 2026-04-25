--[[
TypeScript compiler validation profile for no-emit type checking.
TypeScript 编译器 no-emit 类型检查验证 profile。
]]

-- Shared TypeScript noEmit policy keeps direct tsc and package-manager exec behavior aligned.
-- 共享 TypeScript noEmit 策略，使直接 tsc 与包管理器 exec 行为保持一致。
local TSC_ARGS = dofile(vulcan.path.join(vulcan.context.entry_dir, "profiles", "tsc_args.lua"))

--- Validate TypeScript compiler arguments so build/init/watch modes never run.
--- 校验 TypeScript 编译器参数，确保 build/init/watch 模式不会执行。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @return string|nil
local function validate(request, context)
    if context.args_contain_from(request.args, 1, "--version") or context.args_contain_from(request.args, 1, "-v") then
        return nil
    end
    local disabled_no_emit = TSC_ARGS.disabled_no_emit_arg(request.args, 1)
    if disabled_no_emit then
        return "`tsc " .. disabled_no_emit .. "` disables noEmit and is outside TestKit validation scope."
    end
    if TSC_ARGS.has_enabled_no_emit(request.args, 1) then
        return nil
    end
    return "`tsc` requires `--noEmit` for TestKit typecheck validation."
end

return {
    key = "tsc",
    aliases = { "tsc", "typescript" },
    adapters = { "tsc", "generic" },
    allowed_first_args = {
        ["--noEmit"] = true,
        ["--pretty"] = true,
        ["--project"] = true,
        ["-p"] = true,
        ["--version"] = true,
        ["-v"] = true,
    },
    blocked_any_args = {
        ["--build"] = true,
        ["--generateTrace"] = true,
        ["--init"] = true,
        ["--watch"] = true,
        ["-b"] = true,
        ["-w"] = true,
    },
    validate = validate,
}
