--[[
codekit-ast-detail
基于 ast-grep 的文件级结构详情工具，输出按文件聚合、可直接阅读的纯文本结构摘要。
File-level AST detail viewer powered by ast-grep. It returns file-grouped plain-text structure summaries.
]]

-- 语言注册表 / Language registry for rule bundles, file extensions, and comment styles.
local LANGUAGE_REGISTRY = {
    bash = { rule_file = "bash.yml", aliases = { "bash", "sh", "shell", "zsh" }, extensions = { "sh", "bash", "zsh" }, comments = { line_prefixes = { "#" } } },
    c = { rule_file = "c.yml", aliases = { "c" }, extensions = { "c", "h" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    cpp = { rule_file = "cpp.yml", aliases = { "cpp", "cxx", "cc", "c++", "hpp", "hh" }, extensions = { "cpp", "cxx", "cc", "c++", "hpp", "hh", "hxx", "cu", "ino" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    css = { rule_file = "css.yml", aliases = { "css" }, extensions = { "css" }, comments = { block_pairs = { { "/*", "*/" } } } },
    csharp = { rule_file = "csharp.yml", aliases = { "csharp", "cs", "c#" }, extensions = { "cs" }, comments = { line_prefixes = { "//", "///" }, block_pairs = { { "/*", "*/" } } } },
    elixir = { rule_file = "elixir.yml", aliases = { "elixir", "ex", "exs" }, extensions = { "ex", "exs" }, comments = { line_prefixes = { "#" } } },
    go = { rule_file = "go.yml", aliases = { "go", "golang" }, extensions = { "go" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    haskell = { rule_file = "haskell.yml", aliases = { "haskell", "hs" }, extensions = { "hs" }, comments = { line_prefixes = { "--" }, block_pairs = { { "{-", "-}" } } } },
    hcl = { rule_file = "hcl.yml", aliases = { "hcl", "tf", "terraform" }, extensions = { "hcl", "tf", "tfvars" }, comments = { line_prefixes = { "#", "//" }, block_pairs = { { "/*", "*/" } } } },
    html = { rule_file = "html.yml", aliases = { "html", "htm", "xhtml" }, extensions = { "html", "htm", "xhtml" }, comments = { block_pairs = { { "<!--", "-->" } } } },
    java = { rule_file = "java.yml", aliases = { "java" }, extensions = { "java" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    javascript = { rule_file = "javascript.yml", aliases = { "javascript", "js", "jsx" }, extensions = { "js", "jsx", "mjs", "cjs" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    json = { rule_file = "json.yml", aliases = { "json" }, extensions = { "json" }, comments = {} },
    kotlin = { rule_file = "kotlin.yml", aliases = { "kotlin", "kt", "kts" }, extensions = { "kt", "kts", "ktm" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    lua = { rule_file = "lua.yml", aliases = { "lua" }, extensions = { "lua" }, comments = { line_prefixes = { "--", "---" }, block_pairs = { { "--[[", "]]" } } } },
    nix = { rule_file = "nix.yml", aliases = { "nix" }, extensions = { "nix" }, comments = { line_prefixes = { "#" }, block_pairs = { { "/*", "*/" } } } },
    php = { rule_file = "php.yml", aliases = { "php" }, extensions = { "php", "phtml" }, comments = { line_prefixes = { "//", "#" }, block_pairs = { { "/*", "*/" } } } },
    python = { rule_file = "python.yml", aliases = { "python", "py" }, extensions = { "py", "pyi", "py3", "bzl" }, comments = { line_prefixes = { "#" }, docstring_tokens = { "\"\"\"", "'''" } } },
    ruby = { rule_file = "ruby.yml", aliases = { "ruby", "rb" }, extensions = { "rb", "rbw", "gemspec" }, comments = { line_prefixes = { "#" } } },
    rust = { rule_file = "rust.yml", aliases = { "rust", "rs" }, extensions = { "rs" }, comments = { line_prefixes = { "//", "///" }, block_pairs = { { "/*", "*/" } } } },
    scala = { rule_file = "scala.yml", aliases = { "scala" }, extensions = { "scala", "sc", "sbt" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    solidity = { rule_file = "solidity.yml", aliases = { "solidity", "sol" }, extensions = { "sol" }, comments = { line_prefixes = { "//", "///" }, block_pairs = { { "/*", "*/" } } } },
    swift = { rule_file = "swift.yml", aliases = { "swift" }, extensions = { "swift" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    typescript = { rule_file = "typescript.yml", aliases = { "typescript", "ts", "mts", "cts" }, extensions = { "ts", "mts", "cts" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    tsx = { rule_file = "tsx.yml", aliases = { "tsx" }, extensions = { "tsx" }, comments = { line_prefixes = { "//" }, block_pairs = { { "/*", "*/" } } } },
    yaml = { rule_file = "yaml.yml", aliases = { "yaml", "yml" }, extensions = { "yaml", "yml" }, comments = { line_prefixes = { "#" } } },
}

-- 结构类型集合 / Structural kind sets used for parent-child tree building.
local CONTAINER_KINDS = { class = true, contract = true, enum = true, impl = true, interface = true, library = true, module = true, namespace = true, object = true, protocol = true, struct = true, trait = true }
local METHOD_PARENT_KINDS = { class = true, contract = true, impl = true, interface = true, library = true, object = true, protocol = true, struct = true, trait = true }
local DEFAULT_IGNORES = {
    ["target"] = true, ["node_modules"] = true, [".git"] = true,
    ["dist"] = true, ["build"] = true, ["vendor"] = true,
    [".idea"] = true, [".vscode"] = true, ["output"] = true,
}

-- 运行时缓存与索引 / Runtime caches and lookup maps.
local FILE_CACHE = {}
local IGNORE_RULE_CACHE = {}
local LANGUAGE_ALIAS_MAP = {}
local EXTENSION_MAP = {}
local DEFAULT_EXTENSION_FILTER = {}
local MAX_AST_GREP_BATCH_FILES = 50
local FALLBACK_AST_GREP_BATCH_FILES = 24
local MAX_MATCHED_FILES = 5000
local MAX_EXPLICIT_FILES = 20
local MAX_INLINE_RESULT_BYTES = 10000
local MAX_HEADER_LINES = 4
local MAX_COMMENT_SUMMARY_BYTES = 100
local CURRENT_WORKING_DIRECTORY = nil
local LFS_MODULE = nil
local SHARED_LENGTH_HELPERS = nil
local load_shared_length_helpers
local AST_GREP_FFI_CLIENT = nil
local AST_GREP_FFI_CDEF_REGISTERED = false
local AST_GREP_FFI_DEPENDENCY_NAME = "ast-grep-ffi"
local AST_GREP_FFI_VERSION = "0.1.2"
local DEFAULT_SOURCE_LANGUAGES = {
    bash = true,
    c = true,
    cpp = true,
    csharp = true,
    elixir = true,
    go = true,
    haskell = true,
    java = true,
    javascript = true,
    kotlin = true,
    lua = true,
    nix = true,
    php = true,
    python = true,
    ruby = true,
    rust = true,
    scala = true,
    solidity = true,
    swift = true,
    typescript = true,
    tsx = true,
}

for language_key, language_spec in pairs(LANGUAGE_REGISTRY) do
    LANGUAGE_ALIAS_MAP[language_key] = language_key
    for _, alias in ipairs(language_spec.aliases or {}) do
        LANGUAGE_ALIAS_MAP[alias:lower()] = language_key
    end
    for _, extension in ipairs(language_spec.extensions or {}) do
        EXTENSION_MAP[extension:lower()] = language_key
        if DEFAULT_SOURCE_LANGUAGES[language_key] then
            DEFAULT_EXTENSION_FILTER[extension:lower()] = true
        end
    end
end

-- 基础工具函数 / Basic helpers for strings and normalization.
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

local function normalize_whitespace(text)
    return trim((tostring(text or ""):gsub("%s+", " ")))
end

local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--[[
解析当前运行时可用的宿主进程执行函数，仅接受正式节点 `vulcan.process.exec`。
Resolve the host-side process execution function and accept only the formal node `vulcan.process.exec`.

返回 / Returns:
- function|nil: 可调用的宿主执行函数；若宿主未注入则返回 nil。
  Callable host execution function, or nil when the host did not inject one.
]]
local function get_host_exec_function()
    if type(vulcan) ~= "table" then
        return nil
    end
    if type(vulcan.process) == "table" and type(vulcan.process.exec) == "function" then
        return vulcan.process.exec
    end
    return nil
end

--[[
在单次工具调用开始时初始化当前客户端的 AST 预算。
Initialize the current AST budget for this tool call.
]]
local function initialize_ast_client_budget()
    local helpers, helper_error = load_shared_length_helpers()
    if helper_error then
        return nil, helper_error
    end
    return helpers.initialize_client_budget(vulcan)
end

--[[
转义 Lua pattern 元字符，便于后续按“字面量”而非模式语义执行字符串替换。
Escape Lua pattern metacharacters so later replacements operate on literal text instead of pattern semantics.

参数 / Parameters:
- text(string): 需要转义的原始文本 / Raw text to escape.

返回 / Returns:
- string: 可安全用于 Lua pattern 的转义结果。
  Escaped text that can be safely embedded into a Lua pattern.
]]
local function escape_lua_pattern(text)
    return (tostring(text or ""):gsub("([%(%)%.%%%+%-%*%?%[%]%^%$])", "%%%1"))
end

--[[
按字面量替换文本，避免块注释起止标记和三引号 token 被 `string.gsub` 当作模式表达式解析。
Replace literal text safely so block-comment markers and triple-quote tokens are not interpreted by `string.gsub` as Lua patterns.

参数 / Parameters:
- text(string): 原始文本 / Original text.
- literal(string): 需要按字面量匹配的 token / Token to match literally.
- replacement(string|nil): 替换文本，缺省时为空字符串 / Replacement text, empty string by default.

返回 / Returns:
- string: 替换后的文本 / Replaced text.
]]
local function replace_literal(text, literal, replacement)
    local normalized_replacement = tostring(replacement or ""):gsub("%%", "%%%%")
    return (tostring(text or ""):gsub(escape_lua_pattern(literal), normalized_replacement))
end

local function split_lines(content)
    local normalized = tostring(content or ""):gsub("\r\n", "\n")
    local lines = {}
    if normalized == "" then
        return lines
    end
    for line in (normalized .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end
    return lines
end

--[[
把路径统一为使用正斜杠且无冗余分隔符的形式，便于跨平台做忽略规则匹配。
Normalize a path into a forward-slash form without redundant separators so ignore-rule matching stays cross-platform.

参数 / Parameters:
- path(string): 待规范化的路径文本 / Path text to normalize.

返回 / Returns:
- string: 适合用于忽略规则比较的标准化路径文本。
  Normalized path text suitable for ignore-rule comparisons.
]]
local function normalize_ignore_path(path)
    local normalized = tostring(path or ""):gsub("\\", "/")
    normalized = normalized:gsub("/+", "/")
    normalized = normalized:gsub("^%./", "")
    normalized = normalized:gsub("/$", "")
    return normalized
end

--[[
复制数组内容，避免在递归遍历目录时直接改写父级忽略规则列表。
Copy an array so recursive directory walking does not mutate the parent ignore-rule list in place.

参数 / Parameters:
- items(table|nil): 待复制的数组 / Array to copy.

返回 / Returns:
- table: 浅拷贝后的新数组 / Shallow-copied array.
]]
local function clone_array(items)
    local copied = {}
    for _, item in ipairs(items or {}) do
        table.insert(copied, item)
    end
    return copied
end

--[[
把完整路径转换成相对于某个忽略规则基目录的相对路径，用于实现子目录级 `.gitignore/.ignore` 作用域。
Convert a full path into a path relative to an ignore-rule base directory so subtree-scoped `.gitignore/.ignore` rules can be evaluated correctly.

参数 / Parameters:
- base_directory(string): 忽略规则所在目录 / Directory that owns the ignore rules.
- full_path(string): 待判断的完整路径 / Full path to evaluate.

返回 / Returns:
- string: 相对于基目录的路径；若无法裁剪则返回规范化后的完整路径。
  Path relative to the base directory, or the normalized full path when it cannot be trimmed safely.
]]
local function relative_ignore_path(base_directory, full_path)
    local normalized_base = normalize_ignore_path(base_directory)
    local normalized_full = normalize_ignore_path(full_path)
    if normalized_full == normalized_base then
        return ""
    end
    if starts_with(normalized_full, normalized_base .. "/") then
        return normalized_full:sub(#normalized_base + 2)
    end
    return normalized_full
end

--[[
匹配简化版 gitignore 通配符，支持 `*`、`**`、`?`，并区分路径分隔符 `/`。
Match a simplified gitignore-style glob that supports `*`, `**`, and `?`, while treating `/` as a path separator boundary.

参数 / Parameters:
- text(string): 待匹配文本 / Candidate text.
- pattern(string): 规范化后的忽略模式 / Normalized ignore pattern.

返回 / Returns:
- boolean: 若文本命中模式则返回 true，否则返回 false。
  True when the text matches the pattern; otherwise false.
]]
local function match_ignore_glob(text, pattern)
    local source = tostring(text or "")
    local glob = tostring(pattern or "")
    local source_length = #source
    local pattern_length = #glob
    local memo = {}

    local function visit(source_index, pattern_index)
        local cache_key = tostring(source_index) .. ":" .. tostring(pattern_index)
        if memo[cache_key] ~= nil then
            return memo[cache_key]
        end

        local matched = false
        if pattern_index > pattern_length then
            matched = source_index > source_length
        else
            local current = glob:sub(pattern_index, pattern_index)
            local next_char = glob:sub(pattern_index + 1, pattern_index + 1)
            if current == "*" and next_char == "*" then
                local next_index = pattern_index + 2
                while glob:sub(next_index, next_index) == "*" do
                    next_index = next_index + 1
                end
                matched = visit(source_index, next_index)
                if not matched then
                    for offset = source_index, source_length do
                        if visit(offset + 1, pattern_index) then
                            matched = true
                            break
                        end
                    end
                end
            elseif current == "*" then
                matched = visit(source_index, pattern_index + 1)
                local offset = source_index
                while not matched and offset <= source_length and source:sub(offset, offset) ~= "/" do
                    matched = visit(offset + 1, pattern_index + 1)
                    offset = offset + 1
                end
            elseif current == "?" then
                matched = source_index <= source_length
                    and source:sub(source_index, source_index) ~= "/"
                    and visit(source_index + 1, pattern_index + 1)
                    or false
            else
                matched = source_index <= source_length
                    and source:sub(source_index, source_index) == current
                    and visit(source_index + 1, pattern_index + 1)
                    or false
            end
        end

        memo[cache_key] = matched
        return matched
    end

    return visit(1, 1)
end

--[[
解析单行忽略规则，提取反选、目录限定、锚定等属性，并记录规则所属目录作用域。
Parse a single ignore-rule line, extracting negation, directory-only, and anchored attributes while recording the owning directory scope.

参数 / Parameters:
- line(string): 忽略文件中的单行文本 / Single line from an ignore file.
- base_directory(string): 忽略文件所在目录 / Directory containing the ignore file.

返回 / Returns:
- table|nil: 规范化后的忽略规则对象；空行或注释行返回 nil。
  Normalized ignore-rule object, or nil for blank/comment lines.
]]
local function parse_ignore_rule(line, base_directory)
    local normalized_line = trim(line)
    if normalized_line == "" or starts_with(normalized_line, "#") then
        return nil
    end

    local negative = false
    if starts_with(normalized_line, "!") then
        negative = true
        normalized_line = trim(normalized_line:sub(2))
    end
    if normalized_line == "" then
        return nil
    end

    local directory_only = normalized_line:sub(-1) == "/"
    if directory_only then
        normalized_line = normalized_line:sub(1, -2)
    end

    local anchored = starts_with(normalized_line, "/")
    if anchored then
        normalized_line = normalized_line:sub(2)
    end

    normalized_line = normalize_ignore_path(normalized_line)
    if normalized_line == "" then
        return nil
    end

    return {
        base_directory = normalize_ignore_path(base_directory),
        pattern = normalized_line,
        negative = negative,
        directory_only = directory_only,
        anchored = anchored,
        has_slash = normalized_line:find("/", 1, true) ~= nil,
    }
end

--[[
读取某个目录下的 `.gitignore` 与 `.ignore`，并缓存解析结果，避免递归遍历时重复读取同一路径。
Read and cache `.gitignore` and `.ignore` files for a directory so recursive traversal does not repeatedly re-read the same path.

参数 / Parameters:
- directory_path(string): 当前遍历目录 / Directory currently being traversed.

返回 / Returns:
- table: 当前目录新增的忽略规则数组 / Ignore rules newly introduced by the current directory.
]]
local function load_directory_ignore_rules(directory_path)
    local cache_key = normalize_ignore_path(directory_path)
    if IGNORE_RULE_CACHE[cache_key] then
        return IGNORE_RULE_CACHE[cache_key]
    end

    local collected = {}
    for _, ignore_file_name in ipairs({ ".gitignore", ".ignore" }) do
        local ignore_file_path = vulcan.path.join(directory_path, ignore_file_name)
        if vulcan.fs.exists(ignore_file_path) and not vulcan.fs.is_dir(ignore_file_path) then
            local ok, content = pcall(vulcan.fs.read, ignore_file_path)
            if ok then
                for _, line in ipairs(split_lines(content or "")) do
                    local parsed_rule = parse_ignore_rule(line, directory_path)
                    if parsed_rule then
                        table.insert(collected, parsed_rule)
                    end
                end
            end
        end
    end

    IGNORE_RULE_CACHE[cache_key] = collected
    return collected
end

--[[
判断单个目录项是否应被忽略，先应用默认忽略目录，再按目录作用域顺序执行 `.gitignore/.ignore` 规则覆盖。
Decide whether a directory entry should be ignored by applying default ignored directories first, then evaluating scoped `.gitignore/.ignore` rules in order.

参数 / Parameters:
- full_path(string): 目录项的完整路径 / Full path of the directory entry.
- entry_name(string): 当前目录项名称 / Current entry name.
- is_directory(boolean): 当前目录项是否为目录 / Whether the current entry is a directory.
- active_ignore_rules(table): 当前目录生效的忽略规则列表 / Ignore rules active for the current directory.
- ignore_enabled(boolean): 是否启用忽略机制 / Whether ignore behavior is enabled.

返回 / Returns:
- boolean: 若目录项应被跳过则返回 true，否则返回 false。
  True when the entry should be skipped; otherwise false.
]]
local function should_ignore_entry(full_path, entry_name, is_directory, active_ignore_rules, ignore_enabled)
    if not ignore_enabled then
        return false
    end

    if is_directory and DEFAULT_IGNORES[tostring(entry_name or ""):lower()] then
        return true
    end

    local ignored = false
    for _, rule in ipairs(active_ignore_rules or {}) do
        if not (rule.directory_only and not is_directory) then
            local candidate = (rule.anchored or rule.has_slash)
                and relative_ignore_path(rule.base_directory, full_path)
                or normalize_ignore_path(entry_name)
            if candidate ~= "" and match_ignore_glob(candidate, rule.pattern) then
                ignored = not rule.negative
            end
        end
    end
    return ignored
end

-- 路径与语言解析 / Resolve host-injected runtime paths and normalize language keys.
local function get_skill_dir()
    return tostring(vulcan.context.skill_dir or ".")
end

local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or get_skill_dir())
end

local function get_entry_file()
    return tostring(vulcan.context.entry_file or vulcan.path.join(get_entry_dir(), "codekit-ast-detail.lua"))
end

--[[
Return the normalized platform key used by LuaSkills dependency installation.
返回 LuaSkills 依赖安装使用的标准平台键。
]]
local function current_platform_key()
    local os_info = vulcan.os.info() or {}
    local architecture = trim((os_info.arch or os_info.architecture or "")):lower()
    local os_name = trim((os_info.os or "")):lower()

    if os_name == "windows" then
        if architecture == "arm64" or architecture == "aarch64" then
            return "windows-arm64"
        end
        return "windows-x64"
    end

    if os_name == "macos" or os_name == "darwin" or os_name == "osx" then
        if architecture == "arm64" or architecture == "aarch64" then
            return "macos-arm64"
        end
        return "macos-x64"
    end

    if architecture == "arm64" or architecture == "aarch64" then
        return "linux-arm64"
    end
    return "linux-x64"
end

--[[
Return the host-injected FFI dependency root for the current skill.
返回宿主为当前 skill 注入的 FFI 依赖根目录。

返回 / Returns:
- string: FFI 依赖根目录；未注入时为空字符串。
  FFI dependency root path, or an empty string when it is not injected.
]]
local function get_ffi_dependency_root()
    return trim(vulcan and vulcan.deps and vulcan.deps.ffi_path or "")
end

--[[
Resolve the platform-specific ast-grep FFI library filename.
解析当前平台对应的 ast-grep FFI 动态库文件名。

返回 / Returns:
- string: 当前平台应加载的动态库文件名。
  Dynamic library filename that should be loaded on the current platform.
]]
local function get_ast_grep_ffi_library_name()
    local os_info = vulcan.os.info()
    if os_info and os_info.os == "windows" then
        return "vulcan_codekit_ast_grep_ffi.dll"
    end
    if os_info and os_info.os == "macos" then
        return "libvulcan_codekit_ast_grep_ffi.dylib"
    end
    return "libvulcan_codekit_ast_grep_ffi.so"
end

--[[
Build possible ast-grep FFI library paths from dependency installation and local development layouts.
从依赖安装布局和本地开发布局构造可能的 ast-grep FFI 动态库路径。

参数 / Parameters:
- library_name(string): 平台动态库文件名 / Platform-specific library filename.

返回 / Returns:
- table: 候选动态库绝对路径数组 / Candidate absolute library paths.
]]
local function build_ast_grep_ffi_library_candidates(library_name)
    local candidates = {}
    local ffi_root = get_ffi_dependency_root()
    local platform_key = current_platform_key()
    if ffi_root ~= "" then
        local dependency_base = vulcan.path.join(
            ffi_root,
            AST_GREP_FFI_DEPENDENCY_NAME,
            AST_GREP_FFI_VERSION,
            platform_key
        )
        table.insert(candidates, vulcan.path.join(vulcan.path.join(dependency_base, "lib"), library_name))
        table.insert(candidates, vulcan.path.join(vulcan.path.join(dependency_base, "bin"), library_name))
        table.insert(candidates, vulcan.path.join(dependency_base, library_name))
    end

    local local_base = vulcan.path.join(get_skill_dir(), "ast-grep-ffi")
    table.insert(candidates, vulcan.path.join(vulcan.path.join(vulcan.path.join(local_base, "target"), "release"), library_name))
    table.insert(candidates, vulcan.path.join(vulcan.path.join(vulcan.path.join(local_base, "target"), "debug"), library_name))
    return candidates
end

--[[
Register ast-grep FFI C declarations exactly once for the LuaJIT process.
为 LuaJIT 进程仅注册一次 ast-grep FFI C 声明。

参数 / Parameters:
- ffi(table): LuaJIT FFI 模块 / LuaJIT FFI module.

返回 / Returns:
- boolean: 注册成功时为 true / True when registration succeeds.
- table|nil: 注册失败时的结构化错误 / Structured error when registration fails.
]]
local function register_ast_grep_ffi_cdef(ffi)
    if AST_GREP_FFI_CDEF_REGISTERED then
        return true, nil
    end
    local ok, cdef_error = pcall(ffi.cdef, [[
        const char* vulcan_codekit_ast_grep_version(void);
        char* vulcan_codekit_ast_grep_scan_json(const char* request_json);
        void vulcan_codekit_ast_grep_free_string(char* value);
    ]])
    if not ok then
        return false, {
            error = "ast_grep_ffi_cdef_failed",
            message = tostring(cdef_error),
        }
    end
    AST_GREP_FFI_CDEF_REGISTERED = true
    return true, nil
end

--[[
Load the ast-grep FFI dynamic library and cache the resulting client object.
加载 ast-grep FFI 动态库，并缓存得到的客户端对象。

返回 / Returns:
- table|nil: FFI 客户端对象 / FFI client object.
- table|nil: 加载失败时的结构化错误 / Structured error when loading fails.
]]
local function load_ast_grep_ffi_client()
    if AST_GREP_FFI_CLIENT then
        return AST_GREP_FFI_CLIENT, nil
    end

    local ffi_ok, ffi = pcall(require, "ffi")
    if not ffi_ok or type(ffi) ~= "table" then
        return nil, {
            error = "luajit_ffi_unavailable",
            message = "LuaJIT ffi module is required to load ast-grep FFI",
            details = tostring(ffi),
        }
    end

    local cdef_ok, cdef_error = register_ast_grep_ffi_cdef(ffi)
    if not cdef_ok then
        return nil, cdef_error
    end

    local library_name = get_ast_grep_ffi_library_name()
    local candidates = build_ast_grep_ffi_library_candidates(library_name)
    local load_errors = {}
    for _, library_path in ipairs(candidates) do
        if vulcan.fs.exists(library_path) then
            local loaded, library_or_error = pcall(ffi.load, library_path)
            if loaded then
                local version = ""
                local version_ok, version_pointer = pcall(library_or_error.vulcan_codekit_ast_grep_version)
                if version_ok and version_pointer ~= nil then
                    version = ffi.string(version_pointer)
                end
                AST_GREP_FFI_CLIENT = {
                    kind = "ast_grep_ffi",
                    ffi = ffi,
                    library = library_or_error,
                    library_name = library_name,
                    library_path = library_path,
                    version = version,
                }
                return AST_GREP_FFI_CLIENT, nil
            end
            table.insert(load_errors, tostring(library_or_error))
        end
    end

    return nil, {
        error = "ast_grep_ffi_library_not_found",
        message = "ast-grep FFI library was not found in the current skill dependency root",
        expected_paths = candidates,
        load_errors = load_errors,
    }
end

--[[
懒加载共享预算模块，让多个 codekit 工具复用同一套 MCP 输出/读取预算映射。
Lazily load the shared budget module so multiple codekit tools reuse one MCP output/read budget mapping.
]]
load_shared_length_helpers = function()
    if SHARED_LENGTH_HELPERS then
        return SHARED_LENGTH_HELPERS, nil
    end

    local helper_path = vulcan.path.join(get_entry_dir(), "shared_length.lua")
    local chunk, load_error = loadfile(helper_path)
    if not chunk then
        return nil, {
            error = "shared_length_load_failed",
            message = tostring(load_error),
            path = helper_path,
        }
    end

    local ok, helpers = pcall(chunk)
    if not ok or type(helpers) ~= "table" then
        return nil, {
            error = "shared_length_invalid",
            message = ok and "shared_length.lua did not return a table" or tostring(helpers),
            path = helper_path,
        }
    end

    SHARED_LENGTH_HELPERS = helpers
    return SHARED_LENGTH_HELPERS, nil
end

--[[
读取并缓存当前进程工作目录，用于把用户传入的相对扫描路径转换为稳定的绝对路径。
Read and cache the current process working directory so relative scan paths from the user can be converted into stable absolute paths.

参数 / Parameters:
- 无 / None.

返回 / Returns:
- string|nil: 当前工作目录；若获取失败则返回 nil。
  Current working directory, or nil when it cannot be resolved.
]]
local function get_current_working_directory()
    if CURRENT_WORKING_DIRECTORY then
        return CURRENT_WORKING_DIRECTORY
    end

    local runtime_cwd = vulcan and vulcan.runtime and vulcan.runtime.cwd
    if type(runtime_cwd) == "function" then
        local ok, output = pcall(runtime_cwd)
        if ok then
            local normalized = trim(output)
            if normalized ~= "" then
                CURRENT_WORKING_DIRECTORY = normalized
                return CURRENT_WORKING_DIRECTORY
            end
        end
    end

    local command = vulcan.os.info().os == "windows" and "cd" or "pwd"
    local handle = io.popen(command)
    if not handle then
        return nil
    end

    local output = trim(handle:read("*a") or "")
    handle:close()
    CURRENT_WORKING_DIRECTORY = output ~= "" and output or nil
    return CURRENT_WORKING_DIRECTORY
end

--[[
判断路径是否已经是绝对路径，避免重复拼接工作目录导致路径失真。
Determine whether a path is already absolute so the working directory is not joined twice and the path stays valid.

参数 / Parameters:
- path(string): 待判断的路径文本 / Path text to inspect.

返回 / Returns:
- boolean: 若路径已是绝对路径则返回 true，否则返回 false。
  True when the path is absolute; otherwise false.
]]
local function is_absolute_path(path)
    local normalized = tostring(path or "")
    return normalized:match("^%a:[/\\]") ~= nil
        or starts_with(normalized, "\\\\")
        or starts_with(normalized, "/")
end

--[[
将扫描目标路径规范化为绝对路径，确保 FFI 扫描器能稳定读取源文件。
Normalize the scan target path into an absolute path so the FFI scanner can read source files reliably.

参数 / Parameters:
- path(string): 用户传入或递归拼接得到的扫描路径。
  Scan path provided by the user or produced during directory walking.

返回 / Returns:
- string: 适合传给 FFI 扫描器与文件读取函数的绝对路径；解析失败时回退原值。
  Absolute path suitable for the FFI scanner and file reads; falls back to the original value when resolution fails.
]]
local function resolve_scan_path(path)
    local normalized = tostring(path or "")
    if normalized == "" or is_absolute_path(normalized) then
        return normalized
    end

    local current_directory = get_current_working_directory()
    if not current_directory or current_directory == "" then
        return normalized
    end
    if normalized == "." then
        return current_directory
    end
    return vulcan.path.join(current_directory, normalized)
end

local function detect_language_key(file_name)
    local extension = file_name:match("%.([^.]+)$")
    if not extension then
        return nil
    end
    return EXTENSION_MAP[extension:lower()]
end

--[[
提取文件名末尾扩展名并统一为小写，便于做目录扫描时的扩展名过滤。
Extract the trailing file extension and normalize it to lowercase for directory-mode extension filtering.

参数 / Parameters:
- file_name(string): 文件名或路径文本 / File name or path text.

返回 / Returns:
- string|nil: 去掉前导点后的扩展名；没有扩展名时返回 nil。
  Lowercased extension text without the leading dot, or nil when absent.
]]
local function extract_extension(file_name)
    local extension = tostring(file_name or ""):match("%.([^.]+)$")
    if not extension or extension == "" then
        return nil
    end
    return extension:lower()
end

local function find_binary()
    local client, load_error = load_ast_grep_ffi_client()
    local library_name = get_ast_grep_ffi_library_name()
    if client then
        return client, nil, library_name, nil
    end
    return nil, nil, library_name, load_error
end

local function get_rule_path(language_key)
    local language_spec = LANGUAGE_REGISTRY[language_key]
    if not language_spec then
        return nil
    end
    local rule_path = vulcan.path.join(vulcan.path.join(get_skill_dir(), "rules"), language_spec.rule_file)
    if vulcan.fs.exists(rule_path) then
        return rule_path
    end
    return nil
end

--[[
Call the ast-grep FFI scanner and decode its JSON response.
调用 ast-grep FFI 扫描器并解码其 JSON 响应。

参数 / Parameters:
- scanner_client(table): 已加载的 FFI 客户端 / Loaded FFI client.
- request(table): FFI 请求对象 / FFI request object.

返回 / Returns:
- table|nil: 命中结果数组；致命失败时为 nil。
  Match array, or nil on fatal failure.
- table: 诊断信息数组 / Diagnostic messages.
]]
local function call_ast_grep_ffi(scanner_client, request)
    if type(scanner_client) ~= "table" or scanner_client.kind ~= "ast_grep_ffi" then
        return nil, { "ast_grep_ffi_client_missing" }
    end

    local encoded_ok, encoded_request = pcall(vulcan.json.encode, request)
    if not encoded_ok or type(encoded_request) ~= "string" then
        return nil, { "ast_grep_ffi_request_encode_failed: " .. tostring(encoded_request) }
    end

    local ffi = scanner_client.ffi
    local library = scanner_client.library
    local scan_ok, response_pointer = pcall(library.vulcan_codekit_ast_grep_scan_json, encoded_request)
    if not scan_ok then
        return nil, { "ast_grep_ffi_scan_failed: " .. tostring(response_pointer) }
    end
    if response_pointer == nil then
        return nil, { "ast_grep_ffi_scan_returned_null" }
    end

    local response_ok, response_text = pcall(ffi.string, response_pointer)
    pcall(library.vulcan_codekit_ast_grep_free_string, response_pointer)
    if not response_ok then
        return nil, { "ast_grep_ffi_response_read_failed: " .. tostring(response_text) }
    end

    local decoded, decode_error = vulcan.json.decode(response_text)
    if not decoded or type(decoded) ~= "table" then
        return nil, { "ast_grep_ffi_response_decode_failed: " .. tostring(decode_error) }
    end

    local diagnostics = {}
    for _, diagnostic in ipairs(decoded.diagnostics or {}) do
        table.insert(diagnostics, tostring(diagnostic))
    end
    if decoded.ok ~= true then
        local message = tostring(decoded.error or "ast_grep_ffi_error")
        if decoded.message and tostring(decoded.message) ~= "" then
            message = message .. ": " .. tostring(decoded.message)
        end
        table.insert(diagnostics, message)
        return nil, diagnostics
    end

    return decoded.matches or {}, diagnostics
end

--[[
Run one file batch through the ast-grep FFI scanner with a rule file.
使用规则文件通过 ast-grep FFI 扫描器执行一个文件批次。

参数 / Parameters:
- scanner_client(table): 已加载的 FFI 客户端 / Loaded FFI client.
- language_key(string): 当前语言键 / Current language key.
- rule_path(string): 规则文件路径 / Rule file path.
- file_paths(table): 待扫描的文件路径列表 / File paths to scan.

返回 / Returns:
- table|nil: 命中结果数组；致命失败时为 nil。
  Match array, or nil on fatal failure.
- table: 诊断信息数组 / Diagnostic messages.
]]
local function run_scan_batch(scanner_client, language_key, rule_path, file_paths)
    return call_ast_grep_ffi(scanner_client, {
        language = language_key,
        rulePath = rule_path,
        files = file_paths,
    })
end

--[[
Run an inline ast-grep rule through the FFI scanner.
通过 FFI 扫描器执行一段内联 ast-grep 规则。

参数 / Parameters:
- scanner_client(table): 已加载的 FFI 客户端 / Loaded FFI client.
- language_key(string): 当前语言键 / Current language key.
- inline_rule_yaml(string): 内联规则 YAML / Inline rule YAML.
- file_paths(table): 待扫描的文件路径列表 / File paths to scan.

返回 / Returns:
- table|nil: 命中结果数组；致命失败时为 nil。
  Match array, or nil on fatal failure.
- table: 诊断信息数组 / Diagnostic messages.
]]
local function run_inline_rule_scan(scanner_client, language_key, inline_rule_yaml, file_paths)
    return call_ast_grep_ffi(scanner_client, {
        language = language_key,
        inlineRuleYaml = inline_rule_yaml,
        files = file_paths,
    })
end

--[[
按指定批大小执行同语言文件扫描，并合并每批返回的结构结果与诊断信息。
Execute same-language scans with the specified batch size, then merge structure results and diagnostics from every batch.

参数 / Parameters:
- scanner_client(table): ast-grep FFI 客户端 / ast-grep FFI client.
- executable_name(string|nil): 兼容旧签名的占位参数 / Compatibility placeholder for the old signature.
- language_key(string): 当前语言键 / Current language key.
- file_paths(table): 待扫描的文件路径列表 / File paths to scan.
- batch_size(number): 每批文件数量上限 / Maximum files allowed in one batch.

返回 / Returns:
- table|nil: 合并后的匹配结果；当所有批次均失败时返回 nil。
  Merged matches, or nil when every batch fails.
- table: 合并后的诊断信息列表 / Merged diagnostics list.
]]
local function run_language_scan_in_batches(scanner_client, executable_name, language_key, file_paths, batch_size)
    local rule_path = get_rule_path(language_key)
    if not rule_path then
        return nil, { "rule_file_missing:" .. tostring(language_key) }
    end

    local all_matches = {}
    local all_diagnostics = {}
    local had_failure = false
    for index = 1, #file_paths, batch_size do
        local batch = {}
        for offset = index, math.min(index + batch_size - 1, #file_paths) do
            table.insert(batch, file_paths[offset])
        end
        local matches, diagnostics = run_scan_batch(scanner_client, language_key, rule_path, batch)
        if matches == nil then
            had_failure = true
        end
        if matches then
            for _, match in ipairs(matches) do
                table.insert(all_matches, match)
            end
        end
        if diagnostics then
            for _, diagnostic in ipairs(diagnostics) do
                table.insert(all_diagnostics, diagnostic)
            end
        end
    end
    if had_failure then
        return nil, all_diagnostics
    end
    return all_matches, all_diagnostics
end

--[[
按语言批量扫描文件。常规路径下每批最多 50 个文件，以规避 Windows `CreateProcess` 命令行长度限制；若某批失败，再自动降级到更小批次。
Scan files grouped by language. The normal path caps each batch at 50 files to avoid Windows `CreateProcess` command-line length limits; failed batches are retried with smaller chunks.

参数 / Parameters:
- scanner_client(table): ast-grep FFI 客户端 / ast-grep FFI client.
- executable_name(string|nil): 兼容旧签名的占位参数 / Compatibility placeholder for the old signature.
- language_key(string): 当前语言键 / Current language key.
- file_paths(table): 待扫描的文件路径列表 / File paths to scan.

返回 / Returns:
- table|nil: 匹配结果数组；若最终失败则返回 nil。
  Match array, or nil when scanning ultimately fails.
- table: 诊断信息数组 / Diagnostic messages.
]]
local function run_language_scan(scanner_client, executable_name, language_key, file_paths)
    local primary_matches, primary_diagnostics = run_language_scan_in_batches(
        scanner_client,
        executable_name,
        language_key,
        file_paths,
        MAX_AST_GREP_BATCH_FILES
    )
    if primary_matches or #file_paths <= 1 or MAX_AST_GREP_BATCH_FILES <= FALLBACK_AST_GREP_BATCH_FILES then
        return primary_matches, primary_diagnostics
    end

    local fallback_matches, fallback_diagnostics = run_language_scan_in_batches(
        scanner_client,
        executable_name,
        language_key,
        file_paths,
        FALLBACK_AST_GREP_BATCH_FILES
    )
    if fallback_matches then
        return fallback_matches, fallback_diagnostics
    end

    local merged_diagnostics = clone_array(primary_diagnostics)
    for _, diagnostic in ipairs(fallback_diagnostics or {}) do
        table.insert(merged_diagnostics, diagnostic)
    end
    return fallback_matches, merged_diagnostics
end

--[[
Normalize the public extensions filter into a lookup set. When omitted, fall back to the default source-language set so HTML/CSS/JSON/YAML-style files are excluded by default.
将公开的 extensions 过滤参数统一解析为集合；未传时自动回退到默认代码语言集合，避免把 HTML/CSS/JSON/YAML 等非核心代码文件扫入结果。

参数 / Parameters:
- value(any): 用户传入的 `extensions` 参数 / User-provided `extensions` argument.

返回 / Returns:
- table: 规范化后的扩展名集合；未提供或为空时返回默认代码扩展集合。
  Normalized extension lookup set; when omitted or empty, the default source-code extension set is returned.
- table|nil: 参数非法时返回结构化错误对象；成功时为 nil。
  A structured error object when the argument is invalid; otherwise nil.
]]
local function validate_extension_argument(value)
    if value == nil then
        return DEFAULT_EXTENSION_FILTER, nil
    end

    local normalized_items = {}
    local seen = {}

    local function append_normalized_extension(extension_name)
        local normalized = trim(extension_name):gsub("^%.*", ""):lower()
        if normalized ~= "" and not seen[normalized] then
            seen[normalized] = true
            table.insert(normalized_items, normalized)
        end
    end

    local function append_extension_item(item)
        if type(item) ~= "string" then
            return false, type(item)
        end
        for token in tostring(item):gmatch("[^,]+") do
            local normalized = trim(token):gsub("^%.*", ""):lower()
            if normalized ~= "" then
                local language_key = nil
                if LANGUAGE_REGISTRY[normalized] then
                    language_key = normalized
                elseif LANGUAGE_ALIAS_MAP[normalized] and not EXTENSION_MAP[normalized] then
                    language_key = LANGUAGE_ALIAS_MAP[normalized]
                end

                if language_key and LANGUAGE_REGISTRY[language_key] then
                    for _, extension_name in ipairs(LANGUAGE_REGISTRY[language_key].extensions or {}) do
                        append_normalized_extension(extension_name)
                    end
                else
                    append_normalized_extension(normalized)
                end
            end
        end
        return true, nil
    end

    if type(value) == "string" then
        local ok = append_extension_item(value)
        if not ok then
            return nil, {
                error = "invalid_extensions_argument",
                message = "extensions must be a comma-separated string or string array of file extensions or language names when provided",
                actual_type = type(value),
            }
        end
    elseif type(value) == "table" then
        for _, item in ipairs(value) do
            local ok, actual_type = append_extension_item(item)
            if not ok then
                return nil, {
                    error = "invalid_extensions_argument",
                    message = "extensions array items must be strings representing file extensions or language names",
                    actual_type = actual_type,
                }
            end
        end
    else
        return nil, {
            error = "invalid_extensions_argument",
            message = "extensions must be a comma-separated string or string array of file extensions or language names when provided",
            actual_type = type(value),
        }
    end

    if #normalized_items == 0 then
        return DEFAULT_EXTENSION_FILTER, nil
    end

    local extension_filter = {}
    for _, extension_name in ipairs(normalized_items) do
        extension_filter[extension_name] = true
    end
    return extension_filter, nil
end

--[[
判断文件是否满足扩展名过滤集合；当前调用链始终会传入显式集合或默认代码语言集合。
Decide whether a file matches the extension filter set. The current flow always supplies either an explicit set or the default source-code set.

参数 / Parameters:
- file_name(string): 文件名或路径文本 / File name or path text.
- extension_filter(table|nil): 扩展名集合 / Extension lookup set.

返回 / Returns:
- boolean: 文件满足过滤条件时返回 true。
  True when the file passes the extension filter.
]]
local function matches_extension_filter(file_name, extension_filter)
    if not extension_filter then
        return true
    end
    local extension = extract_extension(file_name)
    return extension ~= nil and extension_filter[extension] == true
end

--[[
基于路径文本与展示路径构建单个可扫描文件项，仅当扩展名可识别时返回结果。
Build a single scannable file item from the real path and the display path, returning nil when the extension is unsupported.

参数 / Parameters:
- full_path(string): 实际扫描路径 / Actual full path to scan.
- display_path(string): 对外展示路径 / User-facing display path.

返回 / Returns:
- table|nil: 单个文件项；扩展名不受支持时返回 nil。
  A single file item, or nil when the extension is unsupported.
]]
local function build_file_item(full_path, display_path)
    local language_key = detect_language_key(display_path)
    if not language_key then
        return nil
    end
    return {
        path = full_path,
        display_file = display_path,
        language = language_key,
    }
end

--[[
基于单个目标路径收集可扫描文件，保持“文件模式忽略递归与扩展名、目录模式应用过滤”的既有规则。
Collect scannable files for a single target path while preserving the existing rules: file mode ignores recursion and extension filters, while directory mode applies them.

参数 / Parameters:
- target_path(string): 单个待处理路径，可以是文件或目录。
  A single target path that may point to either a file or a directory.
- recursive(boolean): 目录模式下是否递归扫描子目录。
  Whether directory mode should recurse into subdirectories.
- extension_filter(table|nil): 目录模式下使用的扩展名过滤集合。
  Extension filter set used in directory mode.
- ignore_enabled(boolean): 是否启用默认忽略目录与 `.gitignore/.ignore` 规则。
  Whether default ignored directories and `.gitignore/.ignore` rules are enabled.

返回 / Returns:
- table|nil: 当前路径收集到的文件列表；发生错误时返回 nil。
  Collected file list for the current path, or nil when an error occurs.
- string|nil: 当前路径的扫描模式，取值为 `file` 或 `directory`。
  Scan mode for the current path, either `file` or `directory`.
- table|nil: 当前路径的结构化错误对象；成功时为 nil。
  Structured error object for the current path, or nil on success.
]]
local function collect_files_for_path(target_path, recursive, extension_filter, ignore_enabled)
    local collected = {}
    local scan_root = resolve_scan_path(target_path)
    local is_directory = vulcan.fs.is_dir(scan_root)

    if not vulcan.fs.exists(scan_root) then
        return nil, nil, {
            error = "path_not_found",
            message = "path does not exist",
            path = target_path,
        }
    end

    if not is_directory then
        local file_item = build_file_item(scan_root, target_path)
        if not file_item then
            return nil, nil, {
                error = "unsupported_file_extension",
                message = "path points to a file whose extension is not supported",
                path = target_path,
            }
        end
        table.insert(collected, file_item)
        return collected, "file", nil
    end

    local function walk(current_directory, display_directory, inherited_ignore_rules)
        local active_ignore_rules = inherited_ignore_rules or {}
        if ignore_enabled then
            active_ignore_rules = clone_array(active_ignore_rules)
            for _, current_rule in ipairs(load_directory_ignore_rules(current_directory)) do
                table.insert(active_ignore_rules, current_rule)
            end
        end

        local entries = vulcan.fs.list(current_directory)
        if not entries then
            return
        end
        table.sort(entries)

        for _, entry in ipairs(entries) do
            if entry ~= "." and entry ~= ".." then
                local full_path = vulcan.path.join(current_directory, entry)
                local display_path = (display_directory == "." or display_directory == "") and entry or vulcan.path.join(display_directory, entry)
                local entry_is_directory = vulcan.fs.is_dir(full_path)
                if not should_ignore_entry(full_path, entry, entry_is_directory, active_ignore_rules, ignore_enabled) then
                    if entry_is_directory then
                        if recursive then
                            walk(full_path, display_path, active_ignore_rules)
                        end
                    else
                        if matches_extension_filter(entry, extension_filter) then
                            local file_item = build_file_item(full_path, display_path)
                            if file_item then
                                table.insert(collected, file_item)
                            end
                        end
                    end
                end
            end
        end
    end

    walk(scan_root, "", {})
    return collected, "directory", nil
end

--[[
聚合多个输入路径的扫描结果，并按绝对路径去重，确保目录与文件路径重叠时不会重复输出同一文件。
Aggregate scan results from multiple input paths and deduplicate by absolute path so overlapping directory/file inputs do not emit the same file.

参数 / Parameters:
- target_paths(table): 用户传入的目标路径列表。
  List of target paths provided by the caller.
- recursive(boolean): 目录模式下是否递归扫描子目录。
  Whether directory mode should recurse into subdirectories.
- extension_filter(table|nil): 目录模式下使用的扩展名过滤集合。
  Extension filter set used in directory mode.
- ignore_enabled(boolean): 是否启用默认忽略目录与忽略文件规则。
  Whether default ignored directories and ignore-file rules are enabled.

返回 / Returns:
- table|nil: 聚合并去重后的文件列表；触发文件数量上限时返回 nil。
  Aggregated and deduplicated file list, or nil when the matched-file limit is exceeded.
- string|nil: 聚合后的扫描模式，可能为 `file`、`directory` 或 `mixed`。
  Aggregated scan mode, which may be `file`, `directory`, or `mixed`.
- table: 路径级错误列表。
  List of path-level errors.
- table|nil: 致命错误对象，例如命中文件数量上限。
  Fatal error object, such as exceeding the matched-file limit.
]]
local function collect_files(target_paths, recursive, extension_filter, ignore_enabled)
    local collected = {}
    local collected_index = {}
    local errors = {}
    local has_file_mode = false
    local has_directory_mode = false

    for _, target_path in ipairs(target_paths or {}) do
        local path_files, scan_mode, collection_error = collect_files_for_path(target_path, recursive, extension_filter, ignore_enabled)
        if collection_error then
            table.insert(errors, collection_error)
        else
            if scan_mode == "file" then
                has_file_mode = true
            elseif scan_mode == "directory" then
                has_directory_mode = true
            end

            for _, file_info in ipairs(path_files or {}) do
                if not collected_index[file_info.path] then
                    collected_index[file_info.path] = true
                    table.insert(collected, file_info)
                    if #collected > MAX_MATCHED_FILES then
                        return nil, nil, errors, {
                            error = "too_many_matched_files",
                            message = "Matched files exceed 5000. Narrow the path scope or provide a more specific file list.",
                            limit = MAX_MATCHED_FILES,
                            matched_files = #collected,
                        }
                    end
                end
            end
        end
    end

    local scan_mode = nil
    if has_file_mode and has_directory_mode then
        scan_mode = "mixed"
    elseif has_directory_mode then
        scan_mode = "directory"
    elseif has_file_mode then
        scan_mode = "file"
    end

    return collected, scan_mode, errors, nil
end

--[[
判断输入路径集合的模式，明确区分“全文件”“全目录”与非法混用，供入口逻辑做约束校验。
Classify the caller-provided path set as all-files, all-directories, or an invalid mixed set so the entrypoint can enforce path-shape rules.

参数 / Parameters:
- target_paths(table): 用户传入的目标路径列表。
  Caller-provided target path list.

返回 / Returns:
- string|nil: `file`、`directory` 或 `mixed`；若路径不存在则返回 nil。
  `file`, `directory`, or `mixed`; returns nil when any path does not exist.
- table|nil: 结构化错误对象；成功时为 nil。
  Structured error object, or nil on success.
]]
local function classify_target_path_modes(target_paths)
    local has_file = false
    local has_directory = false
    for _, target_path in ipairs(target_paths or {}) do
        local resolved = resolve_scan_path(target_path)
        if not vulcan.fs.exists(resolved) then
            return nil, {
                error = "path_not_found",
                message = "path does not exist",
                path = target_path,
            }
        end
        if vulcan.fs.is_dir(resolved) then
            has_directory = true
        else
            has_file = true
        end
    end
    if has_file and has_directory then
        return "mixed", nil
    end
    if has_directory then
        return "directory", nil
    end
    return "file", nil
end

--[[
校验 Markdown 导出路径，要求为绝对路径，并建议使用 `.md` 扩展名。
Validate the Markdown export path; it must be absolute and should use the `.md` extension.
]]
local function validate_export_md_argument(value)
    if value == nil then
        return nil, nil
    end
    if type(value) ~= "string" or trim(value) == "" then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must be a non-empty absolute file path when provided",
            actual_type = type(value),
        }
    end

    local normalized = trim(value)
    if not is_absolute_path(normalized) then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must be an absolute file path",
            export_md_path = normalized,
        }
    end
    if not normalized:lower():match("%.md$") then
        return nil, {
            error = "invalid_export_md_argument",
            message = "export_md_path must end with .md",
            export_md_path = normalized,
        }
    end
    return normalized, nil
end

--[[
懒加载 LuaFileSystem，用于在导出 JSON/Markdown 前递归创建目录。
Lazily load LuaFileSystem so directories can be created recursively before exporting JSON or Markdown.
]]
local function get_lfs_module()
    if LFS_MODULE ~= nil then
        return LFS_MODULE
    end
    local ok, module = pcall(require, "lfs")
    if ok then
        LFS_MODULE = module
    else
        LFS_MODULE = false
    end
    return LFS_MODULE
end

--[[
获取某个路径的父目录，缺失时返回 nil。
Get the parent directory of a path and return nil when no parent exists.
]]
local function get_parent_directory(path)
    local normalized = tostring(path or ""):gsub("[\\/]+$", "")
    local parent = normalized:match("^(.*)[\\/][^\\/]+$")
    if not parent or parent == normalized then
        return nil
    end
    return parent
end

local function append_path_segment(current, segment)
    if current == "" then
        return segment
    end
    if current == "/" then
        return "/" .. segment
    end
    if current:sub(-1) == "/" then
        return current .. segment
    end
    return current .. "/" .. segment
end

--[[
在 LuaFileSystem 不可用时，回退到宿主 `vulcan.process.exec` 递归创建目录，避免大结果落盘依赖单一 Lua C 模块。
Fall back to host-side `vulcan.process.exec` recursive directory creation when LuaFileSystem is unavailable, so large-result spilling does not depend on a single Lua C module.

参数 / Parameters:
- directory_path(string): 需要创建的目录绝对路径 / Absolute directory path that should be created.

返回 / Returns:
- boolean: 创建成功或目录已存在时返回 true / Returns true when creation succeeds or the directory already exists.
- table|nil: 创建失败时返回结构化错误对象 / Structured error object when creation fails.
]]
local function ensure_directory_via_exec(directory_path)
    local host_exec = get_host_exec_function()
    if type(host_exec) ~= "function" then
        return false, {
            error = "directory_creation_failed",
            message = "neither LuaFileSystem nor host process exec is available for directory creation",
            path = directory_path,
        }
    end

    local os_info = vulcan.os.info()
    local request
    if os_info and os_info.os == "windows" then
        request = {
            program = "powershell.exe",
            args = {
                "-NoProfile",
                "-Command",
                string.format("New-Item -ItemType Directory -Force -Path '%s' | Out-Null", tostring(directory_path):gsub("'", "''")),
            },
            timeout_ms = 10000,
        }
    else
        request = {
            program = "mkdir",
            args = { "-p", directory_path },
            timeout_ms = 10000,
        }
    end

    local ok, result = pcall(host_exec, request)
    if not ok or type(result) ~= "table" then
        return false, {
            error = "directory_creation_failed",
            message = ok and "unexpected exec result" or tostring(result),
            path = directory_path,
        }
    end
    if result.error or result.timed_out or result.success == false then
        return false, {
            error = "directory_creation_failed",
            message = trim(result.error or result.stderr or "mkdir failed"),
            path = directory_path,
        }
    end
    if vulcan.fs.exists(directory_path) and vulcan.fs.is_dir(directory_path) then
        return true, nil
    end
    return false, {
        error = "directory_creation_failed",
        message = "directory was not created",
        path = directory_path,
    }
end

--[[
递归创建目录，供工作目录缓存和 Markdown 导出复用。
Create directories recursively so workdir-based cache dumps and Markdown exports can share the same helper.
]]
local function ensure_directory(directory_path)
    local normalized = trim(directory_path or "")
    if normalized == "" then
        return false, {
            error = "directory_creation_failed",
            message = "directory path is empty",
        }
    end
    if vulcan.fs.exists(normalized) then
        if vulcan.fs.is_dir(normalized) then
            return true, nil
        end
        return false, {
            error = "directory_creation_failed",
            message = "target path already exists as a file",
            path = normalized,
        }
    end

    local lfs = get_lfs_module()
    if not lfs then
        return ensure_directory_via_exec(normalized)
    end

    local path_text = tostring(normalized):gsub("\\", "/")
    local prefix = ""
    if path_text:match("^%a:/") then
        prefix = path_text:sub(1, 3)
        path_text = path_text:sub(4)
    elseif starts_with(path_text, "/") then
        prefix = "/"
        path_text = path_text:sub(2)
    end

    local current = prefix
    for segment in path_text:gmatch("[^/]+") do
        current = append_path_segment(current, segment)
        if not vulcan.fs.exists(current) then
            local ok, mkdir_error = lfs.mkdir(current)
            if not ok then
                return false, {
                    error = "directory_creation_failed",
                    message = tostring(mkdir_error or "mkdir failed"),
                    path = current,
                }
            end
        elseif not vulcan.fs.is_dir(current) then
            return false, {
                error = "directory_creation_failed",
                message = "path exists but is not a directory",
                path = current,
            }
        end
    end
    return true, nil
end

--[[
确保输出文件的父目录存在，并把文本内容写入目标文件。
Ensure the parent directory exists and then write the text content to the target file.
]]
local function shallow_copy_object(source)
    local copied = {}
    for key, value in pairs(source or {}) do
        copied[key] = value
    end
    return copied
end

local function render_error_lines(errors)
    local lines = {}
    for _, item in ipairs(errors or {}) do
        if type(item) == "string" then
            table.insert(lines, "- " .. item)
        elseif type(item) == "table" then
            if item.group then
                table.insert(lines, "- Group: " .. tostring(item.group))
            end
            for _, diagnostic in ipairs(item.diagnostics or {}) do
                table.insert(lines, "  - " .. tostring(diagnostic))
            end
        end
    end
    return lines
end

--[[
把 `codekit-ast-detail` 的扫描结果渲染为 Markdown 纯文本，便于 AI 直接阅读并继续选择下一步文件操作。
Render the `codekit-ast-detail` scan result as plain Markdown text so the AI can read it directly and choose the next file-level action.
]]
local function build_ast_detail_text(result)
    local lines = {
        "# AST DETAIL SUMMARY",
        string.format(
            "- files_scanned: %d | files_with_symbols: %d | items_found: %d | errors: %d",
            result.files_scanned or 0,
            result.files_with_symbols or 0,
            result.items_found or 0,
            #(result.errors or {})
        ),
    }

    local error_lines = render_error_lines(result.errors)
    if #error_lines > 0 then
        table.insert(lines, "")
        table.insert(lines, "## ERRORS")
        table.insert(lines, "")
        for _, line in ipairs(error_lines) do
            table.insert(lines, line)
        end
    end

    for index, file_result in ipairs(result.files or {}) do
        if index > 1 or #error_lines > 0 then
            table.insert(lines, "")
        end
        table.insert(
            lines,
            string.format(
                "[%s Lines:%d Symbols:%d]",
                tostring(file_result.file or "unknown"),
                tonumber(file_result.lines) or 0,
                tonumber(file_result.symbol_count) or 0
            )
        )
        if trim(file_result.content or "") == "" then
            table.insert(lines, "> No AST symbols found in this file.")
        else
            table.insert(lines, tostring(file_result.content))
        end
    end

    return table.concat(lines, "\n")
end

--[[
完成 AST detail 正文输出；超限策略不再由 Lua 决定，而是交给 MCP 宿主统一处理。
Finalize the AST detail body; overflow strategy is no longer decided by Lua and is delegated to the MCP host.
]]
local function finalize_ast_detail_content(markdown_text, summary_lines)
    return tostring(markdown_text or ""), vulcan.runtime.overflow_type.page
end

--[[
把结构化错误对象编码成稳定文本，确保工具入口最终始终返回 plain string。
Encode one structured error object into stable text so the public tool entry always returns a plain string.
]]
local function encode_codekit_error_payload(error_payload)
    if type(error_payload) == "string" then
        return error_payload, "text"
    end

    local ok, encoded = pcall(vulcan.json.encode, error_payload)
    if ok and type(encoded) == "string" and encoded ~= "" then
        return encoded, "json"
    end

    return tostring(error_payload), "text"
end

--[[
把当前入口的错误结果统一渲染成 Markdown 字符串，避免直接返回 table。
Render one Markdown string for current entry errors so the tool never returns a raw table.
]]
local function render_codekit_error_markdown(tool_title, error_payload)
    local payload_text, payload_language = encode_codekit_error_payload(error_payload)
    return table.concat({
        "# " .. tostring(tool_title or "CodeKit Error"),
        "",
        "## Status",
        "FAILED",
        "",
        "## Error",
        "```" .. tostring(payload_language or "text"),
        payload_text,
        "```",
    }, "\n")
end

-- 文件读取与 capture 提取 / Cache file content and decode ast-grep captures.
local function read_file_state(file_path)
    if FILE_CACHE[file_path] then
        return FILE_CACHE[file_path]
    end

    local ok, content = pcall(vulcan.fs.read, file_path)
    if not ok then
        return nil, tostring(content)
    end

    local state = {
        raw = content or "",
        lines = split_lines(content or ""),
    }
    state.line_count = #state.lines
    FILE_CACHE[file_path] = state
    return state
end

--[[
读取单个文件的总行数，优先复用文件状态缓存；若读取失败，则返回 0，避免影响整体结构输出。
Read the total line count for a single file, reusing the file-state cache whenever possible; return 0 on read failure so the overall structure output stays stable.

参数 / Parameters:
- file_path(string): 目标文件完整路径 / Full path of the target file.

返回 / Returns:
- number: 文件总行数；读取失败时返回 0。
  Total number of lines in the file, or 0 when the file cannot be read.
]]
local function get_file_line_count(file_path)
    local file_state = read_file_state(file_path)
    if not file_state then
        return 0
    end
    return file_state.line_count or 0
end

local function extract_text_by_range(file_state, range)
    if not file_state or not range or not range.byteOffset then
        return ""
    end

    local start_offset = (range.byteOffset.start or 0) + 1
    local end_offset = range.byteOffset["end"] or 0
    if end_offset < start_offset then
        return ""
    end

    return file_state.raw:sub(start_offset, end_offset)
end

local function extract_single_capture(match, capture_name)
    local meta = match.metaVariables or {}
    local single = meta.single or {}
    local item = single[capture_name]
    if not item then
        return ""
    end
    return trim(item.text or "")
end

local function extract_multi_capture(match, file_state, capture_name)
    local meta = match.metaVariables or {}
    local multi = meta.multi or {}
    local items = multi[capture_name]
    if not items or not items[1] then
        return "", {}
    end

    local first_item = items[1]
    local last_item = items[#items]
    local combined = ""
    if first_item.range and last_item.range then
        combined = extract_text_by_range(file_state, {
            byteOffset = {
                start = first_item.range.byteOffset.start,
                ["end"] = last_item.range.byteOffset["end"],
            },
        })
    end

    local values = {}
    for _, item in ipairs(items) do
        local text = trim(item.text or "")
        if text ~= "" and not text:match("^[,%[%]%(%){}]+$") then
            table.insert(values, text)
        end
    end
    return trim(combined), values
end

-- 声明头与名称推断 / Extract compact headers and infer symbol names when captures are absent.
local function extract_header_text(symbol_text)
    local lines = split_lines(symbol_text or "")
    local collected = {}

    for index = 1, math.min(#lines, MAX_HEADER_LINES) do
        local current = trim(lines[index])
        if current ~= "" then
            table.insert(collected, current)
            if current:find("{", 1, true) or current:match(":$") or current:find("=>", 1, true) or current:match("%sdo%s*$") then
                break
            end
        end
    end

    local header = normalize_whitespace(table.concat(collected, " "))
    local brace_index = header:find("{", 1, true)
    if brace_index then
        header = trim(header:sub(1, brace_index - 1))
    end
    if header:find("=>", 1, true) then
        header = trim((header:match("^(.-=>)") or header))
    end
    return trim(header)
end

local function infer_name_from_header(symbol_kind, header)
    local patterns_by_kind = {
        class = { "^class%s+([%w_%.:<>]+)", "^data%s+class%s+([%w_%.:<>]+)", "^record%s+([%w_%.:<>]+)" },
        constant = {
            "^export%s+const%s+([%w_$.]+)",
            "^const%s+([%w_$.]+)",
        },
        contract = { "^contract%s+([%w_%.:<>]+)" },
        enum = { "^enum%s+([%w_%.:<>]+)", "^type%s+([%w_%.:<>]+)%s+enum" },
        ["function"] = {
            "^function%s+([%w_%.:]+)%s*%(",
            "^local%s+function%s+([%w_%.:]+)%s*%(",
            "^def%s+([%w_!?]+)%s*%(",
            "^func%s*%b()%s*([%w_]+)%s*%(",
            "^func%s+([%w_]+)%s*%(",
            "^fn%s+([%w_]+)%s*%(",
            "^([%w_%.:]+)%s*=%s*function%s*%(",
            "^([%w_%.:]+)%s*=%s*%b()%s*=>",
            "^([%w_%.:]+)%s*=%s*[%w_]+%s*=>",
            "([%w_]+)%s*%(",
        },
        impl = { "^impl%s+([%w_%.:<>]+)" },
        interface = { "^interface%s+([%w_%.:<>]+)", "^type%s+([%w_%.:<>]+)%s+interface" },
        library = { "^library%s+([%w_%.:<>]+)" },
        method = { "^function%s+([%w_%.:]+)%s*%(", "^def%s+([%w_!?]+)%s*%(", "^func%s*%b()%s*([%w_]+)%s*%(", "^fn%s+([%w_]+)%s*%(", "([%w_]+)%s*%(" },
        module = { "^module%s+([%w_%.:<>]+)", "^defmodule%s+([%w_%.:<>]+)" },
        namespace = { "^namespace%s+([%w_%.:<>]+)" },
        object = { "^object%s+([%w_%.:<>]+)" },
        property = { '^"([^"]+)"%s*:', "^([%w_%-%.:]+)%s*:" },
        protocol = { "^protocol%s+([%w_%.:<>]+)" },
        tag = { "^<([%w:_-]+)" },
        struct = { "^struct%s+([%w_%.:<>]+)", "^type%s+([%w_%.:<>]+)%s+struct" },
        attribute = { "^([%w_%-%.:]+)%s*=" },
        trait = { "^trait%s+([%w_%.:<>]+)" },
        type = { "^type%s+([%w_%.:<>]+)%s*=" },
        block = { "^([%w_%-%.:]+)" },
    }

    for _, pattern in ipairs(patterns_by_kind[symbol_kind] or {}) do
        local name = header:match(pattern)
        if name and name ~= "" then
            return name
        end
    end
    return ""
end

-- 参数与备注提取 / Extract parameters, receivers, and nearby comment blocks.
local function infer_parameters(header, symbol_kind)
    local parameter_text = ""
    local receiver_text = ""
    local segments = {}
    local depth = 0
    local start_index = nil

    for index = 1, #header do
        local char = header:sub(index, index)
        if char == "(" then
            if depth == 0 then
                start_index = index
            end
            depth = depth + 1
        elseif char == ")" and depth > 0 then
            depth = depth - 1
            if depth == 0 and start_index then
                table.insert(segments, { start_pos = start_index, end_pos = index })
                start_index = nil
            end
        end
    end

    if (symbol_kind == "function" or symbol_kind == "method") and #segments > 0 then
        if starts_with(header, "func ") and #segments >= 2 then
            receiver_text = header:sub(segments[1].start_pos + 1, segments[1].end_pos - 1)
            parameter_text = header:sub(segments[2].start_pos + 1, segments[2].end_pos - 1)
        else
            local segment = segments[#segments]
            parameter_text = header:sub(segment.start_pos + 1, segment.end_pos - 1)
        end
    elseif symbol_kind == "function" and header:find("=", 1, true) then
        local name = infer_name_from_header(symbol_kind, header)
        if name ~= "" then
            local pattern = "^" .. name:gsub("([^%w])", "%%%1") .. "%s+(.+)%s*="
            parameter_text = trim(header:match(pattern) or "")
        end
    end

    local parameters = {}
    if parameter_text ~= "" then
        for chunk in parameter_text:gmatch("[^,]+") do
            local value = trim(chunk)
            if value ~= "" then
                table.insert(parameters, value)
            end
        end
    end

    return trim(parameter_text), parameters, trim(receiver_text)
end

local function get_sorted_comment_prefixes(comment_config)
    local prefixes = clone_array((comment_config and comment_config.line_prefixes) or {})
    table.sort(prefixes, function(left, right)
        return #tostring(left or "") > #tostring(right or "")
    end)
    return prefixes
end

local function clean_comment_line(line, comment_config)
    local current = trim(line)
    for _, prefix in ipairs(get_sorted_comment_prefixes(comment_config)) do
        if starts_with(current, prefix) then
            return trim(current:sub(#prefix + 1))
        end
    end
    return current
end

--[[
移除注释行里常见的文档装饰字符，例如块注释中的 `*` 前缀。
Strip common decorative markers from a comment line, such as the leading `*` used in block comments.
]]
local function strip_comment_decorations(line)
    local current = trim(line)
    current = current:gsub("^%*+%s*", "")
    current = current:gsub("^%-%-+%s*", "")
    return trim(current)
end

--[[
判断一行备注是否只是分隔线、区域边界或其它无语义装饰文本。
Determine whether a comment line is merely a separator, section boundary, or other non-semantic decoration.
]]
local function is_separator_comment_line(line)
    local current = strip_comment_decorations(line)
    if current == "" then
        return true
    end
    local meaningful = current
        :gsub("[%s%-%=%*_/\\|#~`%.:,;>%<%+%(%)[%]{}]+", "")
        :gsub("·", "")
        :gsub("•", "")
    return meaningful == ""
end

--[[
判断备注行是否属于参数、返回值等结构化标签说明，而非核心摘要内容。
Determine whether a comment line is a structured label such as params or returns instead of core summary content.
]]
local function is_comment_metadata_line(line)
    local current = strip_comment_decorations(line):lower()
    if current == "" then
        return false
    end
    if starts_with(current, "@param")
        or starts_with(current, "@return")
        or starts_with(current, "@returns")
        or starts_with(current, "@throws")
        or starts_with(current, "@example")
        or starts_with(current, "param ")
        or starts_with(current, "params ")
        or starts_with(current, "return ")
        or starts_with(current, "returns ")
        or starts_with(current, "parameters:")
        or starts_with(current, "returns:")
        or starts_with(current, "parameters /")
        or starts_with(current, "returns /")
    then
        return true
    end
    if starts_with(current, "参数")
        or starts_with(current, "返回")
        or starts_with(current, "返回值")
        or starts_with(current, "参数 /")
        or starts_with(current, "返回 /")
        or starts_with(current, "返回值 /")
    then
        return true
    end
    return false
end

--[[
按 UTF-8 字节边界截断字符串，避免中文字符被截成半个字节序列。
Truncate text on UTF-8 byte boundaries so multi-byte characters are not split mid-sequence.
]]
local function utf8_truncate_by_bytes(text, max_bytes)
    local source = tostring(text or "")
    local limit = tonumber(max_bytes) or #source
    if #source <= limit then
        return source, false
    end

    local index = 1
    local used = 0
    local parts = {}
    while index <= #source do
        local byte = source:byte(index)
        local char_length = 1
        if byte >= 240 then
            char_length = 4
        elseif byte >= 224 then
            char_length = 3
        elseif byte >= 192 then
            char_length = 2
        end
        if used + char_length > limit then
            break
        end
        table.insert(parts, source:sub(index, index + char_length - 1))
        used = used + char_length
        index = index + char_length
    end
    return table.concat(parts), true
end

--[[
把原始多行注释压缩为适合 AST 备注展示的单行摘要。
Compress raw multi-line comments into a single-line summary suitable for AST note display.
]]
local function summarize_comment_text(raw_comment)
    local source = tostring(raw_comment or "")
    if trim(source) == "" then
        return ""
    end

    local effective_lines = {}
    for _, raw_line in ipairs(split_lines(source)) do
        local cleaned = strip_comment_decorations(raw_line)
        if cleaned ~= ""
            and not is_separator_comment_line(cleaned)
            and not is_comment_metadata_line(cleaned)
        then
            table.insert(effective_lines, cleaned)
        end
    end

    if #effective_lines == 0 then
        return ""
    end

    local merged = normalize_whitespace(table.concat(effective_lines, " "))
    local summary_limit = MAX_COMMENT_SUMMARY_BYTES
    if summary_limit > 3 and #merged > summary_limit then
        summary_limit = summary_limit - 3
    end
    local truncated, was_truncated = utf8_truncate_by_bytes(merged, summary_limit)
    if was_truncated and truncated ~= "" then
        return truncated .. "..."
    end
    return truncated
end

local function extract_leading_comment(file_state, start_line, comment_config)
    if not comment_config or start_line <= 1 then
        return ""
    end

    local index = start_line - 1
    while index >= 1 and trim(file_state.lines[index] or "") == "" do
        index = index - 1
    end
    if index < 1 then
        return ""
    end

    local line_comments = {}
    local cursor = index
    while cursor >= 1 do
        local current = trim(file_state.lines[cursor] or "")
        local matched = false
        for _, prefix in ipairs(get_sorted_comment_prefixes(comment_config)) do
            if starts_with(current, prefix) then
                table.insert(line_comments, 1, clean_comment_line(current, comment_config))
                matched = true
                cursor = cursor - 1
                break
            end
        end
        if not matched then
            break
        end
    end

    if #line_comments > 0 then
        return trim(table.concat(line_comments, "\n"))
    end

    local current_line = trim(file_state.lines[index] or "")
    for _, pair in ipairs(comment_config.block_pairs or {}) do
        local start_token = pair[1]
        local end_token = pair[2]
        if current_line:find(end_token, 1, true) then
            local collected = {}
            local block_cursor = index
            while block_cursor >= 1 do
                local block_line = file_state.lines[block_cursor] or ""
                table.insert(collected, 1, block_line)
                if block_line:find(start_token, 1, true) then
                    local normalized_lines = {}
                    for _, collected_line in ipairs(collected) do
                        local text = replace_literal(collected_line, start_token, "")
                        text = replace_literal(text, end_token, "")
                        table.insert(normalized_lines, trim(text))
                    end
                    return trim(table.concat(normalized_lines, "\n"))
                end
                block_cursor = block_cursor - 1
            end
        end
    end

    return ""
end

local function extract_docstring(file_state, start_line, end_line, comment_config)
    local tokens = comment_config and comment_config.docstring_tokens or nil
    if not tokens or start_line >= end_line then
        return ""
    end

    local cursor = start_line + 1
    while cursor <= end_line and trim(file_state.lines[cursor] or "") == "" do
        cursor = cursor + 1
    end

    local first_line = trim(file_state.lines[cursor] or "")
    for _, token in ipairs(tokens) do
        if starts_with(first_line, token) then
            local parts = {}
            local body = first_line:sub(#token + 1)
                if body:find(token, 1, true) then
                    table.insert(parts, trim(replace_literal(body, token, "")))
                    return trim(table.concat(parts, "\n"))
                end
            if body ~= "" then
                table.insert(parts, body)
            end
            cursor = cursor + 1
            while cursor <= end_line do
                local line = file_state.lines[cursor] or ""
                    if line:find(token, 1, true) then
                        table.insert(parts, trim(replace_literal(line, token, "")))
                        return trim(table.concat(parts, "\n"))
                    end
                table.insert(parts, line)
                cursor = cursor + 1
            end
        end
    end

    return ""
end

-- 结构归一化与建树 / Normalize matches into canonical symbols and assemble a nested tree.
local function normalize_symbol(match, language_key)
    if not match or not match.file then
        return nil
    end

    local file_state = read_file_state(match.file)
    if not file_state then
        return nil
    end

    local metadata = match.metadata or {}
    local kind = metadata.symbol_kind or "unknown"
    local comment_config = LANGUAGE_REGISTRY[language_key] and LANGUAGE_REGISTRY[language_key].comments or nil
    local start_line = ((match.range and match.range.start and match.range.start.line) or 0) + 1
    local end_line = ((match.range and match.range["end"] and match.range["end"].line) or 0) + 1
    local symbol_text = match.text or ""
    local header = extract_header_text(symbol_text)
    if kind == "constant" then
        header = trim((header or ""):gsub("%s*=%s*$", ""))
    end
    local capture_name = metadata.name_capture and extract_single_capture(match, metadata.name_capture) or ""
    local name = capture_name ~= "" and capture_name or infer_name_from_header(kind, header)
    name = trim((name or ""):gsub("[:{%s]+$", ""))

    local params_text = ""
    local params = {}
    local receiver = ""
    if metadata.params_capture then
        params_text, params = extract_multi_capture(match, file_state, metadata.params_capture)
    else
        params_text, params, receiver = infer_parameters(header, kind)
    end
    if metadata.receiver_capture then
        receiver = extract_single_capture(match, metadata.receiver_capture)
    end

    local detail = metadata.detail_capture and extract_single_capture(match, metadata.detail_capture) or ""
    local comment = extract_leading_comment(file_state, start_line, comment_config)
    if comment == "" then
        comment = extract_docstring(file_state, start_line, end_line, comment_config)
    end
    comment = summarize_comment_text(comment)

    return {
        kind = kind,
        name = name ~= "" and name or "unknown",
        language = language_key,
        file = match.file,
        header = header,
        signature = header,
        params_text = params_text,
        params = params,
        receiver = receiver,
        detail = detail,
        comment = comment,
        start_line = start_line,
        end_line = end_line,
        container = metadata.container == true or CONTAINER_KINDS[kind] == true,
        children = {},
    }
end

local function contains_symbol(parent_symbol, child_symbol)
    return parent_symbol
        and child_symbol
        and parent_symbol.file == child_symbol.file
        and parent_symbol.start_line <= child_symbol.start_line
        and parent_symbol.end_line >= child_symbol.end_line
        and (parent_symbol.start_line ~= child_symbol.start_line or parent_symbol.end_line ~= child_symbol.end_line)
end

local function build_symbol_tree(symbols)
    table.sort(symbols, function(left, right)
        if left.start_line ~= right.start_line then
            return left.start_line < right.start_line
        end
        if left.end_line ~= right.end_line then
            return left.end_line > right.end_line
        end
        if left.container ~= right.container then
            return left.container and not right.container
        end
        return left.kind < right.kind
    end)

    local roots = {}
    local stack = {}

    for _, symbol in ipairs(symbols) do
        while #stack > 0 and not contains_symbol(stack[#stack], symbol) do
            table.remove(stack)
        end

        local parent = stack[#stack]
        if parent then
            symbol.parent_name = parent.name
            symbol.parent_kind = parent.kind
            if symbol.kind == "function" and METHOD_PARENT_KINDS[parent.kind] then
                symbol.kind = "method"
            end
            table.insert(parent.children, symbol)
        else
            table.insert(roots, symbol)
        end

        if symbol.container then
            table.insert(stack, symbol)
        end
    end

    return roots
end

local function deduplicate_symbols(symbols)
    local unique = {}
    local seen = {}
    for _, symbol in ipairs(symbols) do
        local key = table.concat({ symbol.file, symbol.kind, symbol.start_line, symbol.end_line, symbol.name }, "::")
        if not seen[key] then
            seen[key] = true
            table.insert(unique, symbol)
        end
    end
    return unique
end

--[[
将起止行号格式化为统一的 `Lx-y` 文本，便于 AI 与人类直接定位结构范围。
Format start and end lines into a unified `Lx-y` label so both AI agents and humans can locate the symbol range directly.

参数 / Parameters:
- start_line(number): 起始行号 / 1-based start line.
- end_line(number): 结束行号 / 1-based end line.

返回 / Returns:
- string: 结构范围标签；若结束行缺失或早于起始行，则退化为单行标签。
  Range label for the symbol; falls back to a single-line label when the end line is missing or invalid.
]]
local function format_line_span(start_line, end_line)
    local normalized_start = tonumber(start_line) or 0
    local normalized_end = tonumber(end_line) or normalized_start
    if normalized_end < normalized_start then
        normalized_end = normalized_start
    end
    if normalized_start <= 0 then
        return "L?"
    end
    if normalized_start == normalized_end then
        return string.format("L%d", normalized_start)
    end
    return string.format("L%d-%d", normalized_start, normalized_end)
end

--[[
为单个结构节点生成一行可读摘要，优先使用原始签名，缺失时再回退到 `kind + name`。
Build a readable one-line summary for a single symbol, preferring its original signature and falling back to `kind + name` when needed.

参数 / Parameters:
- symbol(table): 已归一化的结构节点，包含 kind、name、signature 与行号信息。
  Normalized symbol record containing kind, name, signature, and line information.
- depth(number): 当前节点在树中的层级，决定缩进与列表前缀。
  Current tree depth used to determine indentation and bullet prefix.

返回 / Returns:
- string: 适合直接展示的单行结构摘要。
  Single-line structure summary ready for direct display.
]]
local function format_symbol_line(symbol, depth)
    local signature = trim(symbol and symbol.signature or "")
    local display_text = signature ~= "" and signature or trim(string.format("%s %s", symbol and symbol.kind or "unknown", symbol and symbol.name or "unknown"))
    local prefix = depth > 0 and (string.rep("  ", depth - 1) .. "- ") or ""
    return string.format("%s%s %s", prefix, display_text, format_line_span(symbol and symbol.start_line, symbol and symbol.end_line))
end

--[[
把结构节点备注渲染为“无行号”的补充说明行，确保备注属于当前节点而不是相邻节点。
Render a symbol comment as a no-line-number supplementary line so the note stays attached to the current node instead of a neighboring one.

参数 / Parameters:
- symbol(table): 已归一化的结构节点，包含可选的 `comment` 字段。
  Normalized symbol record that may contain a `comment` field.
- depth(number): 当前节点层级，用于决定备注行的缩进深度。
  Current node depth used to determine indentation for the comment line.

返回 / Returns:
- string|nil: 备注展示文本；当节点没有备注时返回 nil。
  Rendered comment text, or nil when the symbol has no comment.
]]
local function format_symbol_comment_line(symbol, depth)
    local comment_text = trim(symbol and symbol.comment or "")
    if comment_text == "" then
        return nil
    end

    local indent = string.rep("  ", math.max((depth or 0), 0))
    return string.format("%snote: %s", indent, comment_text)
end

--[[
递归遍历结构树并展开为纯文本列表，输出目标是“无需解析 JSON 也能直接理解”的结构摘要。
Recursively flatten the symbol tree into a plain-text outline so the structure can be understood directly without parsing JSON nodes.

参数 / Parameters:
- symbols(table): 当前层级的结构节点列表。
  Symbol list for the current tree level.
- depth(number): 当前层级深度，用于控制缩进。
  Current nesting depth used to control indentation.
- lines(table): 输出行缓冲区，函数会按顺序向其中追加文本行。
  Output line buffer that will receive formatted lines in order.
- include_comments(boolean): 是否为当前输出附加备注说明。
  Whether comments should be appended to the current rendered outline.

返回 / Returns:
- nil: 结果通过 `lines` 参数原地累积。
  Nil; results are accumulated into the `lines` buffer in place.
]]
local function append_symbol_outline(symbols, depth, lines, include_comments)
    for _, symbol in ipairs(symbols or {}) do
        table.insert(lines, format_symbol_line(symbol, depth))
        if include_comments then
            local comment_line = format_symbol_comment_line(symbol, depth)
            if comment_line then
                table.insert(lines, comment_line)
            end
        end
        if symbol.children and #symbol.children > 0 then
            append_symbol_outline(symbol.children, depth + 1, lines, include_comments)
        end
    end
end

--[[
将单文件的结构树渲染为最终 `content` 文本字段，作为对外暴露的轻量结构视图。
Render a file-level symbol tree into the final `content` text field, which serves as the lightweight public structure view.

参数 / Parameters:
- symbols(table): 单个文件对应的结构树根节点列表。
  Root symbol list representing a single file's structure tree.
- include_comments(boolean): 是否在结构项下附带备注文本。
  Whether comment text should be rendered beneath each structural item.

返回 / Returns:
- string: 按层级缩进组织的纯文本结构摘要；若无结构则返回空字符串。
  Plain-text structure summary with indentation by nesting level; returns an empty string when no symbols are present.
]]
local function build_file_content(symbols, include_comments)
    local lines = {}
    append_symbol_outline(symbols or {}, 0, lines, include_comments == true)
    return table.concat(lines, "\n")
end

--[[
校验技能入口的路径参数，支持单路径或多行路径列表；每一行都必须是非空字符串。
Validate the skill entry path argument. It supports either a single path or a multi-line path list, and each line must be a non-empty string.

参数 / Parameters:
- value(any): 用户传入的 `path` 参数。
  The user-provided `path` argument.

返回 / Returns:
- table|nil: 规范化后的路径列表；若缺省则返回仅包含 "." 的数组。
  Normalized path list, or an array containing only "." when omitted.
- table|nil: 参数非法时返回结构化错误对象；成功时为 nil。
  A structured error object when the argument is invalid; otherwise nil.
]]
local function validate_path_argument(value)
    if value == nil then
        return { "." }, nil
    end
    if type(value) ~= "string" then
        return nil, {
            error = "invalid_path_argument",
            message = "path must be a non-empty string",
            actual_type = type(value),
        }
    end

    local normalized_paths = {}
    for _, line in ipairs(split_lines(value)) do
        local normalized_line = trim(line)
        if normalized_line ~= "" then
            table.insert(normalized_paths, normalized_line)
        end
    end

    if #normalized_paths == 0 then
        return nil, {
            error = "invalid_path_argument",
            message = "path must be a non-empty string",
            actual_type = "string",
        }
    end
    return normalized_paths, nil
end

--[[
校验递归扫描开关，要求显式布尔值，避免把 0/"true" 一类值误当成配置。
Validate the recursive scan flag and require an explicit boolean to avoid treating values like 0/"true" as configuration.

参数 / Parameters:
- value(any): 用户传入的 `recursive` 参数。
  The user-provided `recursive` argument.

返回 / Returns:
- boolean: 规范化后的递归开关，未传时默认为 false。
  Normalized recursive flag, defaulting to false when omitted.
- table|nil: 参数非法时返回结构化错误对象；成功时为 nil。
  A structured error object when the argument is invalid; otherwise nil.
]]
local function validate_recursive_argument(value)
    if value == nil then
        return false, nil
    end
    if type(value) ~= "boolean" then
        return nil, {
            error = "invalid_recursive_argument",
            message = "recursive must be a boolean when provided",
            actual_type = type(value),
        }
    end
    return value, nil
end

--[[
校验 `noignore` 开关，默认仍启用忽略目录与忽略文件规则，只有显式传入 `true` 时才关闭忽略。
Validate the `noignore` toggle. Ignore directories and ignore-file rules remain enabled by default and are disabled only when `true` is explicitly provided.

参数 / Parameters:
- value(any): 用户传入的 `noignore` 参数。
  The user-provided `noignore` argument.

返回 / Returns:
- boolean: 规范化后的“忽略是否启用”开关，未传时默认为 true。
  Normalized ignore-enabled flag, defaulting to true when omitted.
- table|nil: 参数非法时返回结构化错误对象；成功时为 nil。
  A structured error object when the argument is invalid; otherwise nil.
]]
local function validate_noignore_argument(value)
    if value == nil then
        return true, nil
    end
    if type(value) ~= "boolean" then
        return nil, {
            error = "invalid_noignore_argument",
            message = "noignore must be a boolean when provided",
            actual_type = type(value),
        }
    end
    return not value, nil
end

--[[
校验备注输出开关，默认关闭；只有显式传入 `true` 时才开启备注渲染。
Validate the comment-output toggle. Comments are disabled by default and are enabled only when `true` is explicitly provided.

参数 / Parameters:
- value(any): 用户传入的备注控制参数。
  User-provided comment control argument.

返回 / Returns:
- boolean: 规范化后的备注开关，未传时默认为 true。
  Normalized comment toggle, defaulting to true when omitted.
- table|nil: 参数非法时返回结构化错误对象；成功时为 nil。
  A structured error object when the argument is invalid; otherwise nil.
]]
local function validate_comment_argument(value)
    if value == nil then
        return false, nil
    end
    if type(value) ~= "boolean" then
        return nil, {
            error = "invalid_comment_argument",
            message = "comment must be a boolean when provided",
            actual_type = type(value),
        }
    end
    return value, nil
end

--[[
校验 `codekit-ast-detail` 的 `paths` 参数；当前仅支持显式文件列表，且最多 20 个文件。
Validate the `paths` argument for `codekit-ast-detail`; only explicit file lists are supported and the request is capped at 20 files.
]]
local function validate_detail_paths_argument(value)
    if type(value) ~= "string" then
        return nil, {
            error = "invalid_paths_argument",
            message = "paths must be a non-empty string containing one or more file paths",
            actual_type = type(value),
        }
    end

    local normalized_paths = {}
    for _, line in ipairs(split_lines(value)) do
        local normalized_line = trim(line)
        if normalized_line ~= "" then
            table.insert(normalized_paths, normalized_line)
        end
    end

    if #normalized_paths == 0 then
        return nil, {
            error = "invalid_paths_argument",
            message = "paths must contain at least one non-empty file path",
            actual_type = "string",
        }
    end
    if #normalized_paths > MAX_EXPLICIT_FILES then
        return nil, {
            error = "too_many_explicit_files",
            message = "codekit-ast-detail supports at most 20 explicit files per request",
            limit = MAX_EXPLICIT_FILES,
            requested_files = #normalized_paths,
        }
    end

    return normalized_paths, nil
end

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
