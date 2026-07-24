--[[
vulcan-workmem-entry
Build fixed-action WorkMem entry handlers around the shared core module.
围绕共享核心模块构建固定 action 的 WorkMem 入口处理器。
]]

--- Load the shared WorkMem core module from the current entry directory.
--- 从当前入口目录加载共享 WorkMem 核心模块。
--- @return table
local function load_workmem_core()
    local entry_dir = tostring(vulcan.context.entry_dir or ".")
    local core_path = vulcan.path.join(entry_dir, "vulcan-workmem.lua")
    local chunk, load_error = loadfile(core_path)
    if not chunk then
        error("Failed to load vulcan-workmem.lua: " .. tostring(load_error))
    end

    local ok, core = pcall(chunk)
    if not ok or type(core) ~= "table" or type(core.handle) ~= "function" then
        error("vulcan-workmem.lua did not return a valid core module: " .. tostring(core))
    end

    return core
end

--- Build one entry handler that injects a fixed WorkMem action.
--- 构建一个会注入固定 WorkMem action 的入口处理器。
--- @param action string Fixed WorkMem action.
--- @return function
local function build_action_entry(action)
    return function(args)
        local core = load_workmem_core()
        return core.handle(core.with_action(args or {}, action))
    end
end

--- Export entry builders for WorkMem wrapper files.
--- 导出供 WorkMem wrapper 文件使用的入口构建器。
return {
    build_action_entry = build_action_entry,
}
