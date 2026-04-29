-- Run the set tool by loading the shared entry builder at call time.
-- 在调用时加载共享入口构建器并执行 set 工具。
return function(args)
    local entry = dofile(vulcan.path.join(tostring(vulcan.context.entry_dir or "."), "vulcan-workmem-entry.lua"))
    return entry.build_action_entry("set")(args)
end
