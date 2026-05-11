--[[
codekit-patch
基于显式共享运行时与补丁执行库组装 patch 入口，保持结构选择器与批量替换协议稳定。
Assemble the patch entry on top of explicit shared runtime and patch libraries while preserving selector and batch-replacement contracts.
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
local PATCH_RUNTIME = load_shared_module("patch_runtime.lua")

--[[
组装 patch 运行时需要的 AST helper 集合，显式替代旧的 upvalue 借用链路。
Assemble the AST helper bundle required by patch runtime and explicitly replace the old upvalue-borrowing path.
]]
local function load_ast_runtime_helpers()
    return {
        collect_files = AST_RUNTIME.collect_files,
        find_binary = AST_RUNTIME.find_binary,
        run_language_scan = AST_RUNTIME.run_language_scan,
        run_inline_rule_scan = AST_RUNTIME.run_inline_rule_scan,
        normalize_symbol = AST_RUNTIME.normalize_symbol,
        deduplicate_symbols = AST_RUNTIME.deduplicate_symbols,
        build_symbol_tree = AST_RUNTIME.build_symbol_tree,
    }, nil
end

local validate_file_argument = PATCH_RUNTIME.validate_file_argument
local validate_structural_path_argument = PATCH_RUNTIME.validate_structural_path_argument
local collect_ast_for_file = PATCH_RUNTIME.collect_ast_for_file
local find_matching_patch_targets = PATCH_RUNTIME.find_matching_patch_targets
local build_candidate_descriptor = PATCH_RUNTIME.build_candidate_descriptor
local render_patch_error = PATCH_RUNTIME.render_patch_error
local execute_patch_batch = PATCH_RUNTIME.execute_patch_batch

-- 工具入口 / Tool entry point invoked by the MCP runtime.
return function(args)
    -- Keep helper functions as direct closure upvalues for sibling CodeKit entries.
    -- 为同级 CodeKit 入口保留 helper 函数作为直接闭包 upvalue。
    if args and args.__codekit_helper_probe == "__never__" then
        return {
            validate_file_argument = validate_file_argument,
            validate_structural_path_argument = validate_structural_path_argument,
            collect_ast_for_file = collect_ast_for_file,
            find_matching_patch_targets = find_matching_patch_targets,
            build_candidate_descriptor = build_candidate_descriptor,
        }
    end

    local helper_bundle, helper_error = load_ast_runtime_helpers()
    if helper_error then
        return render_patch_error(helper_error)
    end
    return execute_patch_batch(args, helper_bundle)
end
