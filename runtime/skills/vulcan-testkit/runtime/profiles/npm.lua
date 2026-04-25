--[[
JavaScript package-manager validation profile for npm, pnpm, and yarn.
npm、pnpm 与 yarn 的 JavaScript 包管理器验证 profile。
]]

-- Shared JavaScript test runner blocklist keeps exec Jest and Vitest behavior aligned with direct profiles.
-- 共享 JavaScript 测试运行器拦截表，使 exec Jest 与 Vitest 行为和直接 profile 保持一致。
local JS_TEST_ARGS = dofile(vulcan.path.join(vulcan.context.entry_dir, "profiles", "js_test_args.lua"))

-- Shared TypeScript noEmit policy keeps exec tsc behavior aligned with the direct profile.
-- 共享 TypeScript noEmit 策略，使 exec tsc 行为和直接 profile 保持一致。
local TSC_ARGS = dofile(vulcan.path.join(vulcan.context.entry_dir, "profiles", "tsc_args.lua"))

-- ESLint options that can mutate files, write reports, or enter interactive setup.
-- 可能修改文件、写入报告或进入交互式初始化的 ESLint 选项。
local ESLINT_BLOCKED_ARGS = {
    ["--cache"] = true,
    ["--fix"] = true,
    ["--init"] = true,
    ["--output-file"] = true,
    ["-o"] = true,
    init = true,
}

--- Copy blocked argument keys from one table into another.
--- 将被拦截的参数键从一个表复制到另一个表。
--- @param target table Target blocked-argument set.
--- @param source table Source blocked-argument set.
--- @return table
local function add_blocked_args(target, source)
    for key, value in pairs(source or {}) do
        target[key] = value
    end
    return target
end

-- Package-manager level blocked options apply to scripts and exec passthrough arguments.
-- 包管理器级别的拦截选项会作用于脚本与 exec 透传参数。
local PACKAGE_BLOCKED_ANY_ARGS = add_blocked_args({
    ["--cache"] = true,
    ["--fix"] = true,
    ["--init"] = true,
    ["--interactive"] = true,
    ["--open"] = true,
    ["--output-file"] = true,
    ["--unsafe-fixes"] = true,
    ["--watch"] = true,
    ["--watchAll"] = true,
    ["--watchall"] = true,
    ["--write"] = true,
    ["-o"] = true,
    ["-w"] = true,
}, JS_TEST_ARGS.blocked_any_args)

--- Validate package-manager run-script profiles.
--- 校验包管理器 run-script profile。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param first_index number First command index.
--- @return string|nil
local function validate_run_script(request, context, first_index)
    local spec = context.spec
    local program = context.program
    local script_name = context.next_non_option_arg(
        request.args,
        (first_index or 1) + 1,
        {
            ["--if-present"] = true,
            ["--silent"] = true,
            ["--verbose"] = true,
            ["-s"] = true,
        },
        nil
    )
    if script_name == nil then
        return "`" .. program .. " run` requires a script name from TestKit's validation allowlist."
    end
    if spec.blocked_run_scripts and spec.blocked_run_scripts[script_name] then
        return "`" .. program .. " run " .. script_name .. "` looks like a long-running app script, not validation."
    end
    if spec.allowed_run_scripts and spec.allowed_run_scripts[script_name] then
        return nil
    end
    return "`" .. program .. " run " .. script_name .. "` is not in TestKit's validation script allowlist."
end

--- Validate npm exec eslint without allowing mutation or interactive setup flags.
--- 校验 npm exec eslint，避免允许变更文件或交互式初始化参数。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param executable_index number Executable argument index.
--- @return string|nil
local function validate_eslint_target(request, context, executable_index)
    local blocked = context.find_blocked_any_arg_from(request.args, (executable_index or 1) + 1, ESLINT_BLOCKED_ARGS)
    if blocked then
        return "`" .. context.program .. " exec eslint " .. blocked .. "` is not a bounded validation option for TestKit."
    end
    return nil
end

--- Validate package-manager JavaScript test runner exec targets without write/watch modes.
--- 校验包管理器 JavaScript 测试运行器 exec 目标，避免写入或 watch 模式。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param executable string Test runner executable.
--- @param executable_index number Executable argument index.
--- @return string|nil
local function validate_js_test_target(request, context, executable, executable_index)
    local blocked = context.find_blocked_any_arg_from(request.args, (executable_index or 1) + 1, JS_TEST_ARGS.blocked_any_args)
    if blocked then
        return "`" .. context.program .. " exec " .. executable .. " " .. blocked .. "` is not a bounded validation option for TestKit."
    end
    return nil
