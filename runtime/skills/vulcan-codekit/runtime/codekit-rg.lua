--[[
codekit-rg
基于显式共享运行时与 RG 执行库组装入口，保留既有文本命中到 AST 结构映射的对外协议。
Assemble the RG entry on top of explicit shared runtime and RG execution libraries while preserving the public text-hit to AST mapping contract.
]]

--[[
解析当前运行时注入的 skill 目录，作为本地共享模块的查找根路径。
Resolve the current skill directory injected by the runtime and use it as the lookup root for local shared modules.
]]
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

--[[
解析当前入口文件所在目录，确保主入口与共享模块使用同一相对定位基准。
Resolve the current entry directory so the main entry and shared modules use the same relative lookup base.
]]
local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

--[[
加载一个本地共享 Lua 模块，并要求其返回 table 结果。
Load one local shared Lua module and require it to return a table result.
]]
local function load_shared_module(file_name)
    local module_path = vulcan.path.join(get_entry_dir(), "lib", tostring(file_name or ""))
    local chunk, load_error = loadfile(module_path)
    if not chunk then
        error(string.format("failed to load shared module %s: %s", tostring(file_name), tostring(load_error)))
    end

    local ok, module_value = pcall(chunk)
    if not ok or type(module_value) ~= "table" then
        error(string.format("shared module %s must return a table", tostring(file_name)))
    end
    return module_value
end

local AST_RUNTIME = load_shared_module("shared_ast_runtime.lua")
local RG_RUNTIME = load_shared_module("rg_runtime.lua")

--[[
组装 RG 运行时需要的 AST helper 集合，显式替代旧的 upvalue 借用链路。
Assemble the AST helper bundle required by RG runtime and explicitly replace the old upvalue-borrowing path.
]]
local function load_ast_runtime_helpers()
    return {
        validate_extension_argument = AST_RUNTIME.validate_extension_argument,
        validate_noignore_argument = AST_RUNTIME.validate_noignore_argument,
        collect_files = AST_RUNTIME.collect_files,
        find_binary = AST_RUNTIME.find_binary,
        run_language_scan = AST_RUNTIME.run_language_scan,
        normalize_symbol = AST_RUNTIME.normalize_symbol,
        deduplicate_symbols = AST_RUNTIME.deduplicate_symbols,
        build_symbol_tree = AST_RUNTIME.build_symbol_tree,
        get_file_line_count = AST_RUNTIME.get_file_line_count,
    }, nil
end

local initialize_rg_client_budget = RG_RUNTIME.initialize_rg_client_budget
local render_codekit_error_markdown = RG_RUNTIME.render_codekit_error_markdown
local execute = RG_RUNTIME.execute

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    local _, client_limit_error = initialize_rg_client_budget()
    if client_limit_error then
        return render_codekit_error_markdown("CodeKit RG Error", client_limit_error)
    end

    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return render_codekit_error_markdown("CodeKit RG Error", helper_error)
    end

    return execute(args, helper_bundle)
end
