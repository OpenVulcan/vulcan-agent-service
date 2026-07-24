--[[
Mypy validation profile for Python type checking.
Mypy Python 类型检查验证 profile。
]]

return {
    key = "mypy",
    aliases = { "mypy" },
    adapters = { "generic" },
    allow_empty_args = true,
    blocked_any_args = {
        ["--install-types"] = true,
    },
}
