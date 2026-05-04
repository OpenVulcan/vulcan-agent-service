--[[
Run or analyze one validation command and compress build/test/check/lint output into AI-native diagnostics.
执行或分析一个验证命令，并将 build/test/check/lint 输出压缩成 AI 原生诊断。
]]

-- Default timeout keeps common project validation bounded without forcing callers to tune every run.
-- 默认超时让常见项目验证保持有界，避免调用方每次都必须调参。
local DEFAULT_TIMEOUT_MS = 120000

-- Maximum timeout prevents one tool call from becoming an unbounded long-running process.
-- 最大超时防止单次工具调用变成无界长时间进程。
local MAX_TIMEOUT_MS = 600000

-- Default root diagnostic count keeps the model-facing result compact.
-- 默认根诊断数量让面向模型的结果保持紧凑。
local DEFAULT_MAX_DIAGNOSTICS = 8

-- Maximum root diagnostic count protects the tool result budget.
-- 最大根诊断数量保护工具结果预算。
local MAX_DIAGNOSTICS = 20

-- Default raw evidence length gives enough local context without returning full logs.
-- 默认原始证据长度提供足够局部上下文，同时不返回完整日志。
local DEFAULT_MAX_EVIDENCE_CHARS = 240

-- Maximum raw evidence length prevents individual diagnostics from dominating the response.
-- 最大原始证据长度防止单条诊断占据过多响应空间。
local MAX_EVIDENCE_CHARS = 1000

-- Accepted validation phases describe the intent of the command instead of the language.
-- 允许的验证阶段描述命令意图，而不是绑定具体语言。
local VALID_PHASES = {
    auto = true,
    build = true,
    test = true,
    check = true,
    lint = true,
    typecheck = true,
}

-- Known adapter keys are the only parser modules that may be loaded from disk.
-- 已知适配器键是唯一允许从磁盘加载的解析器模块。
local KNOWN_ADAPTER_KEYS = {
    cargo = true,
    generic = true,
    ["go-test"] = true,
    ["js-test"] = true,
    node = true,
    pytest = true,
    python = true,
    tsc = true,
}

-- Profile cache keeps validation routing modular while avoiding repeated file loads.
-- profile 缓存让验证路由保持模块化，同时避免重复加载文件。
local PROFILE_CACHE = nil

-- Detector cache keeps log detection modular while avoiding repeated file loads.
-- detector 缓存让日志检测保持模块化，同时避免重复加载文件。
local DETECTOR_CACHE = nil

-- Adapter module cache avoids reloading parser files within the same Lua VM.
-- 适配器模块缓存避免同一个 Lua VM 内重复加载解析器文件。
local ADAPTER_CACHE = {}

-- Blocked shell-like programs keep TestKit from becoming a general-purpose shell runner.
-- 被拦截的类 shell 程序防止 TestKit 变成通用 shell 执行器。
local BLOCKED_PROGRAMS = {
    ["bash"] = true,
    ["cmd"] = true,
    ["fish"] = true,
    ["nu"] = true,
    ["powershell"] = true,
    ["pwsh"] = true,
    ["sh"] = true,
    ["wsl"] = true,
    ["zsh"] = true,
}

