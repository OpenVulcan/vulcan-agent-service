--[[
Vitest validation profile for bounded JavaScript and TypeScript tests.
Vitest 有界 JavaScript 与 TypeScript 测试验证 profile。
]]

-- Shared JavaScript test runner blocklist keeps direct Vitest and package-manager exec behavior aligned.
-- 共享 JavaScript 测试运行器拦截表，使直接 Vitest 与包管理器 exec 行为保持一致。
local JS_TEST_ARGS = dofile(vulcan.path.join(vulcan.context.entry_dir, "profiles", "js_test_args.lua"))

return {
    key = "vitest",
    aliases = { "vitest" },
    adapters = { "js-test", "generic" },
    allowed_first_args = {
        run = true,
        ["--run"] = true,
        ["--version"] = true,
        ["-v"] = true,
    },
    blocked_any_args = JS_TEST_ARGS.blocked_any_args,
}
