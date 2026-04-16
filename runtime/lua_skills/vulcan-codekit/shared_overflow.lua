--[[
shared_overflow
中文：为 vulcan-codekit 提供统一的大结果溢出协议；超限时不再返回残缺正文，而是返回 raw file 指针与安全分块读取计划。
English: Provide a shared large-result overflow protocol for vulcan-codekit. When content exceeds the safe inline budget, omit the body and return a raw-file pointer plus a safe chunked read plan.
]]

local SAFE_INLINE_RATIO = 0.95
local CHUNK_WARNING_THRESHOLD = 24
local MAX_RENDERED_CHUNKS = 64

--[[
中文：清理字符串首尾空白，保证指针块与补充摘要拼接稳定。
English: Trim leading and trailing whitespace so pointer blocks and summary lines remain stable.
]]
local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

--[[
中文：统一正文换行风格，避免 Windows CRLF 影响逐行字节统计与行号切分。
English: Normalize line endings before byte counting and chunk planning so CRLF does not skew line-based slicing.
]]
local function normalize_text(content)
    return tostring(content or ""):gsub("\r\n", "\n")
end

--[[
中文：按行拆分文本，保留空行，供 chunk 规划时按稳定行边界切分。
English: Split text into lines while preserving empty lines so chunk planning can cut on stable line boundaries.
]]
local function split_lines(content)
    local normalized = normalize_text(content)
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
中文：根据客户端标称预算计算实际安全内联预算，默认预留 5% 作为链路与包装冗余保护。
English: Convert the nominal client budget into the actual safe inline budget, reserving 5% for transport and wrapping overhead.
]]
local function compute_safe_inline_limit(client_char_limit)
    local numeric_limit = math.max(1, tonumber(client_char_limit) or 1)
    local safe_limit = math.floor(numeric_limit * SAFE_INLINE_RATIO)
    if safe_limit < 1 then
        safe_limit = 1
    end
    return safe_limit
end

--[[
中文：按 UTF-8 字节预算生成逐行 chunk 计划，并同时记录宿主 `Read(offset, limit)` 可直接使用的 offset/limit。
English: Build a line-based chunk plan within the UTF-8 byte budget and also record host-ready `Read(offset, limit)` values.
]]
local function build_chunk_plan(content, safe_limit_bytes)
    local normalized = normalize_text(content)
    local lines = split_lines(normalized)
    local chunks = {}
    local current_start = nil
    local current_bytes = 0
    local current_line_count = 0
    local oversized_line_count = 0
    local total_lines = #lines

    local function flush_chunk(end_line)
        if current_start == nil or current_line_count <= 0 then
            return
        end
        table.insert(chunks, {
            offset = current_start - 1,
            limit = current_line_count,
            start_line = current_start,
            end_line = end_line,
            line_count = current_line_count,
            byte_count = current_bytes,
        })
        current_start = nil
        current_bytes = 0
        current_line_count = 0
    end

    for index, line in ipairs(lines) do
        local line_bytes = #tostring(line or "")
        if index < total_lines then
            line_bytes = line_bytes + 1
        end
        if line_bytes > safe_limit_bytes then
            oversized_line_count = oversized_line_count + 1
        end
        if current_start ~= nil and current_bytes + line_bytes > safe_limit_bytes then
            flush_chunk(index - 1)
        end
        if current_start == nil then
            current_start = index
        end
        current_bytes = current_bytes + line_bytes
        current_line_count = current_line_count + 1
    end

    flush_chunk(total_lines)

    return {
        chunks = chunks,
        chunk_count = #chunks,
        total_lines = total_lines,
        total_bytes = #normalized,
        safe_limit_bytes = safe_limit_bytes,
        oversized_line_count = oversized_line_count,
    }
end

--[[
中文：把附加摘要行标准化为 Markdown 列表项，避免调用方重复关心前缀格式。
English: Normalize extra summary lines into Markdown bullets so callers do not need to manage list prefixes themselves.
]]
local function append_summary_lines(lines, summary_lines)
    local normalized_items = {}
    for _, item in ipairs(summary_lines or {}) do
        local text = trim(item)
        if text ~= "" then
            if text:match("^%- ") then
                table.insert(normalized_items, text)
            else
                table.insert(normalized_items, "- " .. text)
            end
        end
    end
    if #normalized_items == 0 then
        return
    end
    table.insert(lines, "")
    table.insert(lines, "## Optional Summary")
    for _, item in ipairs(normalized_items) do
        table.insert(lines, item)
    end
end