--- Trim surrounding whitespace from one value after converting it to text.
--- 去除一个值转为文本后的首尾空白。
--- @param value any Text-like value.
--- @return string
local function trim(value)
    return (tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--- Check whether one value is missing or whitespace-only text.
--- 判断一个值是否缺失或仅包含空白文本。
--- @param value any Candidate value.
--- @return boolean
local function is_blank_string(value)
    return type(value) ~= "string" or value:match("^%s*$") ~= nil
end

--- Convert text to lowercase in a nil-safe way.
--- 以空值安全的方式将文本转换为小写。
--- @param value any Candidate value.
--- @return string
local function lower_text(value)
    return trim(value):lower()
end

--- Check whether one adapter key is a safe module stem.
--- 检查适配器键是否是安全的模块文件名主体。
--- @param value any Candidate adapter key.
--- @return boolean
local function is_safe_adapter_key(value)
    local text = lower_text(value)
    return text ~= "" and text:match("^[%w_-]+$") ~= nil
end

--- Check whether text starts with one prefix.
--- 判断文本是否以指定前缀开头。
--- @param text string Candidate text.
--- @param prefix string Expected prefix.
--- @return boolean
local function starts_with(text, prefix)
    return tostring(text or ""):sub(1, #prefix) == prefix
end

--- Split text into lines while preserving line order for parsers.
--- 将文本拆分为行并保持顺序，供解析器使用。
--- @param text string Raw text.
--- @return table
local function split_lines(text)
    local lines = {}
    local normalized = tostring(text or ""):gsub("\r\n", "\n"):gsub("\r", "\n")
    for line in (normalized .. "\n"):gmatch("([^\n]*)\n") do
        table.insert(lines, line)
    end
    return lines
end

--- Return the executable basename without platform extension.
--- 返回去除平台扩展名后的可执行文件名。
--- @param program string Program path or name.
--- @return string
local function program_basename(program)
    local normalized = tostring(program or ""):gsub("\\", "/")
    local name = normalized:match("([^/]+)$") or normalized
    return lower_text((name:gsub("%.exe$", "")))
end

--- Check whether a program is a bare executable name instead of a caller-controlled path.
--- 检查程序名是否为裸可执行名称，而不是调用方可控路径。
--- @param value any Candidate program value.
--- @return boolean
local function is_bare_program_name(value)
    local text = trim(value)
    return text ~= "" and not text:find("[/\\]") and not text:find(":", 1, true) and not text:find("%s")
end

--- Resolve the current entry directory for loading sibling adapter modules.
--- 解析当前入口目录，用于加载同级 adapter 模块。
--- @return string
local function get_entry_dir()
    return tostring(vulcan.context.entry_dir or ".")
end

--- Resolve the validation profile directory for modular command guards.
--- 解析模块化命令守卫使用的验证 profile 目录。
--- @return string
local function get_profile_dir()
    return vulcan.path.join(get_entry_dir(), "profiles")
end

--- Resolve the log detector directory for modular output routing.
--- 解析模块化输出路由使用的日志 detector 目录。
--- @return string
local function get_detector_dir()
    return vulcan.path.join(get_entry_dir(), "detectors")
end

--- Load all validation profile modules through the profile middleware index.
--- 通过 profile 中间件索引加载全部验证 profile 模块。
--- @return table, string|nil
local function load_profiles()
    if PROFILE_CACHE ~= nil then
        return PROFILE_CACHE, nil
    end

    local profile_dir = get_profile_dir()
    local index_path = vulcan.path.join(profile_dir, "index.lua")
    local chunk, load_error = loadfile(index_path)
    if not chunk then
        return nil, "failed to load TestKit profile index: " .. tostring(load_error)
    end

    local ok, loader = pcall(chunk)
    if not ok then
        return nil, "failed to initialize TestKit profile index: " .. tostring(loader)
    end
    if type(loader) ~= "function" then
        return nil, "TestKit profile index must return a loader function"
    end

    local load_ok, profiles = pcall(loader, profile_dir)
    if not load_ok then
        return nil, tostring(profiles)
    end

    local by_key = {}
    local by_alias = {}
    for _, profile in ipairs(profiles or {}) do
        by_key[profile.key] = profile
        by_alias[lower_text(profile.key)] = profile
        for _, alias in ipairs(profile.aliases or {}) do
            by_alias[lower_text(alias)] = profile
        end
    end

    PROFILE_CACHE = {
        list = profiles or {},
        by_key = by_key,
        by_alias = by_alias,
    }
    return PROFILE_CACHE, nil
end

--- Resolve one profile by canonical key.
--- 通过规范键解析一个 profile。
--- @param key string Profile key.
--- @return table|nil, string|nil
local function profile_by_key(key)
    local profiles, load_error = load_profiles()
    if load_error then
        return nil, load_error
    end
    return profiles.by_key[key], nil
end

--- Resolve one profile from an executable name or tool hint.
--- 通过可执行文件名或 tool hint 解析一个 profile。
--- @param value string Program name or hint.
--- @return table|nil, string|nil
local function profile_for_alias(value)
    local profiles, load_error = load_profiles()
    if load_error then
        return nil, load_error
    end
    return profiles.by_alias[lower_text(value)], nil
end

--- Return parser adapter keys for one profile and concrete executable.
--- 返回某个 profile 与具体可执行文件对应的解析器适配器键。
--- @param profile table Profile object.
--- @param program string Program basename.
--- @return table
local function profile_adapters(profile, program)
    if profile and type(profile.adapters_for_program) == "function" then
        return profile.adapters_for_program(program)
    end
    if profile and type(profile.adapters) == "table" then
        return profile.adapters
    end
    return { "generic" }
end

--- Return the display tool name for one profile and concrete executable.
--- 返回某个 profile 与具体可执行文件对应的展示工具名。
--- @param profile table Profile object.
--- @param program string Program basename.
--- @return string
local function profile_tool_name(profile, program)
    if profile and type(profile.display_for_program) == "function" then
        local display = profile.display_for_program(program)
        if display and tostring(display) ~= "" then
            return display
        end
    end
    return (profile and profile.key) or program or "generic"
end

--- Load all log detector modules through the detector middleware index.
--- 通过 detector 中间件索引加载全部日志检测模块。
--- @return table|nil, string|nil
local function load_detectors()
    if DETECTOR_CACHE ~= nil then
        return DETECTOR_CACHE, nil
    end

    local detector_dir = get_detector_dir()
    local index_path = vulcan.path.join(detector_dir, "index.lua")
    local chunk, load_error = loadfile(index_path)
    if not chunk then
        return nil, "failed to load TestKit detector index: " .. tostring(load_error)
    end

    local ok, loader = pcall(chunk)
    if not ok then
        return nil, "failed to initialize TestKit detector index: " .. tostring(loader)
    end
    if type(loader) ~= "function" then
        return nil, "TestKit detector index must return a loader function"
    end

    local load_ok, detectors = pcall(loader, detector_dir)
    if not load_ok then
        return nil, tostring(detectors)
    end

    DETECTOR_CACHE = detectors or {}
    return DETECTOR_CACHE, nil
end

--- Lazily load one adapter parser module by adapter key.
--- 根据适配器键懒加载一个解析器模块。
--- @param adapter string Adapter key.
--- @return function|nil, string|nil
local function load_adapter_module(adapter)
    local adapter_key = lower_text(adapter)
    if not is_safe_adapter_key(adapter_key) then
        return nil, "invalid adapter key `" .. tostring(adapter or "") .. "`; expected a known TestKit adapter name"
    end
    if not KNOWN_ADAPTER_KEYS[adapter_key] then
        return nil, "unknown adapter key `" .. adapter_key .. "`; expected a known TestKit adapter name"
    end

    if ADAPTER_CACHE[adapter_key] ~= nil then
        if ADAPTER_CACHE[adapter_key] == false then
            return nil, nil
        end
        return ADAPTER_CACHE[adapter_key], nil
    end

    local filename = adapter_key .. ".lua"
    local adapter_path = vulcan.path.join(get_entry_dir(), "adapters", filename)
    local chunk, load_error = loadfile(adapter_path)
    if not chunk then
        return nil, "failed to load adapter `" .. adapter_key .. "` from " .. adapter_path .. ": " .. tostring(load_error)
    end

    local ok, parser = pcall(chunk)
    if not ok then
        return nil, "failed to initialize adapter `" .. adapter_key .. "`: " .. tostring(parser)
    end
    if type(parser) ~= "function" then
        return nil, "adapter `" .. adapter_key .. "` must return a parser function"
    end

    ADAPTER_CACHE[adapter_key] = parser
    return parser, nil
end

--- Render one command token for compact Markdown display.
--- 渲染一个命令片段，用于紧凑的 Markdown 展示。
--- @param value any Command token.
--- @return string
local function render_command_token(value)
    local text = tostring(value or "")
    if text:find("%s") then
        return '"' .. text:gsub('"', '\\"') .. '"'
    end
    return text
end

--- Render a program and argument array as one readable command line.
--- 将程序与参数数组渲染为一条可读命令行。
--- @param program string Program name.
--- @param args table Argument array.
--- @return string
local function render_command(program, args)
    local parts = { render_command_token(program) }
    for _, arg in ipairs(args or {}) do
        table.insert(parts, render_command_token(arg))
    end
    return table.concat(parts, " ")
end

--- Build a concise input validation error as Markdown.
--- 构建紧凑的 Markdown 输入校验错误。
--- @param message string Error message.
--- @return string
local function render_input_error(message)
    return "# Vulcan TestKit Input Error\n\n## Status\nFAILED\n\n## Error\n```text\n" .. tostring(message) .. "\n```"
end

--- Normalize a numeric option with default, minimum, and maximum bounds.
--- 使用默认值、最小值和最大值规范化数值选项。
--- @param value any Candidate value.
--- @param default_value number Default value.
--- @param min_value number Minimum accepted value.
--- @param max_value number Maximum accepted value.
--- @param field_name string Field name for diagnostics.
--- @return number|nil, string|nil
local function normalize_number(value, default_value, min_value, max_value, field_name)
    if value == nil then
        return default_value, nil
    end
    if type(value) ~= "number" then
        return nil, "`" .. field_name .. "` must be a number"
    end
    if value < min_value then
        return nil, "`" .. field_name .. "` must be >= " .. tostring(min_value)
    end
    if value > max_value then
        return max_value, nil
    end
    return value, nil
end

--- Normalize the command argument array and stringify each item.
--- 规范化命令参数数组，并把每个元素转为字符串。
--- @param value any Candidate argument array.
--- @return table|nil, string|nil
local function normalize_args_array(value)
    if value == nil then
        return {}, nil
    end
    if type(value) ~= "table" then
        return nil, "`args` must be an array table"
    end

    local args = {}
    for index, item in ipairs(value) do
        if type(item) == "table" then
            return nil, "`args[" .. tostring(index) .. "]` must be scalar"
        end
        table.insert(args, tostring(item))
    end
    return args, nil
end

--- Normalize the validation phase hint.
--- 规范化验证阶段提示。
--- @param value any Candidate phase.
--- @return string|nil, string|nil
local function normalize_phase(value)
    local phase = is_blank_string(value) and "auto" or lower_text(value)
    if not VALID_PHASES[phase] then
        return nil, "`phase` must be build, test, check, lint, typecheck, or auto"
    end
    return phase, nil
end

--- Normalize a tool hint into an adapter key.
--- 将工具提示规范化为适配器键。
--- @param value any Candidate tool hint.
--- @return string|nil, string|nil
local function normalize_tool_hint(value)
    if is_blank_string(value) then
        return nil, nil
    end

    local hint = lower_text(value)
    if not is_safe_adapter_key(hint) then
        return nil, "`tool_hint` must be a known TestKit profile or adapter name without path separators"
    end

    local profile, profile_error = profile_for_alias(hint)
    if profile_error then
        return nil, profile_error
    end
    if profile then
        return profile.key, nil
    end
    if KNOWN_ADAPTER_KEYS[hint] then
        return hint, nil
    end
    return nil, "`tool_hint` must reference a known TestKit profile or adapter"
end

--- Return one argument at a one-based index as lowercase text.
--- 按一基索引返回参数的小写文本。
--- @param args table Argument array.
--- @param index number Argument index.
--- @return string|nil
local function lower_arg_at(args, index)
    local value = (args or {})[index]
    if value == nil then
        return nil
    end
    return lower_text(value)
end

--- Return the exact option key before any inline equals value.
--- 返回去除内联等号取值后的精确选项键。
--- @param value string Raw argument.
--- @return string
local function option_key(value)
    return trim(tostring(value or ""):match("^([^=]+)") or value)
end

--- Return a profile token while preserving case-sensitive options.
--- 返回 profile 识别片段，同时保留大小写敏感的选项。
--- @param value string Raw argument.
--- @return string
local function profile_token(value)
    local key = option_key(value)
    if key ~= "" and key:sub(1, 1) == "-" then
        return key
    end
    return lower_text(key)
end

--- Check whether one argument is an option-like token.
--- 判断参数是否像命令行选项。
--- @param value string|nil Raw argument.
--- @return boolean
local function is_option_arg(value)
    local text = tostring(value or "")
    return text ~= "" and text:sub(1, 1) == "-"
end

--- Return true when an option token carries its value inline with `=`.
--- 当选项以 `=` 内联携带取值时返回 true。
--- @param value string Raw argument.
--- @return boolean
local function has_inline_option_value(value)
    return tostring(value or ""):find("=", 1, true) ~= nil
end

--- Return true when any argument matches a blocked option token.
--- 当任意参数命中被禁止选项时返回 true。
--- @param args table Argument array.
--- @param start_index number Start index.
--- @param blocked table Blocked option set.
--- @return string|nil
local function find_blocked_any_arg_from(args, start_index, blocked)
    for index = start_index or 1, #(args or {}) do
        local arg = args[index]
        local key = option_key(arg)
        if blocked and blocked[key] then
            return key
        end
    end
    return nil
end

--- Return true when arguments from one index contain a specific token.
--- 判断从某个位置开始的参数是否包含指定片段。
--- @param args table Argument array.
--- @param start_index number Start index.
--- @param expected string Expected token.
--- @return boolean
local function args_contain_from(args, start_index, expected)
    local expected_token = profile_token(expected)
    for index = start_index or 1, #(args or {}) do
        if profile_token(args[index]) == expected_token then
            return true
        end
    end
    return false
end

--- Return true when any argument matches a blocked option token.
--- 当任意参数命中被禁止选项时返回 true。
--- @param args table Argument array.
--- @param blocked table Blocked option set.
--- @return string|nil
local function find_blocked_any_arg(args, blocked)
    return find_blocked_any_arg_from(args, 1, blocked)
end

--- Skip safe leading command options and return the first subcommand-like token.
--- 跳过安全的前置命令选项，并返回第一个像子命令的参数。
--- @param args table Argument array.
--- @param spec table Adapter profile spec.
--- @return string|nil, number|nil, string|nil
local function first_profile_token(args, spec)
    local index = 1
    while index <= #(args or {}) do
        local raw = tostring(args[index] or "")
        local value = profile_token(raw)
        local key = option_key(raw)

        if value == "" then
            index = index + 1
        elseif value == "--" then
            local next_raw = args[index + 1]
            return next_raw and profile_token(next_raw) or nil, index + 1, nil
        elseif spec and spec.allow_plus_toolchain and raw:sub(1, 1) == "+" then
            index = index + 1
        elseif spec and spec.blocked_first_args and spec.blocked_first_args[value] then
            return value, index, nil
        elseif spec and spec.allowed_first_args and spec.allowed_first_args[value] then
            return value, index, nil
        elseif spec and spec.flag_args and spec.flag_args[key] then
            index = index + 1
        elseif spec and spec.value_args and spec.value_args[key] then
            if has_inline_option_value(raw) then
                index = index + 1
            else
                index = index + 2
            end
        elseif is_option_arg(raw) then
            return value, index, "unknown_option"
        else
            return value, index, nil
        end
    end
    return nil, nil, nil
end

--- Return the next non-option argument after a command token.
--- 返回某个命令参数之后的下一个非选项参数。
--- @param args table Argument array.
--- @param start_index number Start index.
--- @param flag_args table|nil Option flags that do not consume a value.
--- @param value_args table|nil Options that consume a following value.
--- @return string|nil, number|nil
local function next_non_option_arg(args, start_index, flag_args, value_args)
    local index = start_index
    while index <= #(args or {}) do
        local raw = tostring(args[index] or "")
        local value = profile_token(raw)
        local key = option_key(raw)
        if value == "" or value == "--" then
            index = index + 1
        elseif flag_args and flag_args[key] then
            index = index + 1
        elseif value_args and value_args[key] then
            if has_inline_option_value(raw) then
                index = index + 1
            else
                index = index + 2
            end
        elseif is_option_arg(raw) then
            index = index + 1
        else
            return value, index
        end
    end
    return nil, nil
end

--- Run the shared first-token checks used by simple profile modules.
--- 执行简单 profile 模块共用的首参数检查。
--- @param first_arg string|nil First profile token.
--- @param first_error string|nil First-token parsing error.
--- @param spec table Profile spec.
--- @param program string Program basename.
--- @return string|nil
local function precheck_standard_profile(first_arg, first_error, spec, program)
    if first_error == "unknown_option" then
        return "`" .. program .. " " .. tostring(first_arg) .. "` is not a recognized validation profile option."
    end
    if spec.allow_empty_args and first_arg == nil then
        return nil
    end
    if first_arg == nil then
        return "`" .. program .. "` requires an explicit validation argument so TestKit can bound the command."
    end
    if spec.blocked_first_args and spec.blocked_first_args[first_arg] then
        return "`" .. program .. " " .. first_arg .. "` is not a bounded validation command for TestKit."
    end
    return nil
end

--- Build the helper surface exposed to language-specific profile modules.
--- 构建暴露给语言特定 profile 模块的 helper 接口。
--- @param spec table Profile spec.
--- @param program string Program basename.
--- @return table
local function build_profile_context(spec, program)
    return {
        spec = spec,
        program = program,
        lower_arg_at = lower_arg_at,
        first_profile_token = first_profile_token,
        next_non_option_arg = next_non_option_arg,
        args_contain_from = args_contain_from,
        find_blocked_any_arg_from = find_blocked_any_arg_from,
        precheck_standard_profile = precheck_standard_profile,
        profile_by_key = function(key)
            local profile = profile_by_key(key)
            return profile or {}
        end,
    }
end

--- Validate a data-only profile with the shared bounded-command rules.
--- 使用共享有界命令规则校验仅含数据的 profile。
--- @param request table Normalized request.
--- @param spec table Profile spec.
--- @param program string Program basename.
--- @return string|nil
local function validate_standard_profile(request, spec, program)
    local first_arg, _, first_error = first_profile_token(request.args, spec)
    local precheck_error = precheck_standard_profile(first_arg, first_error, spec, program)
    if precheck_error then
        return precheck_error
    end
    if spec.allow_empty_args and first_arg == nil then
        return nil
    end
    if spec.allowed_first_args and spec.allowed_first_args[first_arg] then
        return nil
    end
    return "`" .. program .. " " .. tostring(first_arg or "") .. "` is not in TestKit's validation allowlist."
end

--- Validate that one command is a bounded validation profile instead of a long-running app command.
--- 校验命令属于有界验证 profile，而不是长时间运行的应用命令。
--- @param request table Normalized request.
--- @return string|nil
local function validate_validation_profile(request)
    if request.mode ~= "run" then
        return nil
    end

    local program = program_basename(request.program)
    local spec, profile_error = profile_for_alias(program)
    if profile_error then
        return profile_error
    end
    if spec == nil then
        return "Unsupported validation executable `" .. program .. "`. Use analyze-only `log` mode or add a TestKit adapter profile for this tool."
    end

    local blocked_any = find_blocked_any_arg(request.args, spec.blocked_any_args)
    if blocked_any then
        return "`" .. program .. " " .. blocked_any .. "` is not a bounded validation option for TestKit."
    end

    request.profile_key = spec.key
    request.profile = spec
    if type(spec.validate) == "function" then
        local ok, result = pcall(spec.validate, request, build_profile_context(spec, program))
        if not ok then
            return "TestKit validation profile `" .. spec.key .. "` failed: " .. tostring(result)
        end
        return result
    end

    return validate_standard_profile(request, spec, program)
end

--- Detect the best adapter from hints, program, arguments, and log text.
--- 根据提示、程序、参数和日志文本识别最合适的适配器。
--- @param request table Normalized request.
--- @param log_text string Combined log text.
--- @return string, table
local function detect_tool(request, log_text)
    local program = program_basename(request.program)
    if request.tool_hint then
        local hint_profile = profile_by_key(request.tool_hint)
        if hint_profile then
            return profile_tool_name(hint_profile, program), profile_adapters(hint_profile, program)
        end
        return request.tool_hint, { request.tool_hint }
    end

    if request.profile then
        return profile_tool_name(request.profile, program), profile_adapters(request.profile, program)
    end

    local detectors, detector_error = load_detectors()
    if detector_error then
        return "generic", { "generic" }, detector_error
    end

    local context = {
        request = request,
        log_text = tostring(log_text or ""),
        lower_log = lower_text(log_text),
    }
    local first_detector_error = nil
    for _, detector in ipairs(detectors or {}) do
        local ok, detected = pcall(detector.detect, context)
        if ok and type(detected) == "table" then
            return detected.tool or detector.key or "generic", detected.adapters or detector.adapters or { "generic" }
        elseif not ok and first_detector_error == nil then
            first_detector_error = "TestKit detector `" .. tostring(detector.key or "unknown") .. "` failed: " .. tostring(detected)
        end
    end

    return "generic", { "generic" }, first_detector_error
end

--- Validate and normalize the raw tool input table.
--- 校验并规范化原始工具输入 table。
--- @param input table|nil Raw input.
--- @return table|nil, string|nil
local function validate_request(input)
    if input == nil then
        input = {}
    end
    if type(input) ~= "table" then
        return nil, "vulcan-testkit-run expects a table input"
    end

    local has_log = not is_blank_string(input.log)
    local has_program = not is_blank_string(input.program)
    if has_log and has_program then
        return nil, "Provide either `log` for analyze-only mode or `program` for run mode, not both"
    end
    if not has_log and not has_program then
        return nil, "Provide `program` for run mode or `log` for analyze-only mode"
    end
    local program = has_program and trim(input.program) or nil
    if has_program and not is_bare_program_name(program) then
        return nil, "`program` must be a bare executable name without path separators, drive prefixes, or whitespace"
    end
    if has_program and BLOCKED_PROGRAMS[program_basename(input.program)] then
        return nil,
            "`program` must be a validation executable, not a shell. Use direct tools such as cargo, go, python, pytest, mypy, ruff, node, tsc, vitest, jest, npm, pnpm, or yarn."
    end

    local args, args_error = normalize_args_array(input.args)
    if args_error then
        return nil, args_error
    end

    local phase, phase_error = normalize_phase(input.phase)
    if phase_error then
        return nil, phase_error
    end

    local timeout_ms, timeout_error = normalize_number(
        input.timeout_ms,
        DEFAULT_TIMEOUT_MS,
        1,
        MAX_TIMEOUT_MS,
        "timeout_ms"
    )
    if timeout_error then
        return nil, timeout_error
    end

    local max_diagnostics, diagnostics_error = normalize_number(
        input.max_diagnostics,
        DEFAULT_MAX_DIAGNOSTICS,
        1,
        MAX_DIAGNOSTICS,
        "max_diagnostics"
    )
    if diagnostics_error then
        return nil, diagnostics_error
    end

    local max_evidence_chars, evidence_error = normalize_number(
        input.max_evidence_chars,
        DEFAULT_MAX_EVIDENCE_CHARS,
        40,
        MAX_EVIDENCE_CHARS,
        "max_evidence_chars"
    )
    if evidence_error then
        return nil, evidence_error
    end

    if input.cwd ~= nil and type(input.cwd) ~= "string" then
        return nil, "`cwd` must be a string when provided"
    end
    if input.env ~= nil then
        return nil, "`env` is not supported by TestKit because environment variables can change validation command behavior"
    end
    if input.stdin ~= nil and type(input.stdin) ~= "string" then
        return nil, "`stdin` must be a string when provided"
    end

    local tool_hint, tool_hint_error = normalize_tool_hint(input.tool_hint)
    if tool_hint_error then
        return nil, tool_hint_error
    end

    local request = {
        mode = has_log and "analyze-log" or "run",
        program = program,
        args = args,
        cwd = is_blank_string(input.cwd) and nil or input.cwd,
        stdin = input.stdin,
        log = has_log and input.log or nil,
        phase = phase,
        tool_hint = tool_hint,
        timeout_ms = timeout_ms,
        max_diagnostics = math.floor(max_diagnostics),
        max_evidence_chars = math.floor(max_evidence_chars),
        include_warnings = input.include_warnings ~= false,
    }

    local profile_error = validate_validation_profile(request)
    if profile_error then
        return nil, profile_error
    end

    return request, nil
end

--- Create one normalized diagnostic object.
--- 创建一个规范化诊断对象。
--- @param severity string Diagnostic severity.
--- @param category string Diagnostic category.
--- @param message string Diagnostic message.
--- @param options table|nil Optional metadata.
--- @return table
local function diagnostic(severity, category, message, options)
    local opts = options or {}
    return {
        severity = severity,
        category = category,
        message = trim(message),
        file = opts.file,
        line = opts.line and tonumber(opts.line) or nil,
        column = opts.column and tonumber(opts.column) or nil,
        test = opts.test,
        raw = opts.raw,
        confidence = opts.confidence or 0.6,
        adapter = opts.adapter,
    }
end

--- Append a diagnostic when it contains a non-empty message.
--- 当诊断包含非空消息时追加到列表。
--- @param diagnostics table Diagnostic array.
--- @param item table Diagnostic item.
local function add_diagnostic(diagnostics, item)
    if item and not is_blank_string(item.message) then
        table.insert(diagnostics, item)
    end
end

--- Attach a source location to the latest diagnostic that does not have one.
--- 给最近一条尚无位置的诊断附加源码位置。
--- @param diagnostics table Diagnostic array.
--- @param file string Source file.
--- @param line string|number Source line.
--- @param column string|number|nil Source column.
local function attach_location_to_latest(diagnostics, file, line, column)
    for index = #diagnostics, 1, -1 do
        local item = diagnostics[index]
        if not item.file then
            item.file = trim(file)
            item.line = tonumber(line)
            item.column = column and tonumber(column) or nil
            return
        end
    end
end

--- Build one shared helper table passed into adapter modules.
--- 构建传给适配器模块的共享 helper table。
--- @return table
local function build_adapter_helpers()
    return {
        trim = trim,
        starts_with = starts_with,
        split_lines = split_lines,
        is_blank_string = is_blank_string,
        diagnostic = diagnostic,
        add_diagnostic = add_diagnostic,
        attach_location_to_latest = attach_location_to_latest,
        json_decode = vulcan.json.decode,
    }
end

--- Run all selected adapters and merge their diagnostics.
--- 运行所有选定适配器并合并诊断。
--- @param text string Raw output.
--- @param phase string Validation phase.
--- @param adapters table Adapter list.
--- @return table
local function run_adapters(text, phase, adapters)
    local diagnostics = {}
    local helpers = build_adapter_helpers()
    for _, adapter in ipairs(adapters or { "generic" }) do
        if adapter == "generic" and #diagnostics > 0 then
            -- Specialized adapters already found actionable diagnostics; generic fallback would mostly add duplicate noise.
            -- 专用适配器已经找到可行动诊断；generic 兜底此时大多只会增加重复噪音。
            break
        end

        local adapter_diagnostics = {}
        local parser, adapter_load_error = load_adapter_module(adapter)
        if adapter_load_error then
            adapter_diagnostics = {
                diagnostic("error", "adapter_error", adapter_load_error, {
                    adapter = adapter,
                    confidence = 1.0,
                }),
            }
        elseif parser then
            local ok, parsed = pcall(parser, text, phase, helpers)
            if ok and type(parsed) == "table" then
                adapter_diagnostics = parsed
            else
                adapter_diagnostics = {
                    diagnostic("error", "adapter_error", "adapter `" .. adapter .. "` failed: " .. tostring(parsed), {
                        adapter = adapter,
                        confidence = 1.0,
                    }),
                }
            end
        end
        for _, item in ipairs(adapter_diagnostics) do
            table.insert(diagnostics, item)
        end
    end
    return diagnostics
end

--- Build a stable deduplication key for one diagnostic.
--- 为单条诊断构建稳定去重键。
--- @param item table Diagnostic item.
--- @return string
local function diagnostic_key(item)
    return table.concat({
        tostring(item.severity or ""),
        tostring(item.category or ""),
        tostring(item.file or ""),
        tostring(item.line or ""),
        tostring(item.column or ""),
        tostring(item.test or ""),
        tostring(item.message or ""):sub(1, 180),
    }, "|")
end

--- Deduplicate diagnostics while preserving first-seen order.
--- 对诊断去重并保留首次出现顺序。
--- @param diagnostics table Diagnostic array.
--- @return table, number
local function deduplicate_diagnostics(diagnostics)
    local seen = {}
    local unique = {}
    local duplicate_count = 0
    for _, item in ipairs(diagnostics or {}) do
        local key = diagnostic_key(item)
        if not seen[key] then
            seen[key] = true
            table.insert(unique, item)
        else
            duplicate_count = duplicate_count + 1
        end
    end
    return unique, duplicate_count
end

--- Count warnings by category for compact reporting.
--- 按类别统计 warning，用于紧凑报告。
--- @param diagnostics table Diagnostic array.
--- @return table, number
local function count_warning_groups(diagnostics)
    local groups = {}
    local total = 0
    for _, item in ipairs(diagnostics or {}) do
        if item.severity == "warning" then
            local category = item.category or "warning"
            groups[category] = (groups[category] or 0) + 1
            total = total + 1
        end
    end
    return groups, total
end

--- Select compact warning examples for successful runs with non-blocking warnings.
--- 为成功但存在 warning 的运行选择紧凑 warning 示例。
--- @param diagnostics table Diagnostic array.
--- @param max_count number Maximum warning examples.
--- @return table
local function select_warning_examples(diagnostics, max_count)
    local examples = {}
    for _, item in ipairs(diagnostics or {}) do
        if item.severity == "warning" then
            table.insert(examples, item)
            if #examples >= max_count then
                break
            end
        end
    end
    return examples
end

--- Score one diagnostic for root-cause ordering, preferring source-bearing actionable diagnostics.
--- 为诊断计算根因排序分数，优先选择带源码位置且可直接行动的诊断。
--- @param item table Diagnostic item.
--- @return number
local function root_diagnostic_score(item)
    local score = 0
    if item.severity == "error" then
        score = score + 100
    elseif item.severity == "warning" then
        score = score + 20
    end

    if item.file then
        score = score + 50
    end
    if item.line then
        score = score + 20
    end

    if item.category == "compile"
        or item.category == "typecheck"
        or item.category == "panic"
        or item.category == "python"
        or item.category == "javascript"
        or item.category == "go"
    then
        score = score + 15
    elseif item.category == "test_failure" and not item.file then
        score = score - 15
    elseif item.category == "generic" then
        score = score - 20
    end

    return score
end

--- Select root diagnostics from normalized diagnostics.
--- 从规范化诊断中选择根诊断。
--- @param diagnostics table Diagnostic array.
--- @param max_count number Maximum returned items.
--- @param include_warnings boolean Whether warnings can be selected.
--- @return table, number
local function select_root_diagnostics(diagnostics, max_count, include_warnings)
    local candidates = {}
    local roots = {}
    for index, item in ipairs(diagnostics or {}) do
        if item.severity ~= "warning" then
            table.insert(candidates, {
                item = item,
                index = index,
                score = root_diagnostic_score(item),
            })
        end
    end

    if #candidates == 0 and include_warnings then
        for index, item in ipairs(diagnostics or {}) do
            if item.severity == "warning" then
                table.insert(candidates, {
                    item = item,
                    index = index,
                    score = root_diagnostic_score(item),
                })
            end
        end
    end

    table.sort(candidates, function(left, right)
        if left.score == right.score then
            return left.index < right.index
        end
        return left.score > right.score
    end)

    for _, candidate in ipairs(candidates) do
        table.insert(roots, candidate.item)
        if #roots >= max_count then
            break
        end
    end

    local collapsed = math.max(0, #(diagnostics or {}) - #roots)
    return roots, collapsed
end

--- Collect failed test names from diagnostics.
--- 从诊断中收集失败测试名称。
--- @param diagnostics table Diagnostic array.
--- @return table
local function collect_failed_tests(diagnostics)
    local seen = {}
    local tests = {}
    for _, item in ipairs(diagnostics or {}) do
        if item.test and not seen[item.test] then
            seen[item.test] = true
            table.insert(tests, {
                name = item.test,
                file = item.file,
                category = item.category,
            })
        end
    end
    return tests
end

--- Collect unique source references from diagnostics.
--- 从诊断中收集唯一源码引用。
--- @param diagnostics table Diagnostic array.
--- @return table
local function collect_source_refs(diagnostics)
    local seen = {}
    local refs = {}
    for _, item in ipairs(diagnostics or {}) do
        if item.file then
            local ref = item.file
            if item.line then
                ref = ref .. ":" .. tostring(item.line)
                if item.column then
                    ref = ref .. ":" .. tostring(item.column)
                end
            end
            if not seen[ref] then
                seen[ref] = true
                table.insert(refs, ref)
            end
        end
    end
    return refs
end

--- Truncate one evidence line to the configured budget.
--- 将一行证据截断到配置预算内。
--- @param text string Evidence text.
--- @param max_chars number Maximum characters.
--- @return string
local function truncate_evidence(text, max_chars)
    local value = trim(text)
    if #value <= max_chars then
        return value
    end
    return value:sub(1, max_chars) .. "..."
end

--- Determine the final validation status from process state and diagnostics.
--- 根据进程状态与诊断确定最终验证状态。
--- @param request table Normalized request.
--- @param execution table Execution result.
--- @param diagnostics table Diagnostic array.
--- @return string
local function determine_status(request, execution, diagnostics)
    if execution and execution.timed_out then
        return "timeout"
    end

    if request.mode == "run" and execution and execution.success ~= false then
        for _, item in ipairs(diagnostics or {}) do
            if item.severity == "warning" then
                return "warning"
            end
        end
        return "passed"
    end

    for _, item in ipairs(diagnostics or {}) do
        if item.severity == "error" then
            return "failed"
        end
    end

    if request.mode == "run" then
        if execution and execution.success == false then
            return "failed"
        end
        return "passed"
    end

    for _, item in ipairs(diagnostics or {}) do
        if item.severity == "warning" then
            return "warning"
        end
    end
    return "unknown"
end

--- Build next-action hints from root diagnostics and source references.
--- 根据根诊断与源码引用构建下一步行动提示。
--- @param status string Final status.
--- @param roots table Root diagnostics.
--- @param source_refs table Source references.
--- @param request table Normalized request.
--- @return table
local function build_next_actions(status, roots, source_refs, request)
    local actions = {}
    if status == "passed" then
        table.insert(actions, "No action required.")
        return actions
    end

    if status == "warning" then
        if source_refs[1] then
            table.insert(actions, "Review the first warning source ref only if warnings are in scope: " .. source_refs[1])
        else
            table.insert(actions, "Warnings were detected; inspect them only if the current task includes warning cleanup.")
        end
        return actions
    end

    if source_refs[1] then
        table.insert(actions, "Inspect the first source ref with CodeKit AST detail before editing: " .. source_refs[1])
    elseif roots[1] and roots[1].message then
        table.insert(actions, "Search the root diagnostic text with CodeKit rg when the owning file is still unknown.")
    else
        table.insert(actions, "No strong root diagnostic was detected; rerun with a more specific tool_hint or machine-readable command mode.")
    end

    if request.phase == "auto" then
        table.insert(actions, "Pass an explicit phase hint on the next run if build and test output are mixed.")
    end
    return actions
end

--- Render one diagnostic as a compact Markdown bullet.
--- 将一条诊断渲染为紧凑 Markdown 条目。
--- @param item table Diagnostic item.
--- @param index number One-based index.
--- @param max_evidence_chars number Evidence character budget.
--- @return string
local function render_diagnostic(item, index, max_evidence_chars)
    local location = item.file or "(no file)"
    if item.line then
        location = location .. ":" .. tostring(item.line)
        if item.column then
            location = location .. ":" .. tostring(item.column)
        end
    end

    local lines = {
        tostring(index) .. ". [" .. tostring(item.severity) .. "][" .. tostring(item.category) .. "] " .. tostring(item.message),
        "   - source: " .. location,
        "   - adapter: " .. tostring(item.adapter or "unknown") .. ", confidence: " .. tostring(item.confidence or 0),
    }
    if item.test then
        table.insert(lines, "   - test: " .. tostring(item.test))
    end
    if item.raw then
        table.insert(lines, "   - evidence: `" .. truncate_evidence(item.raw, max_evidence_chars):gsub("`", "'") .. "`")
    end
    return table.concat(lines, "\n")
end

--- Render grouped warnings as compact Markdown lines.
--- 将分组 warning 渲染为紧凑 Markdown 行。
--- @param groups table Warning groups.
--- @return table
local function render_warning_groups(groups)
    local lines = {}
    for category, count in pairs(groups or {}) do
        table.insert(lines, "- " .. tostring(category) .. ": " .. tostring(count))
    end
    table.sort(lines)
    return lines
end

--- Render one warning diagnostic as a compact single-line Markdown bullet.
--- 将一条 warning 诊断渲染为单行紧凑 Markdown 条目。
--- @param item table Warning diagnostic.
--- @param index number One-based index.
--- @return string
local function render_compact_warning(item, index)
    local location = item.file or "(no source)"
    if item.line then
        location = location .. ":" .. tostring(item.line)
        if item.column then
            location = location .. ":" .. tostring(item.column)
        end
    end
    return tostring(index) .. ". " .. tostring(item.message) .. " @ " .. location
end

--- Render the final AI-friendly TestKit report.
--- 渲染最终 AI 友好的 TestKit 报告。
--- @param report table Final report model.
--- @return string
local function render_report(report)
    if report.status == "passed" then
        local lines = {
            "# Vulcan TestKit Result",
            "",
            "PASSED: Validation passed. No diagnostics detected.",
        }
        return table.concat(lines, "\n")
    end

    if report.status == "warning" then
        local lines = {
            "# Vulcan TestKit Result",
            "",
            "WARNING: " .. report.summary,
            "",
        }
        if #report.warning_examples == 0 then
            table.insert(lines, "- No warning examples returned.")
        else
            for index, item in ipairs(report.warning_examples) do
                table.insert(lines, render_compact_warning(item, index))
            end
        end
        table.insert(lines, "")
        for _, action in ipairs(report.next_actions) do
            table.insert(lines, "Next: " .. action)
        end
        return table.concat(lines, "\n")
    end

    local lines = {
        "# Vulcan TestKit Result",
        "",
        "## Status",
        string.upper(report.status),
        "",
        "## Run",
        "- mode: " .. report.mode,
        "- phase: " .. report.phase,
        "- tool: " .. report.tool,
        "- adapters: " .. table.concat(report.adapters, ", "),
    }

    if report.command then
        table.insert(lines, "- command: `" .. report.command:gsub("`", "'") .. "`")
    end
    if report.cwd then
        table.insert(lines, "- cwd: " .. report.cwd)
    end
    if report.exit_code ~= nil then
        table.insert(lines, "- exit_code: " .. tostring(report.exit_code))
    end
    if report.timed_out ~= nil then
        table.insert(lines, "- timed_out: " .. tostring(report.timed_out))
    end

    table.insert(lines, "")
    table.insert(lines, "## Summary")
    table.insert(lines, report.summary)

    table.insert(lines, "")
    table.insert(lines, "## Root Diagnostics")
    if #report.roots == 0 then
        if report.status == "warning" and #report.warning_examples > 0 then
            for index, item in ipairs(report.warning_examples) do
                table.insert(lines, render_diagnostic(item, index, report.max_evidence_chars))
            end
        else
            table.insert(lines, "- No root diagnostics detected in the returned budget.")
        end
    else
        for index, item in ipairs(report.roots) do
            table.insert(lines, render_diagnostic(item, index, report.max_evidence_chars))
        end
    end

    table.insert(lines, "")
    table.insert(lines, "## Failed Tests")
    if #report.failed_tests == 0 then
        table.insert(lines, "- None detected.")
    else
        for _, test in ipairs(report.failed_tests) do
            local suffix = test.file and (" @ " .. test.file) or ""
            table.insert(lines, "- " .. tostring(test.name) .. suffix)
        end
    end

    table.insert(lines, "")
    table.insert(lines, "## Source Refs")
    if #report.source_refs == 0 then
        table.insert(lines, "- None detected.")
    else
        for _, ref in ipairs(report.source_refs) do
            table.insert(lines, "- " .. ref)
        end
    end

    table.insert(lines, "")
    table.insert(lines, "## Collapsed Noise")
    table.insert(lines, "- raw_output_chars: " .. tostring(report.raw_output_chars))
    table.insert(lines, "- duplicate_diagnostics: " .. tostring(report.duplicate_diagnostics))
    table.insert(lines, "- collapsed_diagnostics: " .. tostring(report.collapsed_diagnostics))
    table.insert(lines, "- warning_total: " .. tostring(report.warning_total))
    local warning_lines = render_warning_groups(report.warning_groups)
    for _, warning_line in ipairs(warning_lines) do
        table.insert(lines, warning_line)
    end

    table.insert(lines, "")
    table.insert(lines, "## Next Actions")
    for _, action in ipairs(report.next_actions) do
        table.insert(lines, "- " .. action)
    end

    table.insert(lines, "")
    table.insert(lines, "## Raw Log Policy")
    table.insert(lines, "- Full stdout/stderr is intentionally not returned by default.")

    return table.concat(lines, "\n")
end

--- Execute the requested validation command through the LuaSkills process bridge.
--- 通过 LuaSkills 进程桥执行请求的验证命令。
--- @param request table Normalized request.
--- @return table
local function execute_validation_command(request)
    local spec = {
        program = request.program,
        args = request.args,
        cwd = request.cwd or vulcan.runtime.cwd(),
        stdin = request.stdin,
        timeout_ms = request.timeout_ms,
    }
    local result = vulcan.process.exec(spec)
    return result or {
        ok = false,
        success = false,
        code = nil,
        stdout = "",
        stderr = "",
        timed_out = false,
        error = "vulcan.process.exec returned nil",
    }
end

--- Build one combined output string from stdout and stderr without exposing it directly.
--- 从 stdout 与 stderr 构建组合输出字符串，但不直接暴露完整内容。
--- @param execution table Execution result.
--- @return string
local function combined_output(execution)
    local stdout = tostring((execution and execution.stdout) or "")
    local stderr = tostring((execution and execution.stderr) or "")
    if stdout == "" then
        return stderr
    end
    if stderr == "" then
        return stdout
    end
    return stdout .. "\n" .. stderr
end

--- Build a human-readable summary sentence from status and diagnostic counts.
--- 根据状态与诊断数量构建可读摘要句。
--- @param status string Final status.
--- @param roots table Root diagnostics.
--- @param warning_total number Warning count.
--- @param collapsed_diagnostics number Collapsed diagnostic count.
--- @return string
local function build_summary(status, roots, warning_total, collapsed_diagnostics)
    if status == "passed" then
        return "Validation passed. No diagnostics detected."
    end
    if status == "warning" then
        return "Validation passed with "
            .. tostring(warning_total)
            .. " warning(s). Warnings are summarized only; full stdout/stderr is omitted."
    end
    if status == "timeout" then
        return "Validation timed out; inspect the command scope or increase timeout only if the command is expected to be long-running."
    end
    if #roots > 0 then
        return "Detected "
            .. tostring(#roots)
            .. " root diagnostic(s), collapsed "
            .. tostring(collapsed_diagnostics)
            .. " additional diagnostic(s), and grouped "
            .. tostring(warning_total)
            .. " warning(s)."
    end
    return "No strong root diagnostic was detected; the command may have failed before producing parseable output."
end

--- Main tool entry for Vulcan TestKit.
--- Vulcan TestKit 的主工具入口。
--- @param args table|nil Tool arguments.
--- @return string
return function(args)
    local request, validation_error = validate_request(args)
    if validation_error then
        return render_input_error(validation_error)
    end

    local execution = nil
    local output = request.log
    if request.mode == "run" then
        execution = execute_validation_command(request)
        output = combined_output(execution)
        if execution.error and tostring(execution.error) ~= "" then
            output = tostring(output or "") .. "\n" .. tostring(execution.error)
        end
    end

    local tool, adapters, detection_error = detect_tool(request, output)
    local diagnostics = run_adapters(output, request.phase, adapters)
    if detection_error then
        add_diagnostic(diagnostics, diagnostic("error", "detector_error", detection_error, {
            adapter = "detector",
            confidence = 1.0,
        }))
    end
    local unique_diagnostics, duplicate_count = deduplicate_diagnostics(diagnostics)
    local warning_groups, warning_total = count_warning_groups(unique_diagnostics)
    local roots, collapsed_count = select_root_diagnostics(
        unique_diagnostics,
        request.max_diagnostics,
        request.include_warnings
    )
    local failed_tests = collect_failed_tests(unique_diagnostics)
    local warning_examples = select_warning_examples(unique_diagnostics, math.min(request.max_diagnostics, 3))
    local source_refs = collect_source_refs(#roots > 0 and roots or warning_examples)
    local status = determine_status(request, execution, unique_diagnostics)
    local next_actions = build_next_actions(status, roots, source_refs, request)

    local report = {
        status = status,
        mode = request.mode,
        phase = request.phase,
        tool = tool,
        adapters = adapters,
        command = request.program and render_command(request.program, request.args) or nil,
        cwd = request.cwd,
        exit_code = execution and execution.code or nil,
        timed_out = execution and execution.timed_out or nil,
        summary = build_summary(status, roots, warning_total, collapsed_count),
        roots = roots,
        warning_examples = warning_examples,
        failed_tests = failed_tests,
        source_refs = source_refs,
        raw_output_chars = #(output or ""),
        duplicate_diagnostics = duplicate_count,
        collapsed_diagnostics = collapsed_count,
        warning_total = warning_total,
        warning_groups = warning_groups,
        next_actions = next_actions,
        max_evidence_chars = request.max_evidence_chars,
    }

    return render_report(report)
end
