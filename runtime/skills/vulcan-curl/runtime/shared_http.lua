--[[
shared_http
Provide shared HTTP request helpers for vulcan-curl tools based on lua-curl.
基于 lua-curl 为 vulcan-curl 工具提供共享 HTTP 请求辅助能力。
]]

-- Return one stable default timeout in milliseconds for host-side safety.
-- 返回宿主侧安全使用的默认超时时间（毫秒）。
local function default_timeout_ms()
    return 60000
end

-- Return whether the current path text should be treated as absolute.
-- 判断当前路径文本是否应视为绝对路径。
local function is_absolute_path(path_text)
    if type(path_text) ~= "string" or path_text == "" then
        return false
    end

    if path_text:match("^%a:[/\\]") then
        return true
    end

    if path_text:match("^[/\\][/\\]?") then
        return true
    end

    if path_text:match("^/") then
        return true
    end

    return false
end

-- Resolve one local path against the selected working directory.
-- 基于选定工作目录解析一个本地路径。
local function resolve_local_path(base_dir, path_text)
    if is_absolute_path(path_text) then
        return path_text
    end

    local join = vulcan and vulcan.path and vulcan.path.join
    if type(join) == "function" then
        return join(base_dir, path_text)
    end

    local separator = package.config and package.config:sub(1, 1) or "/"
    if base_dir:sub(-1) == "/" or base_dir:sub(-1) == "\\" then
        return base_dir .. path_text
    end

    return base_dir .. separator .. path_text
end

-- Return the current runtime working directory exposed by the host.
-- 返回宿主暴露的当前运行时工作目录。
local function resolve_runtime_cwd()
    if vulcan and vulcan.runtime and type(vulcan.runtime.cwd) == "function" then
        local ok, cwd = pcall(vulcan.runtime.cwd)
        if ok and type(cwd) == "string" and cwd ~= "" then
            return cwd
        end
    end

    return "."
end

-- Return whether the current Lua runtime should be treated as Windows.
-- 判断当前 Lua 运行时是否应视为 Windows 平台。
local function is_windows_runtime()
    if type(jit) == "table" and type(jit.os) == "string" then
        return jit.os == "Windows"
    end

    local separator = package.config and package.config:sub(1, 1) or "/"
    return separator == "\\"
end

-- Return whether the request should prefer Windows native CA lookup.
-- 判断当前请求是否应优先启用 Windows 原生 CA 查找。
local function should_enable_windows_native_ca(curl, spec)
    if not is_windows_runtime() then
        return false
    end

    if spec.insecure or spec.cacert or spec.capath then
        return false
    end

    if type(curl) ~= "table" then
        return false
    end

    if type(curl.OPT_SSL_OPTIONS) ~= "number" or type(curl.SSLOPT_NATIVE_CA) ~= "number" then
        return false
    end

    return true
end

-- Apply native CA preferences to the easy handle when the runtime supports it.
-- 在运行时支持时将原生 CA 偏好应用到 easy 句柄。
local function apply_native_ca_preferences(curl, easy, spec)
    if not should_enable_windows_native_ca(curl, spec) then
        return
    end

    local native_ca_flag = curl.SSLOPT_NATIVE_CA
    local ok, err = easy:setopt(curl.OPT_SSL_OPTIONS, native_ca_flag)
    if not ok then
        error("Failed to enable Windows native CA for target TLS: " .. tostring(err))
    end

    if spec.proxy and type(curl.OPT_PROXY_SSL_OPTIONS) == "number" then
        ok, err = easy:setopt(curl.OPT_PROXY_SSL_OPTIONS, native_ca_flag)
        if not ok then
            error("Failed to enable Windows native CA for proxy TLS: " .. tostring(err))
        end
    end
end