end

--- Validate package-manager exec targets.
--- 校验包管理器 exec 目标。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param first_index number First command index.
--- @return string|nil
local function validate_exec_target(request, context, first_index)
    local program = context.program
    local executable, executable_index = context.next_non_option_arg(
        request.args,
        (first_index or 1) + 1,
        {
            ["--"] = true,
            ["--silent"] = true,
            ["--verbose"] = true,
            ["-s"] = true,
        },
        nil
    )
    if executable == "tsc" then
        local disabled_no_emit = executable_index and TSC_ARGS.disabled_no_emit_arg(request.args, executable_index + 1) or nil
        if disabled_no_emit then
            return "`" .. program .. " exec tsc " .. disabled_no_emit .. "` disables noEmit and is outside TestKit validation scope."
        end
        if executable_index and TSC_ARGS.has_enabled_no_emit(request.args, executable_index + 1) then
            return nil
        end
        return "`" .. program .. " exec tsc` requires `--noEmit` for TestKit typecheck validation."
    end
    if executable == "vitest" then
        local blocked = validate_js_test_target(request, context, executable, executable_index)
        if blocked then
            return blocked
        end
        if executable_index
            and (context.args_contain_from(request.args, executable_index + 1, "run")
                or context.args_contain_from(request.args, executable_index + 1, "--run"))
        then
            return nil
        end
        return "`" .. program .. " exec vitest` requires `run` or `--run`; plain vitest can enter watch mode."
    end
    if executable == "jest" then
        return validate_js_test_target(request, context, executable, executable_index)
    end
    if executable == "eslint" then
        return validate_eslint_target(request, context, executable_index)
    end
    return "`" .. program .. " exec " .. tostring(executable or "") .. "` is not in TestKit's validation executable allowlist."
end

--- Validate npm, pnpm, and yarn arguments.
--- 校验 npm、pnpm 与 yarn 参数。
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

    if first_arg == "run" then
        return validate_run_script(request, context, first_index)
    end
    if first_arg == "exec" then
        return validate_exec_target(request, context, first_index)
    end
    if first_arg == "test" or first_arg == "--version" or first_arg == "-v" then
        return nil
    end
    if not spec.allowed_first_args[first_arg] then
        return "`" .. program .. " " .. tostring(first_arg or "") .. "` is not in TestKit's validation allowlist."
    end
    return nil
end

--- Return parser adapters for the concrete package manager executable.
--- 返回具体包管理器可执行文件对应的解析器适配器。
--- @param program string Program basename.
--- @return table
local function adapters_for_program(program)
    return { "tsc", "js-test", "generic" }
end

--- Return the concrete package manager name for reports.
--- 返回报告中使用的具体包管理器名称。
--- @param program string Program basename.
--- @return string
local function display_for_program(program)
    return program
end

return {
    key = "npm",
    aliases = { "npm", "pnpm", "yarn" },
    allowed_first_args = {
        test = true,
        exec = true,
        run = true,
        ["--version"] = true,
        ["-v"] = true,
    },
    allowed_run_scripts = {
        build = true,
        check = true,
        lint = true,
        test = true,
        typecheck = true,
        ["type-check"] = true,
        ["test:unit"] = true,
    },
    blocked_run_scripts = {
        dev = true,
        start = true,
        serve = true,
        preview = true,
        watch = true,
        ["test:watch"] = true,
    },
    blocked_any_args = PACKAGE_BLOCKED_ANY_ARGS,
    blocked_first_args = {
        add = true,
        ci = true,
        create = true,
        dev = true,
        init = true,
        install = true,
        link = true,
        publish = true,
        start = true,
        update = true,
    },
    flag_args = {
        ["--if-present"] = true,
        ["--silent"] = true,
        ["--verbose"] = true,
        ["-s"] = true,
    },
    value_args = {
        ["--filter"] = true,
        ["--workspace"] = true,
        ["-c"] = true,
        ["--dir"] = true,
        ["--prefix"] = true,
    },
    adapters_for_program = adapters_for_program,
    display_for_program = display_for_program,
    validate = validate,
}
