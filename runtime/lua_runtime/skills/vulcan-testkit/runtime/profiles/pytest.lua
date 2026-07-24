--[[
Pytest validation profile for bounded Python test commands.
Pytest 有界 Python 测试命令验证 profile。
]]

return {
    key = "pytest",
    aliases = { "pytest" },
    adapters = { "pytest", "generic" },
    allow_empty_args = true,
    blocked_any_args = {
        ["--cov"] = true,
        ["--cov-append"] = true,
        ["--cov-config"] = true,
        ["--cov-context"] = true,
        ["--cov-report"] = true,
        ["--html"] = true,
        ["--json-report"] = true,
        ["--json-report-file"] = true,
        ["--junit-xml"] = true,
        ["--junitxml"] = true,
        ["--looponfail"] = true,
        ["--pdb"] = true,
        ["--self-contained-html"] = true,
        ["--trace"] = true,
        ["--watch"] = true,
        ["-f"] = true,
    },
}