-- Return whether the current string starts with one of the provided prefixes.
-- 判断当前字符串是否以给定前缀集合中的任意前缀开头。
local function starts_with_any(text, prefixes)
    for _, prefix in ipairs(prefixes) do
        if text:sub(1, #prefix) == prefix then
            return true
        end
    end
    return false
end

-- URL-encode one string for query construction.
-- 对字符串做 URL 编码，供查询串构造使用。
local function url_encode(text)
    local normalized = tostring(text or "")
    normalized = normalized:gsub("\r\n", "\n")
    normalized = normalized:gsub("\n", "\r\n")
    normalized = normalized:gsub("([^%w%-_%.~])", function(ch)
        return string.format("%%%02X", string.byte(ch))
    end)
    return normalized
end

-- Append one query fragment to the target URL.
-- 将一个查询片段追加到目标 URL。
local function append_query_fragment(url, fragment)
    if fragment == nil or fragment == "" then
        return url
    end

    local separator = "?"
    if url:find("?", 1, true) then
        separator = "&"
    end

    return url .. separator .. fragment
end

-- Read one UTF-8 file through the host file API when available.
-- 优先通过宿主文件 API 读取一个 UTF-8 文件。
local function read_text_file(path_text)
    if vulcan and vulcan.fs and type(vulcan.fs.read) == "function" then
        return vulcan.fs.read(path_text)
    end

    local handle, open_error = io.open(path_text, "rb")
    if not handle then
        error("Failed to open file: " .. tostring(open_error))
    end

    local content = handle:read("*a")
    handle:close()
    return content
end

-- Write one UTF-8 text file using the host file API when available.
-- 优先通过宿主文件 API 写入一个 UTF-8 文本文件。
local function write_text_file(path_text, content)
    local handle, open_error = io.open(path_text, "wb")
    if not handle then
        error("Failed to open output file: " .. tostring(open_error))
    end

    local ok, write_error = handle:write(tostring(content or ""))
    handle:close()
    if not ok then
        error("Failed to write output file: " .. tostring(write_error))
    end
end

-- Return one basename extracted from a path-like string.
-- 从路径样式字符串中提取文件名。
local function basename(path_text)
    return tostring(path_text):match("([^/\\]+)$") or tostring(path_text)
end

-- Return whether one header list already contains the target header name.
-- 判断某个请求头列表中是否已包含目标头名。
local function has_header(headers, header_name)
    local expected = tostring(header_name):lower()
    for _, header_line in ipairs(headers) do
        local name = tostring(header_line):match("^%s*([^:]+)%s*:")
        if name and name:lower() == expected then
            return true
        end
    end
    return false
end

-- Normalize optional render flags into one lookup table for response shaping.
-- 将可选渲染标记规范化为查找表，用于控制响应展示形态。
local function normalize_render_flags(flags)
    local normalized = {}
    if flags == nil then
        return normalized
    end

    local values = {}
    if type(flags) == "string" then
        for token in flags:gmatch("([^,]+)") do
            local trimmed = token:match("^%s*(.-)%s*$")
            if trimmed and trimmed ~= "" then
                values[#values + 1] = trimmed
            end
        end
    elseif type(flags) == "table" then
        for _, value in ipairs(flags) do
            if type(value) == "string" then
                values[#values + 1] = value
            end
        end
    else
        return normalized
    end

    for _, value in ipairs(values) do
        local flag = tostring(value or ""):lower():gsub("_", "-")
        if flag == "request-header" or flag == "get-header" then
            normalized.request_header = true
        elseif flag == "response-header" or flag == "responst-header" then
            normalized.response_header = true
        end
    end

    return normalized
end

-- Return whether a normalized render flag is enabled on the current request spec.
-- 判断当前请求规格中某个规范化渲染标记是否已启用。
local function has_render_flag(spec, flag_name)
    local flags = spec and spec.render_flags
    return type(flags) == "table" and flags[flag_name] == true
end

-- Build render flags from structured inputs and legacy header switches.
-- 根据结构化输入与旧版头信息开关构建渲染标记。
local function build_render_flags(args)
    local flags = normalize_render_flags(args and args.flags)
    if args and args.include_headers == true then
        flags.response_header = true
    end
    return flags
end

-- Normalize one incoming argv list into a plain Lua string array.
-- 将传入的参数数组归一化为普通 Lua 字符串数组。
local function normalize_argv(args)
    if type(args) ~= "table" then
        error("vulcan-curl requires args to be an array table")
    end

    local argv = {}
    for index, value in ipairs(args) do
        if type(value) ~= "string" then
            error("curl argv items must be strings at position: " .. tostring(index))
        end
        argv[#argv + 1] = value
    end

    if argv[1] == "curl" then
        table.remove(argv, 1)
    end

    if #argv == 0 then
        error("curl argv must not be empty")
    end

    return argv
end

-- Parse one curl form token into a structured form entry.
-- 将一个 curl form 片段解析成结构化表单项。
local function parse_form_token(base_dir, token)
    local name, raw_value = tostring(token):match("^([^=]+)=(.*)$")
    if not name then
        error("Unsupported -F/--form syntax: " .. tostring(token))
    end

    local entry = {
        name = name,
        kind = "content",
        value = raw_value,
        mime_type = nil,
        filename = nil,
    }

    if raw_value:sub(1, 1) == "@" then
        entry.kind = "file"
        local path_and_meta = raw_value:sub(2)
        local file_path, meta = path_and_meta:match("^([^;]+);?(.*)$")
        entry.value = resolve_local_path(base_dir, file_path)
        if meta and meta ~= "" then
            local mime_type = meta:match("type=([^;]+)")
            local filename = meta:match("filename=([^;]+)")
            entry.mime_type = mime_type
            entry.filename = filename
        end
    elseif raw_value:sub(1, 1) == "<" then
        entry.kind = "buffer"
        local path_and_meta = raw_value:sub(2)
        local file_path, meta = path_and_meta:match("^([^;]+);?(.*)$")
        local resolved = resolve_local_path(base_dir, file_path)
        entry.value = read_text_file(resolved)
        entry.filename = basename(resolved)
        if meta and meta ~= "" then
            local mime_type = meta:match("type=([^;]+)")
            local filename = meta:match("filename=([^;]+)")
            entry.mime_type = mime_type
            if filename and filename ~= "" then
                entry.filename = filename
            end
        end
    end

    return entry
end

-- Convert one --data-urlencode token into one encoded query fragment.
-- 将一个 --data-urlencode 片段转换为编码后的查询参数片段。
local function encode_data_urlencode(base_dir, token)
    local raw = tostring(token)
    if raw:sub(1, 1) == "@" then
        raw = read_text_file(resolve_local_path(base_dir, raw:sub(2)))
    end

    local key, value = raw:match("^([^=]+)=(.*)$")
    if key then
        return key .. "=" .. url_encode(value)
    end

    return url_encode(raw)
end

-- Parse curl-style argv into one structured request specification.
-- 将 curl 风格参数数组解析为结构化请求规格。
local function parse_curl_argv(argv, base_dir)
    local spec = {
        url = nil,
        request_method = nil,
        headers = {},
        body_parts = {},
        query_parts = {},
        forms = {},
        follow_location = false,
        insecure = false,
        head_only = false,
        http_get = false,
        output_path = nil,
        dump_header_path = nil,
        userpwd = nil,
        useragent = nil,
        referer = nil,
        cookie = nil,
        cookie_file = nil,
        cookie_jar = nil,
        proxy = nil,
        proxy_userpwd = nil,
        cacert = nil,
        capath = nil,
        cert = nil,
        key = nil,
        timeout_seconds = nil,
        connect_timeout_seconds = nil,
        compressed = false,
        fail_on_http_error = false,
        fail_with_body = false,
        retries = 0,
        retry_delay_seconds = 0,
        retry_max_time_seconds = nil,
        http_version = nil,
        silent = false,
        include = false,
        render_flags = {},
    }

    local value_options = {
        ["-X"] = "request_method",
        ["--request"] = "request_method",
        ["-H"] = "header",
        ["--header"] = "header",
        ["-d"] = "data",
        ["--data"] = "data",
        ["--data-raw"] = "data",
        ["--data-binary"] = "data",
        ["--data-urlencode"] = "data_urlencode",
        ["--json"] = "json",
        ["-F"] = "form",
        ["--form"] = "form",
        ["-u"] = "userpwd",
        ["--user"] = "userpwd",
        ["-A"] = "useragent",
        ["--user-agent"] = "useragent",
        ["-e"] = "referer",
        ["--referer"] = "referer",
        ["-o"] = "output_path",
        ["--output"] = "output_path",
        ["-D"] = "dump_header_path",
        ["--dump-header"] = "dump_header_path",
        ["-m"] = "timeout_seconds",
        ["--max-time"] = "timeout_seconds",
        ["--connect-timeout"] = "connect_timeout_seconds",
        ["--retry"] = "retries",
        ["--retry-delay"] = "retry_delay_seconds",
        ["--retry-max-time"] = "retry_max_time_seconds",
        ["--url"] = "url",
        ["--proxy"] = "proxy",
        ["--proxy-user"] = "proxy_userpwd",
        ["-b"] = "cookie",
        ["--cookie"] = "cookie",
        ["-c"] = "cookie_jar",
        ["--cookie-jar"] = "cookie_jar",
        ["--cacert"] = "cacert",
        ["--capath"] = "capath",
        ["-E"] = "cert",
        ["--cert"] = "cert",
        ["--key"] = "key",
    }

    local index = 1
    while index <= #argv do
        local token = argv[index]

        if token == "-L" or token == "--location" then
            spec.follow_location = true
        elseif token == "-k" or token == "--insecure" then
            spec.insecure = true
        elseif token == "-I" or token == "--head" then
            spec.head_only = true
        elseif token == "-G" or token == "--get" then
            spec.http_get = true
        elseif token == "--compressed" then
            spec.compressed = true
        elseif token == "-f" or token == "--fail" then
            spec.fail_on_http_error = true
        elseif token == "--fail-with-body" then
            spec.fail_on_http_error = true
            spec.fail_with_body = true
        elseif token == "-s" or token == "--silent" then
            spec.silent = true
        elseif token == "-S" or token == "--show-error" then
            -- Linux curl combines -sS for quiet mode plus visible errors.
            -- Linux curl 中 -sS 表示静默传输但保留错误显示，这里无需额外动作。
        elseif token == "-i" or token == "--include" then
            spec.include = true
        elseif token == "--http1.1" then
            spec.http_version = "1.1"
        elseif token == "--http2" then
            spec.http_version = "2"
        elseif value_options[token] then
            local next_value = argv[index + 1]
            if type(next_value) ~= "string" then
                error("curl option requires one value: " .. tostring(token))
            end

            local field_name = value_options[token]
            if field_name == "header" then
                spec.headers[#spec.headers + 1] = next_value
            elseif field_name == "data" then
                spec.body_parts[#spec.body_parts + 1] = next_value
            elseif field_name == "data_urlencode" then
                spec.query_parts[#spec.query_parts + 1] = encode_data_urlencode(base_dir, next_value)
            elseif field_name == "json" then
                spec.body_parts[#spec.body_parts + 1] = next_value
                if not has_header(spec.headers, "Content-Type") then
                    spec.headers[#spec.headers + 1] = "Content-Type: application/json"
                end
                if not has_header(spec.headers, "Accept") then
                    spec.headers[#spec.headers + 1] = "Accept: application/json"
                end
            elseif field_name == "form" then
                spec.forms[#spec.forms + 1] = parse_form_token(base_dir, next_value)
            elseif field_name == "output_path" or field_name == "dump_header_path"
                or field_name == "cacert" or field_name == "capath"
                or field_name == "cert" or field_name == "key"
                or field_name == "cookie_jar"
            then
                spec[field_name] = resolve_local_path(base_dir, next_value)
            elseif field_name == "cookie" then
                local resolved_candidate = resolve_local_path(base_dir, next_value)
                local exists = vulcan and vulcan.fs and type(vulcan.fs.exists) == "function" and vulcan.fs.exists(resolved_candidate)
                if exists and not next_value:find("=") then
                    spec.cookie_file = resolved_candidate
                else
                    spec.cookie = next_value
                end
            elseif field_name == "timeout_seconds"
                or field_name == "connect_timeout_seconds"
                or field_name == "retries"
                or field_name == "retry_delay_seconds"
                or field_name == "retry_max_time_seconds"
            then
                local numeric = tonumber(next_value)
                if not numeric then
                    error("curl numeric option expects a number: " .. tostring(token))
                end
                spec[field_name] = numeric
            else
                spec[field_name] = next_value
            end

            index = index + 1
        elseif token == "--" then
            if index + 1 <= #argv then
                spec.url = argv[index + 1]
                index = #argv
            end
        elseif starts_with_any(token, { "http://", "https://", "ftp://", "ftps://", "file://" }) then
            spec.url = token
        elseif token:sub(1, 1) == "-" then
            error("Unsupported curl option in current basic version: " .. tostring(token))
        else
            spec.url = token
        end

        index = index + 1
    end

    if not spec.url or spec.url == "" then
        error("curl request requires one target URL")
    end

    if spec.head_only and (#spec.body_parts > 0 or #spec.forms > 0 or #spec.query_parts > 0) then
        error("HEAD requests do not support body or form payload in this tool")
    end

    if spec.http_get and #spec.forms > 0 then
        error("GET mode cannot be combined with form upload in this tool")
    end

    return spec
end

-- Return the final HTTP method selected from parsed curl semantics.
-- 根据解析后的 curl 语义返回最终 HTTP 方法。
local function resolve_http_method(spec)
    if spec.head_only then
        return "HEAD"
    end

    if spec.request_method and spec.request_method ~= "" then
        return string.upper(spec.request_method)
    end

    if spec.http_get then
        return "GET"
    end

    if #spec.forms > 0 or #spec.body_parts > 0 then
        return "POST"
    end

    return "GET"
end

-- Build one request body or query string payload from parsed curl segments.
-- 基于解析后的 curl 片段构建请求体或查询串负载。
local function build_payload(spec)
    local payload = table.concat(spec.body_parts, "&")
    if spec.http_get then
        local query = payload
        for _, fragment in ipairs(spec.query_parts) do
            if query == "" then
                query = fragment
            else
                query = query .. "&" .. fragment
            end
        end
        payload = ""
        spec.query_parts = { query }
    end
    return payload
end

-- Apply one parsed form payload to the easy handle.
-- 将解析后的 form 负载应用到 easy 句柄。
local function apply_form_payload(curl, easy, spec)
    if #spec.forms == 0 then
        return nil
    end

    local form, form_error = curl.form()
    if not form then
        error("Failed to create curl form: " .. tostring(form_error))
    end

    for _, item in ipairs(spec.forms) do
        local ok, err
        if item.kind == "file" then
            ok, err = form:add_file(item.name, item.value, item.mime_type, item.filename)
        elseif item.kind == "buffer" then
            ok, err = form:add_buffer(item.name, item.filename or (item.name .. ".txt"), item.value, item.mime_type)
        else
            ok, err = form:add_content(item.name, item.value)
        end

        if not ok then
            form:free()
            error("Failed to add curl form field '" .. tostring(item.name) .. "': " .. tostring(err))
        end
    end

    local ok, err = easy:setopt_httppost(form)
    if not ok then
        form:free()
        error("Failed to apply curl form payload: " .. tostring(err))
    end

    return form
end

-- Create and configure one easy handle from the parsed curl spec.
-- 基于解析后的 curl 规格创建并配置一个 easy 句柄。
local function create_easy_handle(curl, spec, base_dir, default_timeout)
    local method = resolve_http_method(spec)
    local payload = build_payload(spec)
    local body_chunks = {}
    local header_chunks = {}
    local output_handle = nil

    local final_url = spec.url
    for _, fragment in ipairs(spec.query_parts) do
        if fragment and fragment ~= "" then
            final_url = append_query_fragment(final_url, fragment)
        end
    end

    local easy_options = {
        url = final_url,
        followlocation = spec.follow_location,
        ssl_verifypeer = not spec.insecure,
        ssl_verifyhost = not spec.insecure,
        httpheader = spec.headers,
    }

    if spec.userpwd then
        easy_options.userpwd = spec.userpwd
    end
    if spec.useragent then
        easy_options.useragent = spec.useragent
    end
    if spec.referer then
        easy_options.referer = spec.referer
    end
    if spec.proxy then
        easy_options.proxy = spec.proxy
    end
    if spec.proxy_userpwd then
        easy_options.proxyuserpwd = spec.proxy_userpwd
    end
    if spec.cookie then
        easy_options.cookie = spec.cookie
    end
    if spec.cookie_file then
        easy_options.cookiefile = spec.cookie_file
    end
    if spec.cookie_jar then
        easy_options.cookiejar = spec.cookie_jar
    end
    if spec.cacert then
        easy_options.cainfo = spec.cacert
    end
    if spec.capath then
        easy_options.capath = spec.capath
    end
    if spec.cert then
        easy_options.cert = spec.cert
    end
    if spec.key then
        easy_options.sslkey = spec.key
    end
    if spec.compressed then
        easy_options.accept_encoding = ""
    end
    if spec.connect_timeout_seconds then
        easy_options.connecttimeout = spec.connect_timeout_seconds
    end
    if spec.timeout_seconds then
        easy_options.timeout = spec.timeout_seconds
    elseif default_timeout and default_timeout > 0 then
        easy_options.timeout = math.max(1, math.floor((default_timeout + 999) / 1000))
    end

    if method == "HEAD" then
        easy_options.nobody = true
    elseif method == "GET" then
        easy_options.httpget = true
    elseif method == "POST" and payload ~= "" and #spec.forms == 0 then
        easy_options.post = true
        easy_options.postfields = payload
    elseif method ~= "POST" then
        easy_options.customrequest = method
        if payload ~= "" and #spec.forms == 0 then
            easy_options.postfields = payload
        end
    end

    local easy, easy_error = curl.easy(easy_options)
    if not easy then
        error("Failed to create curl easy handle: " .. tostring(easy_error))
    end

    apply_native_ca_preferences(curl, easy, spec)

    local ok, err = easy:setopt_headerfunction(table.insert, header_chunks)
    if not ok then
        easy:close()
        error("Failed to register header collector: " .. tostring(err))
    end

    if spec.output_path then
        output_handle = io.open(spec.output_path, "wb")
        if not output_handle then
            easy:close()
            error("Failed to open curl output file: " .. tostring(spec.output_path))
        end

        ok, err = easy:setopt_writefunction(output_handle)
        if not ok then
            output_handle:close()
            easy:close()
            error("Failed to register curl output file writer: " .. tostring(err))
        end
    else
        ok, err = easy:setopt_writefunction(table.insert, body_chunks)
        if not ok then
            easy:close()
            error("Failed to register body collector: " .. tostring(err))
        end
    end

    local form = apply_form_payload(curl, easy, spec)

    return {
        easy = easy,
        form = form,
        output_handle = output_handle,
        body_chunks = body_chunks,
        header_chunks = header_chunks,
        method = method,
        url = final_url,
    }
end

-- Close all allocated curl and file resources safely.
-- 安全关闭所有已分配的 curl 与文件资源。
local function close_request_resources(bundle)
    if bundle.output_handle then
        bundle.output_handle:close()
    end
    if bundle.form then
        bundle.form:free()
    end
    if bundle.easy then
        bundle.easy:close()
    end
end

-- Perform one parsed curl request with optional retry behavior.
-- 按解析后的规格执行一次 curl 请求，并可选进行重试。
local function perform_request(spec, base_dir, default_timeout)
    local curl = require("lcurl.safe")
    local socket = require("socket")
    local method = resolve_http_method(spec)
    local started_at = os.time()
    local last_result = nil

    local max_attempts = math.max(1, tonumber(spec.retries or 0) + 1)
    for attempt = 1, max_attempts do
        local bundle = create_easy_handle(curl, spec, base_dir, default_timeout)
        local ok, err = bundle.easy:perform()
        local code = bundle.easy:getinfo_response_code()
        local effective_url = bundle.easy:getinfo_effective_url()
        local headers_text = table.concat(bundle.header_chunks)
        local body_text = bundle.output_handle and nil or table.concat(bundle.body_chunks)
        close_request_resources(bundle)

        if spec.dump_header_path and headers_text and headers_text ~= "" then
            write_text_file(spec.dump_header_path, headers_text)
        end

        last_result = {
            ok = ok and true or false,
            error = err and tostring(err) or nil,
            status_code = code,
            effective_url = effective_url,
            headers_text = headers_text,
            body_text = body_text,
            method = method,
            request_url = bundle.url,
            output_path = spec.output_path,
            dump_header_path = spec.dump_header_path,
            attempt = attempt,
        }

        local should_fail_for_http = spec.fail_on_http_error and type(code) == "number" and code >= 400
        local retryable_transport = not last_result.ok
        local retryable_http = should_fail_for_http and code >= 500
        local can_retry = attempt < max_attempts

        if not can_retry or (not retryable_transport and not retryable_http) then
            return last_result
        end

        if spec.retry_max_time_seconds then
            local elapsed = os.time() - started_at
            if elapsed >= spec.retry_max_time_seconds then
                return last_result
            end
        end

        if spec.retry_delay_seconds and spec.retry_delay_seconds > 0 then
            socket.sleep(spec.retry_delay_seconds)
        end
    end

    return last_result
end

-- Render one request result into a stable Markdown success response.
-- 将请求结果渲染为稳定的 Markdown 成功响应。
local function render_success_markdown(result, spec)
    local lines = {
        "# Curl Result",
        "",
        "## Summary",
        "- status_code: `" .. tostring(result.status_code or "unknown") .. "`",
        "- attempt: `" .. tostring(result.attempt or 1) .. "`",
    }

    if result.output_path then
        lines[#lines + 1] = "- output_file: `" .. tostring(result.output_path) .. "`"
    end
    if result.dump_header_path then
        lines[#lines + 1] = "- header_file: `" .. tostring(result.dump_header_path) .. "`"
    end

    if has_render_flag(spec, "request_header") then
        lines[#lines + 1] = ""
        lines[#lines + 1] = "## Request"
        lines[#lines + 1] = "- method: `" .. tostring(result.method) .. "`"
        lines[#lines + 1] = "- url: `" .. tostring(result.request_url) .. "`"

        if result.effective_url and result.effective_url ~= "" then
            lines[#lines + 1] = "- effective_url: `" .. tostring(result.effective_url) .. "`"
        end
    end

    if has_render_flag(spec, "response_header") and result.headers_text and result.headers_text ~= "" then
        lines[#lines + 1] = ""
        lines[#lines + 1] = "## Response Headers"
        lines[#lines + 1] = result.headers_text
    end

    if result.body_text and result.body_text ~= "" then
        lines[#lines + 1] = ""
        lines[#lines + 1] = "## Response Body"
        lines[#lines + 1] = result.body_text
    end

    return table.concat(lines, "\n")
end

-- Render one request result into a stable Markdown error response.
-- 将请求结果渲染为稳定的 Markdown 错误响应。
local function render_error_markdown(message, result, spec)
    local lines = {
        "# Curl Error",
        "",
        "## Error",
        tostring(message),
    }

    if result then
        lines[#lines + 1] = ""
        lines[#lines + 1] = "## Summary"
        lines[#lines + 1] = "- status_code: `" .. tostring(result.status_code or "unknown") .. "`"
        lines[#lines + 1] = "- attempt: `" .. tostring(result.attempt or 1) .. "`"

        if result.output_path then
            lines[#lines + 1] = "- output_file: `" .. tostring(result.output_path) .. "`"
        end
        if result.dump_header_path then
            lines[#lines + 1] = "- header_file: `" .. tostring(result.dump_header_path) .. "`"
        end

        if has_render_flag(spec, "request_header") then
            lines[#lines + 1] = ""
            lines[#lines + 1] = "## Request"
            lines[#lines + 1] = "- method: `" .. tostring(result.method or "unknown") .. "`"
            lines[#lines + 1] = "- url: `" .. tostring(result.request_url or "unknown") .. "`"

            if result.effective_url and result.effective_url ~= "" then
                lines[#lines + 1] = "- effective_url: `" .. tostring(result.effective_url) .. "`"
            end
        end

        if has_render_flag(spec, "response_header") and result.headers_text and result.headers_text ~= "" then
            lines[#lines + 1] = ""
            lines[#lines + 1] = "## Response Headers"
            lines[#lines + 1] = result.headers_text
        end

        if result.body_text and result.body_text ~= "" then
            lines[#lines + 1] = ""
            lines[#lines + 1] = "## Response Body"
            lines[#lines + 1] = result.body_text
        end
    end

    return table.concat(lines, "\n")
end

-- Execute one parsed curl request end-to-end and return one Markdown string.
-- 端到端执行一次解析后的 curl 请求，并返回一个 Markdown 字符串。
local function execute_parsed_request(spec, base_dir, timeout_ms)
    local result = perform_request(spec, base_dir, timeout_ms or default_timeout_ms())
    local should_fail_for_http = spec.fail_on_http_error and type(result.status_code) == "number" and result.status_code >= 400

    if not result.ok then
        return render_error_markdown(result.error or "curl perform failed", result, spec)
    end

    if should_fail_for_http then
        local error_text = "HTTP request failed with status " .. tostring(result.status_code)
        if not spec.fail_with_body then
            result.body_text = nil
        end
        return render_error_markdown(error_text, result, spec)
    end

    return render_success_markdown(result, spec)
end

-- Execute one curl-style argv request and return one Markdown string.
-- 执行一次 curl 风格参数请求并返回一个 Markdown 字符串。
local function execute_curl_args_request(args)
    if type(args) ~= "table" then
        error("vulcan-curl expects a table input")
    end

    local base_dir = tostring(args.cwd or resolve_runtime_cwd())
    local argv = normalize_argv(args.args)
    local spec = parse_curl_argv(argv, base_dir)
    spec.render_flags = normalize_render_flags(args.flags)
    if spec.include then
        spec.render_flags.response_header = true
    end
    return execute_parsed_request(spec, base_dir, tonumber(args.timeout_ms) or default_timeout_ms())
end

-- Append raw header lines from one explicit string array.
-- 从显式字符串数组追加原始请求头行。
local function append_header_lines(target, header_lines)
    if header_lines == nil then
        return
    end

    if type(header_lines) ~= "table" then
        error("header_lines must be an array")
    end

    for _, value in ipairs(header_lines) do
        if type(value) ~= "string" then
            error("header_lines items must be strings")
        end
        target[#target + 1] = value
    end
end

-- Normalize one simple header object plus optional raw array into curl header lines.
-- 将简单请求头对象与可选原始数组归一化为 curl 头行列表。
local function normalize_simple_headers(headers, header_lines)
    local normalized = {}

    if headers ~= nil then
        if type(headers) ~= "table" then
            error("headers must be an object or array")
        end

        local array_count = 0
        for _, value in ipairs(headers) do
            array_count = array_count + 1
            if type(value) ~= "string" then
                error("header array items must be strings")
            end
            normalized[#normalized + 1] = value
        end

        local has_named_entries = false
        for key, value in pairs(headers) do
            if type(key) ~= "number" then
                has_named_entries = true
                if type(value) ~= "string" and type(value) ~= "number" and type(value) ~= "boolean" then
                    error("header object values must be scalar")
                end
                normalized[#normalized + 1] = tostring(key) .. ": " .. tostring(value)
            end
        end

        if array_count > 0 and has_named_entries then
            -- Mixed array/object input is accepted by concatenating both forms.
            -- 混合数组/对象输入允许同时存在，并按两种形式拼接。
        end
    end

    append_header_lines(normalized, header_lines)
    return normalized
end

-- Normalize one quick basic-auth input into the libcurl userpwd format.
-- 将快捷 basic-auth 输入归一化为 libcurl 的 userpwd 格式。
local function normalize_basic_auth_value(basic)
    if basic == nil then
        return nil
    end

    if type(basic) == "string" then
        if basic == "" then
            error("basic auth string must not be empty")
        end
        return basic
    end

    if type(basic) ~= "table" then
        error("basic auth must be a string or object")
    end

    local username = basic.username or basic.user
    local password = basic.password or basic.pass
    if username == nil or password == nil then
        error("basic auth object requires username and password")
    end

    return tostring(username) .. ":" .. tostring(password)
end

local function apply_quick_auth(spec, args)
    if type(spec) ~= "table" or type(args) ~= "table" then
        return
    end

    local has_auth_header = has_header(spec.headers or {}, "Authorization")
    local bearer = args.bearer
    local basic = args.basic
    local basic_text = args.basic_text

    if basic ~= nil and basic_text ~= nil then
        error("basic and basic_text cannot be used together")
    end

    if bearer ~= nil and (basic ~= nil or basic_text ~= nil) and not has_auth_header then
        error("bearer and basic cannot be used together unless Authorization header is explicit")
    end

    if has_auth_header then
        return
    end

    if bearer ~= nil then
        if type(bearer) ~= "string" or bearer == "" then
            error("bearer must be a non-empty string")
        end
        spec.headers[#spec.headers + 1] = "Authorization: Bearer " .. bearer
        return
    end

    if basic ~= nil or basic_text ~= nil then
        spec.userpwd = normalize_basic_auth_value(basic ~= nil and basic or basic_text)
    end
end

-- Append query params from one explicit string array into the target fragment list.
-- 将显式字符串数组形式的查询参数追加到目标查询片段列表。
local function append_query_param_lines(target, param_lines)
    if param_lines == nil then
        return
    end

    if type(param_lines) ~= "table" then
        error("params_list must be an array")
    end

    for _, value in ipairs(param_lines) do
        if type(value) ~= "string" then
            error("params_list items must be strings")
        end
        target[#target + 1] = value
    end
end

-- Append query params from one object or array plus optional explicit array.
-- 将对象或数组形式的查询参数及可选显式数组追加到目标查询片段列表。
local function append_query_params(target, params, params_list)
    if params ~= nil then
        if type(params) ~= "table" then
            error("params must be an object or array")
        end

        for _, value in ipairs(params) do
            if type(value) ~= "string" then
                error("params array items must be strings")
            end
            target[#target + 1] = value
        end

        for key, value in pairs(params) do
            if type(key) ~= "number" then
                if type(value) == "table" then
                    error("nested params are not supported in quick tools")
                end
                target[#target + 1] = tostring(key) .. "=" .. url_encode(value)
            end
        end
    end

    append_query_param_lines(target, params_list)
end

-- Append form fields from one explicit string array into the target multipart list.
-- 将显式字符串数组形式的表单字段追加到目标 multipart 列表。
local function append_form_lines(target, form_lines, base_dir)
    if form_lines == nil then
        return
    end

    if type(form_lines) ~= "table" then
        error("form_lines must be an array")
    end

    for _, value in ipairs(form_lines) do
        if type(value) ~= "string" then
            error("form_lines items must be strings")
        end
        target[#target + 1] = parse_form_token(base_dir, value)
    end
end

-- Append simple form fields from one object or array plus optional explicit array.
-- 将对象或数组形式的简单表单字段及可选显式数组追加到目标列表。
local function append_form_entries(target, form, form_lines, base_dir)
    if form ~= nil then
        if type(form) ~= "table" then
            error("form must be an object or array")
        end

        for _, value in ipairs(form) do
            if type(value) ~= "string" then
                error("form array items must be strings")
            end
            target[#target + 1] = parse_form_token(base_dir, value)
        end

        for key, value in pairs(form) do
            if type(key) ~= "number" then
                if type(value) == "table" then
                    error("nested form values are not supported")
                end
                target[#target + 1] = {
                    name = tostring(key),
                    kind = "content",
                    value = tostring(value),
                    mime_type = nil,
                    filename = nil,
                }
            end
        end
    end

    append_form_lines(target, form_lines, base_dir)
end

-- Append file entries from one explicit string array into the target multipart list.
-- 将显式字符串数组形式的文件字段追加到目标 multipart 列表。
local function append_file_lines(target, file_lines, base_dir)
    if file_lines == nil then
        return
    end

    if type(file_lines) ~= "table" then
        error("file_lines must be an array")
    end

    for _, value in ipairs(file_lines) do
        if type(value) ~= "string" then
            error("file_lines items must be strings")
        end
        target[#target + 1] = parse_form_token(base_dir, value)
    end
end

-- Append file form entries from one object or array plus optional explicit array.
-- 将对象或数组形式的文件字段及可选显式数组追加到目标表单列表。
local function append_file_entries(target, files, file_lines, base_dir)
    if files ~= nil then
        if type(files) ~= "table" then
            error("files must be an object or array")
        end

        for _, value in ipairs(files) do
            if type(value) ~= "string" then
                error("files array items must be strings")
            end
            target[#target + 1] = parse_form_token(base_dir, value)
        end

        for key, value in pairs(files) do
            if type(key) ~= "number" then
                if type(value) ~= "string" then
                    error("files object values must be file paths")
                end
                target[#target + 1] = {
                    name = tostring(key),
                    kind = "file",
                    value = resolve_local_path(base_dir, value),
                    mime_type = nil,
                    filename = nil,
                }
            end
        end
    end

    append_file_lines(target, file_lines, base_dir)
end

-- Build one simple GET request specification for AI-friendly usage.
-- 构建一个面向 AI 友好使用的简单 GET 请求规格。
local function build_get_spec(args, base_dir)
    if type(args) ~= "table" then
        error("vulcan-curl-get expects a table input")
    end

    if type(args.url) ~= "string" or args.url == "" then
        error("vulcan-curl-get requires url")
    end

    local spec = {
        url = args.url,
        request_method = "GET",
        headers = normalize_simple_headers(args.headers, args.header_lines),
        body_parts = {},
        query_parts = {},
        forms = {},
        follow_location = args.follow_location == true,
        insecure = args.insecure == true,
        head_only = false,
        http_get = true,
        output_path = args.download_to and resolve_local_path(base_dir or resolve_runtime_cwd(), args.download_to) or nil,
        dump_header_path = args.save_headers_to and resolve_local_path(base_dir or resolve_runtime_cwd(), args.save_headers_to) or nil,
        userpwd = nil,
        useragent = nil,
        referer = nil,
        cookie = nil,
        cookie_file = nil,
        cookie_jar = nil,
        proxy = nil,
        proxy_userpwd = nil,
        cacert = nil,
        capath = nil,
        cert = nil,
        key = nil,
        timeout_seconds = nil,
        connect_timeout_seconds = nil,
        compressed = args.compressed == true,
        fail_on_http_error = args.fail_on_http_error == true,
        fail_with_body = args.fail_with_body == true,
        retries = 0,
        retry_delay_seconds = 0,
        retry_max_time_seconds = nil,
        http_version = nil,
        silent = false,
        include = args.include_headers == true,
        render_flags = build_render_flags(args),
    }

    append_query_params(spec.query_parts, args.params, args.params_list)
    apply_quick_auth(spec, args)
    return spec
end

-- Build one simple POST request specification for AI-friendly usage.
-- 构建一个面向 AI 友好使用的简单 POST 请求规格。
local function build_post_like_spec(args, base_dir, method_name)
    if type(args) ~= "table" then
        error("post-like curl tool expects a table input")
    end

    if type(args.url) ~= "string" or args.url == "" then
        error("post-like curl tool requires url")
    end

    local payload_mode_count = 0
    if args.json ~= nil then
        payload_mode_count = payload_mode_count + 1
    end
    if args.form ~= nil or args.form_lines ~= nil then
        payload_mode_count = payload_mode_count + 1
    end
    if args.body ~= nil then
        payload_mode_count = payload_mode_count + 1
    end
    if args.files ~= nil or args.file_lines ~= nil then
        payload_mode_count = payload_mode_count + 1
    end

    if (args.files ~= nil or args.file_lines ~= nil) and (args.form ~= nil or args.form_lines ~= nil) then
        payload_mode_count = payload_mode_count - 1
    end

    if payload_mode_count > 1 then
        error("post-like curl tool supports only one payload family among json, body, or form/files")
    end

    local spec = {
        url = args.url,
        request_method = tostring(method_name or "POST"),
        headers = normalize_simple_headers(args.headers, args.header_lines),
        body_parts = {},
        query_parts = {},
        forms = {},
        follow_location = args.follow_location == true,
        insecure = args.insecure == true,
        head_only = false,
        http_get = false,
        output_path = args.download_to and resolve_local_path(base_dir or resolve_runtime_cwd(), args.download_to) or nil,
        dump_header_path = args.save_headers_to and resolve_local_path(base_dir or resolve_runtime_cwd(), args.save_headers_to) or nil,
        userpwd = nil,
        useragent = nil,
        referer = nil,
        cookie = nil,
        cookie_file = nil,
        cookie_jar = nil,
        proxy = nil,
        proxy_userpwd = nil,
        cacert = nil,
        capath = nil,
        cert = nil,
        key = nil,
        timeout_seconds = nil,
        connect_timeout_seconds = nil,
        compressed = args.compressed == true,
        fail_on_http_error = args.fail_on_http_error == true,
        fail_with_body = args.fail_with_body == true,
        retries = 0,
        retry_delay_seconds = 0,
        retry_max_time_seconds = nil,
        http_version = nil,
        silent = false,
        include = args.include_headers == true,
        render_flags = build_render_flags(args),
    }

    append_query_params(spec.query_parts, args.params, args.params_list)

    if args.json ~= nil then
        local json_encode = vulcan and vulcan.json and vulcan.json.encode
        if type(json_encode) ~= "function" then
            error("Host JSON encoder is unavailable")
        end
        spec.body_parts[#spec.body_parts + 1] = json_encode(args.json)
        if not has_header(spec.headers, "Content-Type") then
            spec.headers[#spec.headers + 1] = "Content-Type: application/json"
        end
        if not has_header(spec.headers, "Accept") then
            spec.headers[#spec.headers + 1] = "Accept: application/json"
        end
    elseif args.form ~= nil then
        append_form_entries(spec.forms, args.form, args.form_lines, base_dir or resolve_runtime_cwd())
        append_file_entries(spec.forms, args.files, args.file_lines, base_dir or resolve_runtime_cwd())
    elseif args.files ~= nil then
        append_file_entries(spec.forms, args.files, args.file_lines, base_dir or resolve_runtime_cwd())
    elseif args.body ~= nil then
        spec.body_parts[#spec.body_parts + 1] = tostring(args.body)
    elseif args.form_lines ~= nil then
        append_form_entries(spec.forms, nil, args.form_lines, base_dir or resolve_runtime_cwd())
        append_file_entries(spec.forms, args.files, args.file_lines, base_dir or resolve_runtime_cwd())
    elseif args.file_lines ~= nil then
        append_file_entries(spec.forms, args.files, args.file_lines, base_dir or resolve_runtime_cwd())
    end

    apply_quick_auth(spec, args)
    return spec
end

-- Build one simple POST request specification for AI-friendly usage.
-- 构建一个面向 AI 友好使用的简单 POST 请求规格。
local function build_post_spec(args, base_dir)
    return build_post_like_spec(args, base_dir, "POST")
end

return {
    apply_quick_auth = apply_quick_auth,
    append_query_fragment = append_query_fragment,
    build_get_spec = build_get_spec,
    build_post_spec = build_post_spec,
    default_timeout_ms = default_timeout_ms,
    execute_curl_args_request = execute_curl_args_request,
    execute_parsed_request = execute_parsed_request,
    normalize_argv = normalize_argv,
    parse_curl_argv = parse_curl_argv,
    render_error_markdown = render_error_markdown,
    resolve_runtime_cwd = resolve_runtime_cwd,
}
