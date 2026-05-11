--[[
vulcan-workmem
通过显式共享运行时模块暴露 WorkMem 核心，保持既有 entry 文件与工具名不变。
Expose the WorkMem core through an explicit shared runtime module while preserving the existing entry files and tool names.
]]

--[[
解析当前运行时注入的 skill 目录，作为本地共享模块的查找根路径。
Resolve the current skill directory injected by the runtime and use it as the lookup root for local shared modules.
]]
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

--[[
解析当前入口文件所在目录，确保包装层与共享运行时使用同一相对定位基准。
Resolve the current entry directory so the wrapper and shared runtime use the same relative lookup base.
]]
local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

--[[
加载共享 WorkMem 运行时模块，并要求其返回 table 结果。
Load the shared WorkMem runtime module and require it to return a table result.
]]
local function load_workmem_runtime()
    local module_path = vulcan.path.join(get_entry_dir(), "lib", "workmem_runtime.lua")
    local chunk, load_error = loadfile(module_path)
    if not chunk then
        error("Failed to load workmem_runtime.lua: " .. tostring(load_error))
    end

    local ok, runtime_module = pcall(chunk)
    if not ok or type(runtime_module) ~= "table" then
        error("workmem_runtime.lua did not return a valid module: " .. tostring(runtime_module))
    end
    return runtime_module
end

return load_workmem_runtime()
