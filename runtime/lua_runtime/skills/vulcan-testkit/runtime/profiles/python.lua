--[[
Python validation profile for module-based compile, test, lint, and typecheck commands.
Python 基于模块的编译、测试、lint 与类型检查命令验证 profile。
]]

--- Validate nested pytest arguments used through python -m pytest.
--- 校验通过 python -m pytest 使用的嵌套 pytest 参数。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param module_index number Module argument index.
--- @return string|nil
local function validate_pytest_module(request, context, module_index)
    local pytest_spec = context.profile_by_key("pytest")
    local blocked = context.find_blocked_any_arg_from(request.args, module_index + 1, pytest_spec.blocked_any_args)
    if blocked then
        return "`python -m pytest " .. blocked .. "` is not a bounded validation option for TestKit."
    end
    return nil
end

--- Validate nested mypy arguments used through python -m mypy.
--- 校验通过 python -m mypy 使用的嵌套 mypy 参数。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param module_index number Module argument index.
--- @return string|nil
local function validate_mypy_module(request, context, module_index)
    local mypy_spec = context.profile_by_key("mypy")
    local blocked = context.find_blocked_any_arg_from(request.args, module_index + 1, mypy_spec.blocked_any_args)
    if blocked then
        return "`python -m mypy " .. blocked .. "` is not a bounded validation option for TestKit."
    end
    return nil
end

--- Validate nested ruff arguments used through python -m ruff.
--- 校验通过 python -m ruff 使用的嵌套 ruff 参数。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @param module_index number Module argument index.
--- @return string|nil
local function validate_ruff_module(request, context, module_index)
    local ruff_spec = context.profile_by_key("ruff")
    local blocked = context.find_blocked_any_arg_from(request.args, module_index + 1, ruff_spec.blocked_any_args)
    if blocked then
        return "`python -m ruff " .. blocked .. "` is not a bounded validation option for TestKit."
    end

    local ruff_arg = context.next_non_option_arg(request.args, module_index + 1, nil, nil)
    if ruff_arg == nil then
        return "`python -m ruff` requires an explicit validation argument so TestKit can bound the command."
    end
    if ruff_arg == "format" and not context.args_contain_from(request.args, module_index + 2, "--check") then
        return "`python -m ruff format` requires `--check` for TestKit validation because plain format mutates files."
    end
    if not ruff_spec.allowed_first_args[ruff_arg] then
        return "`python -m ruff " .. tostring(ruff_arg) .. "` is not in TestKit's validation allowlist."
    end
    return nil
end

--- Validate Python arguments so only bounded validation modules run.
--- 校验 Python 参数，确保只执行有界验证模块。
--- @param request table Normalized request.
--- @param context table Profile middleware helpers.
--- @return string|nil
local function validate(request, context)
    local spec = context.spec
    local first_arg, first_index, first_error = context.first_profile_token(request.args, spec)
    local generic_error = context.precheck_standard_profile(first_arg, first_error, spec, context.program)
    if generic_error then
        return generic_error
    end

    if first_arg == "-m" then
        local module_index = (first_index or 1) + 1
        local module_name = context.lower_arg_at(request.args, module_index)
        if module_name and spec.blocked_module_args and spec.blocked_module_args[module_name] then
            return "`python -m " .. module_name .. "` writes Python bytecode cache files and is outside TestKit's bounded validation scope."
        end
        if module_name and spec.allowed_module_args and spec.allowed_module_args[module_name] then
            if module_name == "pytest" then
                return validate_pytest_module(request, context, module_index)
            elseif module_name == "mypy" then
                return validate_mypy_module(request, context, module_index)
            elseif module_name == "ruff" then
                return validate_ruff_module(request, context, module_index)
            end
            return nil
        end
        return "`python -m " .. tostring(module_name or "") .. "` is not in TestKit's validation module allowlist."
    end

    if first_arg ~= "--version" and first_arg ~= "-V" then
        return "`python " .. tostring(first_arg or "") .. "` is not in TestKit's validation allowlist. Use `python -m <validation_module>`."
    end
    return nil
end

return {
    key = "python",
    aliases = { "python", "python3", "py" },
    adapters = { "python", "generic" },
    allowed_module_args = {
        pytest = true,
        unittest = true,
        mypy = true,
        ruff = true,
    },
    blocked_module_args = {
        py_compile = true,
        compileall = true,
    },
    allowed_first_args = {
        ["-m"] = true,
        ["--version"] = true,
        ["-V"] = true,
    },
    blocked_first_args = {
        ["-c"] = true,
        ["--command"] = true,
        ["-i"] = true,
    },
    flag_args = {
        ["-B"] = true,
        ["-E"] = true,
        ["-I"] = true,
        ["-O"] = true,
        ["-OO"] = true,
        ["-S"] = true,
        ["-s"] = true,
    },
    validate = validate,
}
