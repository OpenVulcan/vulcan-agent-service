-- codeview_ts: Scan TypeScript files and extract function information
-- Uses lfs for directory traversal and vulcan.fs_read for file reading

local ts_patterns = {
    "%.ts$",
    "%.tsx$",
    "%.js$",
    "%.jsx$",
}

local function is_ts_file(filename)
    for _, pat in ipairs(ts_patterns) do
        if filename:match(pat) then
            return true
        end
    end
    return false
end

local function extract_functions(filepath, content)
    local functions = {}
    local line_num = 0

    for line in content:gmatch("[^\r\n]*") do
        line_num = line_num + 1

        local fn = line:match("function%s+([%w_]+)%s*%(")
        if not fn then fn = line:match("const%s+([%w_]+)%s*=%s*function%s*%(") end
        if not fn then fn = line:match("const%s+([%w_]+)%s*=%s*%([^)]*%)%s*=>") end
        if not fn then fn = line:match("const%s+([%w_]+)%s*=%s*async%s+%([^)]*%)%s*=>") end
        if not fn then fn = line:match("export%s+function%s+([%w_]+)%s*%(") end
        local cls = line:match("class%s+([%w_]+)")
        local iface = line:match("interface%s+([%w_]+)")

        if fn then
            table.insert(functions, {
                kind = "function",
                name = fn,
                line = line_num,
                file = filepath,
            })
        end
        if cls then
            table.insert(functions, {
                kind = "class",
                name = cls,
                line = line_num,
                file = filepath,
            })
        end
        if iface then
            table.insert(functions, {
                kind = "interface",
                name = iface,
                line = line_num,
                file = filepath,
            })
        end
    end

    return functions
end

local function scan_dir(dir, recursive)
    local all_files = {}

    local function walk(current_dir)
        for entry in lfs.dir(current_dir) do
            if entry ~= "." and entry ~= ".." then
                local full_path = current_dir .. package.config:sub(1,1) .. entry
                local attr = lfs.attributes(full_path)
                if attr and attr.mode == "directory" then
                    if recursive then
                        walk(full_path)
                    end
                elseif is_ts_file(entry) then
                    table.insert(all_files, full_path)
                end
            end
        end
    end

    walk(dir)
    return all_files
end

return function(args)
    local dir = args.dir or "."
    local recursive = args.recursive or false

    local files = scan_dir(dir, recursive)
    local all_functions = {}

    for _, filepath in ipairs(files) do
        local ok, content = pcall(vulcan.fs_read, filepath)
        if ok and content then
            local fns = extract_functions(filepath, content)
            for _, fn in ipairs(fns) do
                table.insert(all_functions, fn)
            end
        else
            vulcan.log("warn", string.format("Could not read %s: %s", filepath, tostring(content)))
        end
    end

    return {
        directory = dir,
        files_scanned = #files,
        items_found = #all_functions,
        items = all_functions,
    }
end