--[[
中文：渲染 overflow pointer 响应文本；当 chunk 数量过大时，明确提示先收窄范围。
English: Render the overflow pointer response; if the chunk count is too large, explicitly advise narrowing the scope first.
]]
local function render_pointer_text(raw_file_path, chunk_plan, summary_lines)
    local lines = {
        "# LARGE RESULT POINTER",
        "",
        "## Status",
        "- mode: overflow-raw-file",
        "- inline_body: omitted",
        "- reason: host-inline-truncation-unreliable",
        "",
        "## Raw File",
        "- raw_file: " .. tostring(raw_file_path or ""),
        "- format: markdown",
        "- encoding: utf-8",
        string.format("- total_bytes: %d", tonumber(chunk_plan.total_bytes) or 0),
        string.format("- total_lines: %d", tonumber(chunk_plan.total_lines) or 0),
        string.format("- safe_inline_limit_bytes: %d", tonumber(chunk_plan.safe_limit_bytes) or 0),
        string.format("- safe_read_limit_bytes: %d", tonumber(chunk_plan.safe_limit_bytes) or 0),
        string.format("- chunk_count: %d", tonumber(chunk_plan.chunk_count) or 0),
    }

    append_summary_lines(lines, summary_lines)

    table.insert(lines, "")
    table.insert(lines, "## Host Safe Read Chunks")
    if chunk_plan.chunk_count > MAX_RENDERED_CHUNKS then
        table.insert(lines, "- chunk_table: omitted")
        table.insert(lines, string.format("- rendered_chunks: %d", MAX_RENDERED_CHUNKS))
        table.insert(lines, "- reason: too_many_chunks")
    else
        for index, chunk in ipairs(chunk_plan.chunks or {}) do
            table.insert(
                lines,
                string.format(
                    "- read_%02d: offset=%d, limit=%d, start_line=%d, end_line=%d, bytes=%d",
                    index,
                    tonumber(chunk.offset) or 0,
                    tonumber(chunk.limit) or 0,
                    tonumber(chunk.start_line) or 0,
                    tonumber(chunk.end_line) or 0,
                    tonumber(chunk.byte_count) or 0
                )
            )
        end
    end

    table.insert(lines, "")
    table.insert(lines, "## Read Strategy")
    table.insert(lines, "- Read the raw_file directly.")
    table.insert(lines, "- The host Read tool expects offset as 0-based line index and limit as line count.")
    table.insert(lines, "- Use offset + limit directly from the chunk table whenever possible.")
    table.insert(lines, "- Do not rely on host-wrapped MCP transcript/cache for line-based reading.")

    if chunk_plan.chunk_count > CHUNK_WARNING_THRESHOLD or (chunk_plan.oversized_line_count or 0) > 0 then
        table.insert(lines, "")
        table.insert(lines, "## Scope Advice")
        table.insert(lines, "- This result is too large for reliable whole-result consumption in a single pass.")
        table.insert(lines, "- Narrow the directory scope, ext filter, or regex signal before continuing.")
        if (chunk_plan.oversized_line_count or 0) > 0 then
            table.insert(lines, string.format("- oversized_line_count: %d", tonumber(chunk_plan.oversized_line_count) or 0))
        end
    end

    return table.concat(lines, "\n")
end

--[[
中文：统一完成大结果输出；未超限时直接返回正文，超限时写入原始文件并返回指针块。
English: Finalize large-result output uniformly; return inline content when safe, otherwise write the raw file and return a pointer block.
]]
local function finalize_large_result(options)
    local normalized = normalize_text(options and options.content or "")
    local safe_limit_bytes = compute_safe_inline_limit(options and options.client_char_limit)
    if #normalized <= safe_limit_bytes then
        return normalized
    end

    local output_directory, output_directory_error = options.resolve_large_result_directory()
    if output_directory_error then
        return output_directory_error
    end

    local file_id = options.build_spill_file_id(options.file_prefix or "codekit_result")
    local full_output_path = vulcan.path_join(output_directory, file_id .. ".md")
    local _, write_error = options.write_text_file(full_output_path, normalized)
    if write_error then
        return write_error
    end

    local chunk_plan = build_chunk_plan(normalized, safe_limit_bytes)
    return render_pointer_text(full_output_path, chunk_plan, options.summary_lines or {})
end

return {
    normalize_text = normalize_text,
    split_lines = split_lines,
    compute_safe_inline_limit = compute_safe_inline_limit,
    build_chunk_plan = build_chunk_plan,
    render_pointer_text = render_pointer_text,
    finalize_large_result = finalize_large_result,
}
