--[[
Go validation profile for build, test, and vet commands.
Go 构建、测试与 vet 命令验证 profile。
]]

return {
    key = "go-test",
    aliases = { "go", "go-test", "gotest" },
    adapters = { "go-test", "generic" },
    allowed_first_args = {
        build = true,
        test = true,
        vet = true,
        version = true,
    },
    blocked_first_args = {
        run = true,
        install = true,
        get = true,
        generate = true,
        clean = true,
        env = true,
        mod = true,
    },
    blocked_any_args = {
        ["-bench"] = true,
        ["-c"] = true,
        ["-exec"] = true,
        ["-fuzz"] = true,
        ["-o"] = true,
        ["-toolexec"] = true,
    },
    flag_args = {
        ["-a"] = true,
        ["-n"] = true,
        ["-race"] = true,
        ["-v"] = true,
        ["-x"] = true,
    },
    value_args = {
        ["-C"] = true,
        ["-buildmode"] = true,
        ["-mod"] = true,
        ["-p"] = true,
        ["-tags"] = true,
    },
}
