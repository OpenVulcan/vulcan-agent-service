--[[
Shared JavaScript test-runner argument policy for bounded validation.
JavaScript 测试运行器的有界验证共享参数策略。
]]

-- Arguments in this set may update snapshots, write reports, write coverage output, or start watch mode.
-- 此集合中的参数可能更新快照、写入报告、写入覆盖率输出或启动 watch 模式。
local BLOCKED_ANY_ARGS = {
    ["--coverage"] = true,
    ["--output-file"] = true,
    ["--outputFile"] = true,
    ["--ui"] = true,
    ["--update"] = true,
    ["--update-snapshot"] = true,
    ["--updateSnapshot"] = true,
    ["--watch"] = true,
    ["--watchAll"] = true,
    ["--watchall"] = true,
    ["-u"] = true,
    ["-w"] = true,
}

return {
    blocked_any_args = BLOCKED_ANY_ARGS,
}
