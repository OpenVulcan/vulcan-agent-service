--[[
codekit-ast-detail
基于显式共享运行时组装 AST detail 入口，保留稳定的工具协议与 helper 可见面。
Assemble the AST detail entry on top of explicit shared runtime modules while preserving the stable tool contract and helper visibility.
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
local AST_RENDER = load_shared_module("shared_ast_render.lua")

local initialize_ast_client_budget = AST_RUNTIME.initialize_ast_client_budget
local build_ast_grep_ffi_library_candidates = AST_RUNTIME.build_ast_grep_ffi_library_candidates
local validate_path_argument = AST_RUNTIME.validate_path_argument
local validate_recursive_argument = AST_RUNTIME.validate_recursive_argument
local validate_noignore_argument = AST_RUNTIME.validate_noignore_argument
local validate_extension_argument = AST_RUNTIME.validate_extension_argument
local validate_comment_argument = AST_RUNTIME.validate_comment_argument
local validate_detail_paths_argument = AST_RUNTIME.validate_detail_paths_argument
local classify_target_path_modes = AST_RUNTIME.classify_target_path_modes
local collect_files = AST_RUNTIME.collect_files
local find_binary = AST_RUNTIME.find_binary
local run_language_scan = AST_RUNTIME.run_language_scan
local run_inline_rule_scan = AST_RUNTIME.run_inline_rule_scan
local normalize_symbol = AST_RUNTIME.normalize_symbol
local deduplicate_symbols = AST_RUNTIME.deduplicate_symbols
local build_symbol_tree = AST_RUNTIME.build_symbol_tree
local get_file_line_count = AST_RUNTIME.get_file_line_count
local build_file_content = AST_RUNTIME.build_file_content

local build_ast_detail_text = AST_RENDER.build_ast_detail_text
local finalize_ast_detail_content = AST_RENDER.finalize_ast_detail_content
local render_codekit_error_markdown = AST_RENDER.render_codekit_error_markdown

-- 技能入口 / Skill entry point invoked by the MCP host runtime.
return function(args)
    local _, client_limit_error = initialize_ast_client_budget()
    if client_limit_error then
        return render_codekit_error_markdown("CodeKit AST Detail Error", client_limit_error)
    end

    -- 为 `codekit-rg`、`codekit-markdown-menu` 与 `codekit-ast-tree` 保留共享 helper 的闭包 upvalue。
    -- Keep shared helper functions as closure upvalues so `codekit-rg`, `codekit-markdown-menu`, and `codekit-ast-tree` can continue extracting them.
    if args and args.__codekit_helper_probe == "__never__" then
        validate_path_argument(args.path)
        validate_recursive_argument(args.recursive)
        validate_noignore_argument(args.noignore)
        validate_extension_argument(args.extensions)
        local _keep_inline_rule_scanner = run_inline_rule_scan
        if _keep_inline_rule_scanner == "__never__" then
            return ""
        end
    end

    local target_paths, path_error = validate_detail_paths_argument(args and args.paths)
    if path_error then
        return render_codekit_error_markdown("CodeKit AST Detail Error", path_error)
    end

    local include_comments, comment_error = validate_comment_argument(args and args.comment)
    if comment_error then
        return render_codekit_error_markdown("CodeKit AST Detail Error", comment_error)
    end

    local target_mode, target_mode_error = classify_target_path_modes(target_paths)
    if target_mode_error then
        return render_codekit_error_markdown("CodeKit AST Detail Error", target_mode_error)
    end
    if target_mode ~= "file" then
        return render_codekit_error_markdown("CodeKit AST Detail Error", {
            error = "explicit_files_required",
            message = "codekit-ast-detail accepts only explicit file paths; directories and mixed path sets are not supported",
        })
    end

    local scanner_client, _, library_name, scanner_error = find_binary()
    if not scanner_client then
        return render_codekit_error_markdown("CodeKit AST Detail Error", {
            error = "ast_grep_ffi_not_found",
            message = "ast-grep FFI library was not found or could not be loaded",
            expected_paths = build_ast_grep_ffi_library_candidates(library_name),
            details = scanner_error,
        })
    end

    local files, _, errors, collection_error = collect_files(target_paths, false, nil, true)
    if collection_error then
        return render_codekit_error_markdown("CodeKit AST Detail Error", collection_error)
    end
    errors = errors or {}
    if #files == 0 then
        return render_codekit_error_markdown("CodeKit AST Detail Error", {
            error = "no_supported_files_found",
            message = "codekit-ast-detail could not analyze any supported source file from the provided paths",
            requested_paths = target_paths,
            errors = errors,
        })
    end

    local grouped_files = {}
    for _, file_info in ipairs(files) do
        grouped_files[file_info.language] = grouped_files[file_info.language] or {}
        table.insert(grouped_files[file_info.language], file_info.path)
    end

    local normalized_by_file = {}
    for language_key, file_paths in pairs(grouped_files) do
        local matches, diagnostics = run_language_scan(scanner_client, nil, language_key, file_paths)
        if diagnostics and #diagnostics > 0 then
            table.insert(errors, { group = language_key, diagnostics = diagnostics })
        end
        for _, match in ipairs(matches or {}) do
            local symbol = normalize_symbol(match, language_key)
            if symbol then
                normalized_by_file[symbol.file] = normalized_by_file[symbol.file] or {}
                table.insert(normalized_by_file[symbol.file], symbol)
            end
        end
    end

    local file_results = {}
    local total_items = 0
    local files_with_symbols = 0
    for _, file_info in ipairs(files) do
        local symbols = deduplicate_symbols(normalized_by_file[file_info.path] or {})
        local tree = (#symbols > 0) and build_symbol_tree(symbols) or {}
        table.insert(file_results, {
            file = file_info.display_file or file_info.path,
            lines = get_file_line_count(file_info.path),
            symbol_count = #symbols,
            content = build_file_content(tree, include_comments),
        })
        if #symbols > 0 then
            files_with_symbols = files_with_symbols + 1
            total_items = total_items + #symbols
        end
    end

    table.sort(file_results, function(left, right)
        return left.file < right.file
    end)

    local meta = {
        files_scanned = #files,
        files_with_symbols = files_with_symbols,
        items_found = total_items,
        errors = errors,
    }

    return finalize_ast_detail_content(
        build_ast_detail_text({
            files_scanned = meta.files_scanned,
            files_with_symbols = meta.files_with_symbols,
            items_found = meta.items_found,
            files = file_results,
            errors = meta.errors,
        }),
        {
            string.format("files_scanned: %d", meta.files_scanned or 0),
            string.format("files_with_symbols: %d", meta.files_with_symbols or 0),
            string.format("items_found: %d", meta.items_found or 0),
        }
    )
end
