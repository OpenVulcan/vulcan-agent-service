--[[
Project-scoped AI working memory with compact Markdown output.
项目级 AI 工作记忆，输出紧凑 Markdown。
]]

-- Maximum accepted node content length keeps memory entries useful without becoming log storage.
-- 最大节点内容长度让记忆条目保持有用，避免变成日志仓库。
local MAX_CONTENT_CHARS = 2000

-- Maximum accepted task detail length keeps task metadata compact.
-- 最大任务说明长度让任务元数据保持紧凑。
local MAX_DETAIL_CHARS = 1200

-- Maximum accepted node title length keeps list output readable.
-- 最大节点标题长度让列表输出保持可读。
local MAX_TITLE_CHARS = 160

-- Minimum caller-provided LUASKILL_SID length prevents accidental tiny anchors from being persisted.
-- 调用方传入的 LUASKILL_SID 最小长度用于避免意外持久化过短锚点。
local MIN_LUASKILL_SID_CHARS = 20

-- Maximum caller-provided LUASKILL_SID length keeps stable anchors compact and readable.
-- 调用方传入的 LUASKILL_SID 最大长度让稳定锚点保持紧凑且可读。
local MAX_LUASKILL_SID_CHARS = 128

-- Maximum nodes accepted in one set call bounds write volume and tool result summaries.
-- 单次 set 允许的最大节点数量用于限制写入量和工具结果摘要。
local MAX_BATCH_NODES = 30

-- Host-managed IDs are intentionally hidden from assistant-visible output.
-- 宿主托管 ID 会从面向助手的输出中隐藏。
local HOST_MANAGED_PREFIX = "LUASKILLS-SID-"

-- Legacy plugin bridge IDs remain recognized during the contract migration window.
-- 契约迁移窗口内仍继续识别旧插件桥接 ID。
local LEGACY_PLUGIN_MANAGED_PREFIX = "VMCP_PLUGINS_"

-- Supported action names intentionally use hyphen style because they double as AI-facing commands.
-- 支持的 action 名称故意使用短横线风格，因为它们同时是面向 AI 的命令。
local ACTIONS = {
    ["task-create"] = true,
    ["task-list"] = true,
    ["task-close"] = true,
    set = true,
    list = true,
    get = true,
    ["get-all"] = true,
    del = true,
}

-- Supported node types keep retrieval and list filters predictable.
-- 支持的节点类型让召回和列表过滤保持可预测。
local NODE_TYPES = {
    note = true,
    file_summary = true,
    decision = true,
    todo = true,
    progress = true,
    risk = true,
    checkpoint = true,
    tool_result = true,
}

-- Random generator state prevents repeated seeding inside one VM.
-- 随机数生成器状态防止同一个 VM 内重复播种。
local RANDOM_SEEDED = false

--- Trim surrounding whitespace from one value.
--- 去除一个值两侧的空白字符。
--- @param value any Value to trim.
--- @return string
local function trim(value)
    return tostring(value or ""):gsub("^%s+", ""):gsub("%s+$", "")
end

--- Convert one value to lower-case text.
--- 将一个值转换为小写文本。
--- @param value any Value to convert.
--- @return string
local function lower_text(value)
    return string.lower(trim(value))
end

--- Return current UTC timestamp in a stable sortable form.
--- 返回稳定且可排序的当前 UTC 时间戳。
--- @return string
local function now_utc()
    return os.date("!%Y-%m-%dT%H:%M:%SZ")
end

--- Escape one string for static SQL snippets.
--- 为静态 SQL 片段转义一个字符串。
--- @param value any Value to quote.
--- @return string
local function sql_quote(value)
    return "'" .. tostring(value or ""):gsub("'", "''") .. "'"
end

--- Limit one string to a maximum byte length for compact output.
--- 将一个字符串限制到最大字节长度以保持输出紧凑。
--- @param value any Value to shorten.
--- @param limit number Maximum length.
--- @return string
local function shorten(value, limit)
    local text = trim(value)
    if #text <= limit then
        return text
    end
    return text:sub(1, math.max(0, limit - 3)) .. "..."
end

--- Normalize one value to a single display line.
--- 将一个值归一化为单行展示文本。
--- @param value any Value to normalize.
--- @return string
local function single_line(value)
    local text = trim(value)
    text = text:gsub("[%c]+", " ")
    text = text:gsub("%s+", " ")
    return trim(text)
end

--- Render one value as inline Markdown code.
--- 将一个值渲染为 Markdown 行内代码。
--- @param value any Value to render.
--- @return string
local function inline_code(value)
    local text = tostring(value or ""):gsub("`", "'")
    return "`" .. text .. "`"
end

--- Return true when the LUASKILL_SID is provided by a trusted host-managed bridge.
--- 当 LUASKILL_SID 由可信宿主管理桥接提供时返回 true。
--- @param luaskill_sid string LuaSkills managed identity.
--- @return boolean
local function is_host_managed(luaskill_sid)
    local text = tostring(luaskill_sid or "")
    return text:sub(1, #HOST_MANAGED_PREFIX) == HOST_MANAGED_PREFIX
        or text:sub(1, #LEGACY_PLUGIN_MANAGED_PREFIX) == LEGACY_PLUGIN_MANAGED_PREFIX
end

--- Render a LUASKILL_SID while hiding host-managed secret-like values.
--- 渲染 LUASKILL_SID，同时隐藏宿主托管的类密钥值。
--- @param luaskill_sid string LuaSkills managed identity.
--- @return string
local function display_luaskill_sid(luaskill_sid)
    if is_host_managed(luaskill_sid) then
        return "`host-managed`"
    end
    return inline_code(luaskill_sid)
end

--- Seed Lua's pseudo-random generator with lightweight local entropy.
--- 使用轻量本地熵为 Lua 伪随机生成器播种。
local function seed_random()
    if RANDOM_SEEDED then
        return
    end

    local entropy = os.time()
    local address_hex = tostring({}):match("0x(%x+)")
    if address_hex then
        entropy = entropy + tonumber(address_hex:sub(-6), 16)
    end
    math.randomseed(entropy)
    RANDOM_SEEDED = true
end

--- Generate a random hexadecimal string with the requested length.
--- 生成指定长度的随机十六进制字符串。
--- @param length number Hex character count.
--- @return string
local function random_hex(length)
    seed_random()
    local parts = {}
    for index = 1, length do
        parts[index] = string.format("%x", math.random(0, 15))
    end
    return table.concat(parts)
end

--- Generate a compact random LUASKILL_SID with the public VWM prefix.
--- 生成带公开 VWM 前缀的紧凑随机 LUASKILL_SID。
--- @return string
local function generate_luaskill_sid()
    seed_random()
    return "vwm_" .. random_hex(32)
end

--- Extract result rows from the host SQLite query_json variants.
--- 从宿主 SQLite query_json 的不同返回形态中提取行数据。
--- @param result any SQLite query result.
--- @return table
local function rows_from_query(result)
    if type(result) == "string" then
        local ok, decoded = pcall(vulcan.json.decode, result)
        if ok then
            return rows_from_query(decoded)
        end
        return {}
    end

    if type(result) ~= "table" then
        return {}
    end

    if type(result.rows) == "table" then
        return result.rows
    end
    if type(result.data) == "table" then
        return result.data
    end
    if type(result.items) == "table" then
        return result.items
    end
    if type(result.result) == "table" then
        return result.result
    end
    if type(result.json) == "string" then
        local ok, decoded = pcall(vulcan.json.decode, result.json)
        if ok then
            return rows_from_query(decoded)
        end
    end
    if type(result.text) == "string" then
        local ok, decoded = pcall(vulcan.json.decode, result.text)
        if ok then
            return rows_from_query(decoded)
        end
    end
    if result[1] ~= nil then
        return result
    end

    return {}
end

--- Return the first count-like numeric field from one query row.
--- 从一行查询结果中返回第一个计数字段。
--- @param row table|nil Query row.
--- @return number
local function count_from_row(row)
    if type(row) ~= "table" then
        return 0
    end
    return tonumber(row.count or row.cnt or row["COUNT(*)"] or row["count(*)"] or 0) or 0
end

--- Execute a SQLite operation and normalize host-side errors.
--- 执行 SQLite 操作并归一化宿主侧错误。
--- @param fn function SQLite function.
--- @param input table SQLite input.
--- @return table|nil, string|nil
local function sqlite_call(fn, input)
    if type(fn) ~= "function" then
        return nil, "SQLite function is unavailable."
    end

    local ok, result = pcall(fn, input)
    if not ok then
        return nil, tostring(result)
    end
    if type(result) == "table" and result.success == false then
        return nil, tostring(result.error or result.message or "SQLite operation failed.")
    end
    return result, nil
end

--- Run a SQL query and return extracted rows.
--- 执行 SQL 查询并返回提取后的行。
--- @param sql string SQL query.
--- @return table|nil, string|nil
local function query_rows(sql)
    local result, err = sqlite_call(vulcan.sqlite.query_json, {
        sql = sql,
    })
    if err then
        return nil, err
    end
    return rows_from_query(result), nil
end

--- Ensure the skill SQLite binding is enabled.
--- 确保当前技能的 SQLite 绑定已经启用。
--- @return string|nil
local function ensure_sqlite_enabled()
    if type(vulcan.sqlite) ~= "table" or type(vulcan.sqlite.status) ~= "function" then
        return "SQLite is unavailable for this skill."
    end

    local ok, status = pcall(vulcan.sqlite.status)
    if not ok or type(status) ~= "table" then
        return "SQLite status is unavailable: " .. tostring(status)
    end
    if not status.enabled then
        return tostring(status.reason or "SQLite is not enabled for this skill.")
    end
    return nil
end

--- Create or migrate the SQLite schema used by WorkMem.
--- 创建或迁移 WorkMem 使用的 SQLite schema。
--- @return string|nil
local function ensure_schema()
    local _, err = sqlite_call(vulcan.sqlite.execute_script, {
        sql = [[
CREATE TABLE IF NOT EXISTS wm_workmem(
    workmem_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS wm_tasks(
    workmem_id TEXT NOT NULL,
    task_name TEXT NOT NULL,
    detail TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(workmem_id, task_name)
);

CREATE TABLE IF NOT EXISTS wm_nodes(
    workmem_id TEXT NOT NULL,
    task_name TEXT NOT NULL,
    tag TEXT NOT NULL,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata_json TEXT,
    PRIMARY KEY(workmem_id, task_name, tag)
);

CREATE INDEX IF NOT EXISTS idx_wm_nodes_task_type
ON wm_nodes(workmem_id, task_name, type, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_wm_nodes_task_updated
ON wm_nodes(workmem_id, task_name, updated_at DESC);
]],
    })
    return err
end

--- Render a compact error response for AI callers.
--- 为 AI 调用方渲染紧凑错误响应。
--- @param code string Error code.
--- @param message string Error message.
--- @return string
local function render_error(code, message)
    return table.concat({
        "# Work Memory Error",
        "",
        "- code: " .. inline_code(code),
        "- message: " .. tostring(message or "Unknown error."),
    }, "\n")
end

--- Validate and normalize one identifier-style field.
--- 校验并归一化一个标识符风格字段。
--- @param value any Input value.
--- @param label string Field label.
--- @param max_length number Maximum length.
--- @param allow_upper boolean Whether uppercase characters are allowed.
--- @return string|nil, string|nil
local function validate_token(value, label, max_length, allow_upper)
    local text = trim(value)
    if text == "" then
        return nil, label .. " is required."
    end
    if #text > max_length then
        return nil, label .. " is too long; max " .. tostring(max_length) .. " characters."
    end
    local single_pattern = allow_upper and "^[A-Za-z0-9]$" or "^[a-z0-9]$"
    local compound_pattern = allow_upper and "^[A-Za-z0-9][A-Za-z0-9_-]*[A-Za-z0-9]$" or "^[a-z0-9][a-z0-9_-]*[a-z0-9]$"
    if not text:match(single_pattern) and not text:match(compound_pattern) then
        return nil, label .. " must use letters, digits, `_`, or `-`, and must start and end with a letter or digit."
    end
    return text, nil
end

--- Validate and normalize a LUASKILL_SID value.
--- 校验并归一化 LUASKILL_SID 值。
--- @param value any Input value.
--- @param required boolean Whether the ID is required.
--- @return string|nil, string|nil
local function validate_luaskill_sid(value, required)
    local text = trim(value)
    if text == "" then
        if required then
            return nil, "LUASKILL_SID is required. Use the saved LUASKILL_SID or call task-create without LUASKILL_SID first."
        end
        return nil, nil
    end
    if #text < MIN_LUASKILL_SID_CHARS then
        return nil, "LUASKILL_SID is too short; min " .. tostring(MIN_LUASKILL_SID_CHARS) .. " characters. Pass a stable saved LUASKILL_SID, or omit LUASKILL_SID in task-create to generate one."
    end
    if #text > MAX_LUASKILL_SID_CHARS then
        return nil, "LUASKILL_SID is too long; max " .. tostring(MAX_LUASKILL_SID_CHARS) .. " characters."
    end
    if text:find("[%c]") then
        return nil, "LUASKILL_SID must be a single-line stable identity. Remove newlines or control characters."
    end
    return text, nil
end

--- Reject the deprecated workmem_id field before identity resolution.
--- 在身份解析前拒绝已废弃的 workmem_id 字段。
--- @param request table Tool request.
--- @return string|nil
local function reject_deprecated_workmem_id(request)
    if type(request) == "table" and request.workmem_id ~= nil then
        return "workmem_id is no longer supported. Use LUASKILL_SID."
    end
    return nil
end

--- Validate and normalize a task name.
--- 校验并归一化任务名称。
--- @param value any Input value.
--- @return string|nil, string|nil
local function validate_task_name(value)
    return validate_token(value, "task_name", 96, false)
end

--- Validate and normalize a node tag.
--- 校验并归一化节点 tag。
--- @param value any Input value.
--- @return string|nil, string|nil
local function validate_tag(value)
    return validate_token(value, "tag", 96, false)
end

--- Validate and normalize an optional tag prefix filter.
--- 校验并归一化可选 tag 前缀过滤器。
--- @param value any Input value.
--- @return string|nil, string|nil
local function validate_optional_tag_prefix(value)
    local text = trim(value)
    if text == "" then
        return "", nil
    end
    if #text > 96 then
        return nil, "tag_prefix is too long; max 96 characters."
    end
    if not text:match("^[a-z0-9][a-z0-9_-]*$") then
        return nil, "tag_prefix must use lowercase letters, digits, `_`, or `-`, and must start with a letter or digit."
    end
    return text, nil
end

--- Validate and normalize a node type.
--- 校验并归一化节点类型。
--- @param value any Input value.
--- @return string|nil, string|nil
local function validate_node_type(value)
    local node_type = lower_text(value)
    if node_type == "" then
        return nil, "type is required."
    end
    if not NODE_TYPES[node_type] then
        return nil, "type must be one of note, file_summary, decision, todo, progress, risk, checkpoint, tool_result."
    end
    return node_type, nil
end

--- Validate and normalize an optional node type filter.
--- 校验并归一化可选节点类型过滤器。
--- @param value any Input value.
--- @return string|nil, string|nil
local function validate_optional_type(value)
    if value == nil or trim(value) == "" then
        return nil, nil
    end
    return validate_node_type(value)
end

--- Normalize an array-like table into a sequential string list.
--- 将类数组表归一化为连续字符串列表。
--- @param value any Input value.
--- @param label string Field label.
--- @param required boolean Whether the list is required.
--- @return table|nil, string|nil
local function normalize_string_list(value, label, required)
    if value == nil then
        if required then
            return nil, label .. " is required."
        end
        return {}, nil
    end
    if type(value) ~= "table" then
        return nil, label .. " must be an array."
    end

    local normalized = {}
    for _, item in ipairs(value) do
        local text = trim(item)
        if text ~= "" then
            normalized[#normalized + 1] = text
        end
    end
    if required and #normalized == 0 then
        return nil, label .. " must contain at least one item."
    end
    return normalized, nil
end

--- Validate and normalize tag array values.
--- 校验并归一化 tag 数组值。
--- @param value any Input value.
--- @param label string Field label.
--- @param required boolean Whether tags are required.
--- @return table|nil, string|nil
local function normalize_tags(value, label, required)
    local tags, err = normalize_string_list(value, label, required)
    if err then
        return nil, err
    end
    for index, tag in ipairs(tags) do
        local normalized, tag_err = validate_tag(tag)
        if tag_err then
            return nil, label .. "[" .. tostring(index) .. "]: " .. tag_err
        end
        tags[index] = normalized
    end
    return tags, nil
end

--- Validate one set node object.
--- 校验一个 set 节点对象。
--- @param item any Node input.
--- @param index number Node index.
--- @return table|nil, string|nil
local function validate_node_input(item, index)
    if type(item) ~= "table" then
        return nil, "list[" .. tostring(index) .. "] must be an object."
    end

    local tag, tag_err = validate_tag(item.tag)
    if tag_err then
        return nil, "list[" .. tostring(index) .. "].tag: " .. tag_err
    end

    local node_type, type_err = validate_node_type(item.type)
    if type_err then
        return nil, "list[" .. tostring(index) .. "].type: " .. type_err
    end

    local title = trim(item.title)
    if title == "" then
        return nil, "list[" .. tostring(index) .. "].title is required."
    end
    if title:find("[%c]") then
        return nil, "list[" .. tostring(index) .. "].title must be a single line. Remove newlines or control characters."
    end
    if #title > MAX_TITLE_CHARS then
        return nil, "list[" .. tostring(index) .. "].title is too long; max " .. tostring(MAX_TITLE_CHARS) .. " characters."
    end

    local content = trim(item.content)
    if content == "" then
        return nil, "list[" .. tostring(index) .. "].content is required."
    end
    if #content > MAX_CONTENT_CHARS then
        return nil, "list[" .. tostring(index) .. "].content is too long; max " .. tostring(MAX_CONTENT_CHARS) .. " characters. Store a concise summary, not full logs or source."
    end

    return {
        tag = tag,
        type = node_type,
        title = title,
        content = content,
    }, nil
end

--- Validate and normalize a set node array.
--- 校验并归一化 set 节点数组。
--- @param value any Input list.
--- @return table|nil, string|nil
local function normalize_node_list(value)
    if type(value) ~= "table" then
        return nil, "list must be an array of node objects."
    end
    if #value == 0 then
        return nil, "list must contain at least one node."
    end
    if #value > MAX_BATCH_NODES then
        return nil, "list contains too many nodes; max " .. tostring(MAX_BATCH_NODES) .. "."
    end

    local nodes = {}
    for index, item in ipairs(value) do
        local node, err = validate_node_input(item, index)
        if err then
            return nil, err
        end
        nodes[#nodes + 1] = node
    end
    return nodes, nil
end

--- Upsert the project-level WorkMem row for one LUASKILL_SID.
--- 为一个 LUASKILL_SID 写入或更新项目级 WorkMem 行。
--- @param luaskill_sid string LuaSkills managed identity.
--- @param source string ID source label.
--- @param updated_at string Current timestamp.
--- @return string|nil
local function upsert_workmem(luaskill_sid, source, updated_at)
    local _, err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = [[
INSERT INTO wm_workmem(workmem_id, source, created_at, updated_at, metadata_json)
VALUES (?1, ?2, ?3, ?3, '{}')
ON CONFLICT(workmem_id) DO UPDATE SET updated_at = excluded.updated_at
]],
        items = {
            { luaskill_sid, source, updated_at },
        },
    })
    return err
end

--- Upsert one task row and optionally replace the task detail.
--- 写入或更新一个任务行，并可选择替换任务说明。
--- @param luaskill_sid string LuaSkills managed identity.
--- @param task_name string Task name.
--- @param detail string Task detail.
--- @param update_detail boolean Whether to update detail on conflict.
--- @param updated_at string Current timestamp.
--- @return string|nil
local function upsert_task(luaskill_sid, task_name, detail, update_detail, updated_at)
    local conflict_sql = update_detail
            and "ON CONFLICT(workmem_id, task_name) DO UPDATE SET detail = excluded.detail, updated_at = excluded.updated_at"
        or "ON CONFLICT(workmem_id, task_name) DO UPDATE SET updated_at = excluded.updated_at"

    local _, err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = [[
INSERT INTO wm_tasks(workmem_id, task_name, detail, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4, ?4)
]]
            .. conflict_sql,
        items = {
            { luaskill_sid, task_name, detail or "", updated_at },
        },
    })
    return err
end

--- Ensure a task namespace exists before node operations.
--- 在节点操作前确保任务命名空间存在。
--- @param luaskill_sid string LuaSkills managed identity.
--- @param task_name string Task name.
--- @param updated_at string Current timestamp.
--- @return string|nil
local function ensure_task_namespace(luaskill_sid, task_name, updated_at)
    local source = is_host_managed(luaskill_sid) and "host" or "provided"
    local err = upsert_workmem(luaskill_sid, source, updated_at)
    if err then
        return err
    end
    return upsert_task(luaskill_sid, task_name, "", false, updated_at)
end

--- Count nodes for one task.
--- 统计一个任务的节点数量。
--- @param luaskill_sid string LuaSkills managed identity.
--- @param task_name string Task name.
--- @return number, string|nil
local function count_task_nodes(luaskill_sid, task_name)
    local rows, err = query_rows(
        "SELECT COUNT(*) AS count FROM wm_nodes WHERE workmem_id = "
            .. sql_quote(luaskill_sid)
            .. " AND task_name = "
            .. sql_quote(task_name)
    )
    if err then
        return 0, err
    end
    return count_from_row(rows[1]), nil
end

--- Render the persistent setup hint after generating a new LUASKILL_SID.
--- 生成新 LUASKILL_SID 后渲染持久化设置提示。
--- @param luaskill_sid string Generated LuaSkills managed identity.
--- @return string
local function render_persistent_setup_hint(luaskill_sid)
    return table.concat({
        "## Persistent Setup Suggested",
        "",
        "Ask the user whether to save this LUASKILL_SID into `AGENTS.md` or `CLAUDE.md`:",
        "",
        "- LUASKILL_SID: " .. inline_code(luaskill_sid),
        "- Once saved, it remains valid for future tasks.",
        "- Do not repeat this prompt when the rule file already contains the ID.",
    }, "\n")
end

--- Render the host-managed setup hint without exposing the raw ID.
--- 渲染宿主管理设置提示且不暴露原始 ID。
--- @return string
local function render_host_managed_hint()
    return table.concat({
        "## Host-Managed Mode",
        "",
        "- The active LUASKILL_SID is injected by the host bridge.",
        "- Do not ask for, print, or persist the raw LUASKILL_SID.",
        "- Continue using the task name and node tags normally.",
    }, "\n")
end

--- Handle the task-create action.
--- 处理 task-create 操作。
--- @param request table Tool request.
--- @return string
local function action_task_create(request)
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end

    local detail = trim(request.detail)
    if detail == "" then
        return render_error("INPUT_ERROR", "detail is required for task-create.")
    end
    if #detail > MAX_DETAIL_CHARS then
        return render_error("INPUT_ERROR", "detail is too long; max " .. tostring(MAX_DETAIL_CHARS) .. " characters.")
    end

    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, false)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end

    local generated = false
    if not luaskill_sid then
        luaskill_sid = generate_luaskill_sid()
        generated = true
    end

    local updated_at = now_utc()
    local source = generated and "generated" or (is_host_managed(luaskill_sid) and "host" or "provided")
    local err = upsert_workmem(luaskill_sid, source, updated_at)
    if err then
        return render_error("SQLITE_ERROR", err)
    end
    err = upsert_task(luaskill_sid, task_name, detail, true, updated_at)
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local lines = {
        "# Work Memory Task Ready",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- source: " .. inline_code(source),
        "- updated_at: " .. inline_code(updated_at),
        "",
        "## Task Detail",
        "",
        shorten(detail, 500),
    }

    if is_host_managed(luaskill_sid) then
        lines[#lines + 1] = ""
        lines[#lines + 1] = render_host_managed_hint()
    elseif generated then
        lines[#lines + 1] = ""
        lines[#lines + 1] = render_persistent_setup_hint(luaskill_sid)
    end

    return table.concat(lines, "\n")
end

--- Handle the set action.
--- 处理 set 操作。
--- @param request table Tool request.
--- @return string
local function action_set(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end
    local nodes, list_err = normalize_node_list(request.list)
    if list_err then
        return render_error("INPUT_ERROR", list_err)
    end
    local delete_tags, delete_err = normalize_tags(request.delete_tags, "delete_tags", false)
    if delete_err then
        return render_error("INPUT_ERROR", delete_err)
    end

    local updated_at = now_utc()
    local err = ensure_task_namespace(luaskill_sid, task_name, updated_at)
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    if #delete_tags > 0 then
        local delete_items = {}
        for _, tag in ipairs(delete_tags) do
            delete_items[#delete_items + 1] = { luaskill_sid, task_name, tag }
        end
        local _, delete_sql_err = sqlite_call(vulcan.sqlite.execute_batch, {
            sql = "DELETE FROM wm_nodes WHERE workmem_id = ?1 AND task_name = ?2 AND tag = ?3",
            items = delete_items,
        })
        if delete_sql_err then
            return render_error("SQLITE_ERROR", delete_sql_err)
        end
    end

    local write_items = {}
    for _, node in ipairs(nodes) do
        write_items[#write_items + 1] = {
            luaskill_sid,
            task_name,
            node.tag,
            node.type,
            node.title,
            node.content,
            updated_at,
        }
    end

    local _, write_err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = [[
INSERT INTO wm_nodes(workmem_id, task_name, tag, type, title, content, created_at, updated_at, metadata_json)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, '{}')
ON CONFLICT(workmem_id, task_name, tag) DO UPDATE SET
    type = excluded.type,
    title = excluded.title,
    content = excluded.content,
    updated_at = excluded.updated_at
]],
        items = write_items,
    })
    if write_err then
        return render_error("SQLITE_ERROR", write_err)
    end

    local tags = {}
    for _, node in ipairs(nodes) do
        tags[#tags + 1] = inline_code(node.tag)
    end

    return table.concat({
        "# Work Memory Saved",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- written_nodes: " .. tostring(#nodes),
        "- delete_tags_requested: " .. tostring(#delete_tags),
        "- updated_at: " .. inline_code(updated_at),
        "",
        "## Written Tags",
        "",
        table.concat(tags, ", "),
        "",
        "Use `list` to inspect tags or `get` to recall selected content.",
    }, "\n")
end

--- Handle the list action.
--- 处理 list 操作。
--- @param request table Tool request.
--- @return string
local function action_list(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end
    local type_filter, type_err = validate_optional_type(request.type)
    if type_err then
        return render_error("INPUT_ERROR", type_err)
    end
    local tag_prefix, prefix_err = validate_optional_tag_prefix(request.tag_prefix)
    if prefix_err then
        return render_error("INPUT_ERROR", prefix_err)
    end

    local conditions = {
        "workmem_id = " .. sql_quote(luaskill_sid),
        "task_name = " .. sql_quote(task_name),
    }
    if type_filter then
        conditions[#conditions + 1] = "type = " .. sql_quote(type_filter)
    end
    if tag_prefix ~= "" then
        conditions[#conditions + 1] = "substr(tag, 1, " .. tostring(#tag_prefix) .. ") = " .. sql_quote(tag_prefix)
    end

    local rows, err = query_rows(
        "SELECT tag, type, title, updated_at FROM wm_nodes WHERE "
            .. table.concat(conditions, " AND ")
            .. " ORDER BY updated_at DESC, tag ASC"
    )
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local lines = {
        "# Work Memory Tags",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- total_tags: " .. tostring(#rows),
    }
    if type_filter then
        lines[#lines + 1] = "- type_filter: " .. inline_code(type_filter)
    end
    if tag_prefix ~= "" then
        lines[#lines + 1] = "- tag_prefix: " .. inline_code(tag_prefix)
    end
    lines[#lines + 1] = ""

    if #rows == 0 then
        lines[#lines + 1] = "No tags stored for this task."
        return table.concat(lines, "\n")
    end

    lines[#lines + 1] = "## Tags"
    lines[#lines + 1] = ""
    for _, row in ipairs(rows) do
        lines[#lines + 1] = "- "
            .. inline_code(row.tag)
            .. " | "
            .. tostring(row.type or "")
            .. " | "
            .. shorten(single_line(row.title or ""), 100)
            .. " | "
            .. tostring(row.updated_at or "")
    end
    return table.concat(lines, "\n")
end

--- Render one full node block.
--- 渲染一个完整节点块。
--- @param row table Node row.
--- @return string
local function render_node_block(row)
    return table.concat({
        "## " .. inline_code(row.tag),
        "",
        "- type: " .. inline_code(row.type),
        "- title: " .. tostring(row.title or ""),
        "- updated_at: " .. inline_code(row.updated_at),
        "",
        tostring(row.content or ""),
    }, "\n")
end

--- Handle the get action.
--- 处理 get 操作。
--- @param request table Tool request.
--- @return string
local function action_get(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end
    local tags, tags_err = normalize_tags(request.tags, "tags", false)
    if tags_err then
        return render_error("INPUT_ERROR", tags_err)
    end

    local sql
    if #tags > 0 then
        local quoted = {}
        for _, tag in ipairs(tags) do
            quoted[#quoted + 1] = sql_quote(tag)
        end
        sql = "SELECT tag, type, title, content, updated_at FROM wm_nodes WHERE workmem_id = "
            .. sql_quote(luaskill_sid)
            .. " AND task_name = "
            .. sql_quote(task_name)
            .. " AND tag IN ("
            .. table.concat(quoted, ",")
            .. ") ORDER BY tag ASC"
    else
        sql = "SELECT tag, type, title, content, updated_at FROM wm_nodes WHERE workmem_id = "
            .. sql_quote(luaskill_sid)
            .. " AND task_name = "
            .. sql_quote(task_name)
            .. " ORDER BY updated_at DESC, tag ASC LIMIT 8"
    end

    local rows, err = query_rows(sql)
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local lines = {
        "# Work Memory Recall",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- returned_nodes: " .. tostring(#rows),
    }
    if #tags == 0 then
        lines[#lines + 1] = "- mode: `latest-compact-node-preview`"
        lines[#lines + 1] = "- limit: `8`"
        lines[#lines + 1] = "- note: This is a latest compact node preview, not a full task summary. Use `vulcan-workmem-list` then `vulcan-workmem-get(tags)` for precise recall, or `vulcan-workmem-get-all` for full recovery."
    end
    lines[#lines + 1] = ""

    if #rows == 0 then
        lines[#lines + 1] = "No matching nodes found."
        return table.concat(lines, "\n")
    end

    if #tags == 0 then
        lines[#lines + 1] = "## Latest Compact Node Preview"
        lines[#lines + 1] = ""
        for _, row in ipairs(rows) do
            lines[#lines + 1] = "- "
                .. inline_code(row.tag)
                .. " | "
                .. tostring(row.type or "")
                .. " | "
                .. shorten(single_line(row.title or ""), 80)
                .. ": "
                .. shorten(single_line(row.content or ""), 220)
        end
        return table.concat(lines, "\n")
    end

    lines[#lines + 1] = "## Nodes"
    lines[#lines + 1] = ""
    for _, row in ipairs(rows) do
        lines[#lines + 1] = render_node_block(row)
        lines[#lines + 1] = ""
    end
    return table.concat(lines, "\n")
end

--- Handle the get-all action.
--- 处理 get-all 操作。
--- @param request table Tool request.
--- @return string
local function action_get_all(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end

    local rows, err = query_rows(
        "SELECT tag, type, title, content, updated_at FROM wm_nodes WHERE workmem_id = "
            .. sql_quote(luaskill_sid)
            .. " AND task_name = "
            .. sql_quote(task_name)
            .. " ORDER BY updated_at DESC, tag ASC"
    )
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local lines = {
        "# Work Memory Full Recall",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- total_nodes: " .. tostring(#rows),
        "- paging: `handled-by-luaskill-runtime`",
        "",
    }

    if #rows == 0 then
        lines[#lines + 1] = "No nodes stored for this task."
        return table.concat(lines, "\n")
    end

    lines[#lines + 1] = "## Nodes"
    lines[#lines + 1] = ""
    for _, row in ipairs(rows) do
        lines[#lines + 1] = render_node_block(row)
        lines[#lines + 1] = ""
    end
    return table.concat(lines, "\n")
end

--- Handle the del action.
--- 处理 del 操作。
--- @param request table Tool request.
--- @return string
local function action_del(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end
    local tags, tags_err = normalize_tags(request.tags, "tags", true)
    if tags_err then
        return render_error("INPUT_ERROR", tags_err)
    end

    local delete_items = {}
    for _, tag in ipairs(tags) do
        delete_items[#delete_items + 1] = { luaskill_sid, task_name, tag }
    end
    local _, err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = "DELETE FROM wm_nodes WHERE workmem_id = ?1 AND task_name = ?2 AND tag = ?3",
        items = delete_items,
    })
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local rendered_tags = {}
    for _, tag in ipairs(tags) do
        rendered_tags[#rendered_tags + 1] = inline_code(tag)
    end

    return table.concat({
        "# Work Memory Deleted",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- delete_tags_requested: " .. tostring(#tags),
        "",
        "## Tags",
        "",
        table.concat(rendered_tags, ", "),
    }, "\n")
end

--- Handle the task-list action.
--- 处理 task-list 操作。
--- @param request table Tool request.
--- @return string
local function action_task_list(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end

    local rows, err = query_rows(
        "SELECT t.task_name, t.detail, t.updated_at, COUNT(n.tag) AS node_count "
            .. "FROM wm_tasks t LEFT JOIN wm_nodes n "
            .. "ON t.workmem_id = n.workmem_id AND t.task_name = n.task_name "
            .. "WHERE t.workmem_id = "
            .. sql_quote(luaskill_sid)
            .. " GROUP BY t.task_name, t.detail, t.updated_at "
            .. "ORDER BY t.updated_at DESC, t.task_name ASC"
    )
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local lines = {
        "# Work Memory Tasks",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- total_tasks: " .. tostring(#rows),
        "",
    }

    if #rows == 0 then
        lines[#lines + 1] = "No tasks stored for this LUASKILL_SID."
        return table.concat(lines, "\n")
    end

    lines[#lines + 1] = "## Tasks"
    lines[#lines + 1] = ""
    for _, row in ipairs(rows) do
        lines[#lines + 1] = "- "
            .. inline_code(row.task_name)
            .. " | nodes: "
            .. tostring(row.node_count or 0)
            .. " | updated: "
            .. tostring(row.updated_at or "")
            .. " | "
            .. shorten(single_line(row.detail or ""), 120)
    end
    return table.concat(lines, "\n")
end

--- Handle the task-close action.
--- 处理 task-close 操作。
--- @param request table Tool request.
--- @return string
local function action_task_close(request)
    local luaskill_sid, id_err = validate_luaskill_sid(request.LUASKILL_SID, true)
    if id_err then
        return render_error("INPUT_ERROR", id_err)
    end
    local task_name, task_err = validate_task_name(request.task_name)
    if task_err then
        return render_error("INPUT_ERROR", task_err)
    end

    local node_count, count_err = count_task_nodes(luaskill_sid, task_name)
    if count_err then
        return render_error("SQLITE_ERROR", count_err)
    end

    local _, err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = "DELETE FROM wm_nodes WHERE workmem_id = ?1 AND task_name = ?2",
        items = {
            { luaskill_sid, task_name },
        },
    })
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    _, err = sqlite_call(vulcan.sqlite.execute_batch, {
        sql = "DELETE FROM wm_tasks WHERE workmem_id = ?1 AND task_name = ?2",
        items = {
            { luaskill_sid, task_name },
        },
    })
    if err then
        return render_error("SQLITE_ERROR", err)
    end

    local remaining_rows, remaining_err = query_rows(
        "SELECT COUNT(*) AS count FROM wm_tasks WHERE workmem_id = " .. sql_quote(luaskill_sid)
    )
    if remaining_err then
        return render_error("SQLITE_ERROR", remaining_err)
    end
    local remaining_tasks = count_from_row(remaining_rows[1])
    local empty_workmem_cleaned = false
    if remaining_tasks == 0 then
        _, err = sqlite_call(vulcan.sqlite.execute_batch, {
            sql = "DELETE FROM wm_workmem WHERE workmem_id = ?1",
            items = {
                { luaskill_sid },
            },
        })
        if err then
            return render_error("SQLITE_ERROR", err)
        end
        empty_workmem_cleaned = true
    end

    local closing_note = "The LUASKILL_SID remains valid as a long-lived project rule value. Reuse it for future tasks if it is saved in `AGENTS.md` or `CLAUDE.md`."
    if is_host_managed(luaskill_sid) then
        closing_note = "Host-managed mode remains active through the host bridge. Do not ask for, print, or persist the raw LUASKILL_SID."
    end

    return table.concat({
        "# Work Memory Task Closed",
        "",
        "- LUASKILL_SID: " .. display_luaskill_sid(luaskill_sid),
        "- task: " .. inline_code(task_name),
        "- deleted_nodes: " .. tostring(node_count),
        "- remaining_tasks: " .. tostring(remaining_tasks),
        "- empty_identity_row_cleaned: " .. tostring(empty_workmem_cleaned),
        "",
        closing_note,
    }, "\n")
end

--- Dispatch the validated action to its handler.
--- 将已校验 action 分发到对应处理器。
--- @param request table Tool request.
--- @return string
local function dispatch(request)
    local action = lower_text(request.action)
    if not ACTIONS[action] then
        return render_error("INPUT_ERROR", "action must be one of task-create, set, list, get, get-all, del, task-list, task-close.")
    end

    local deprecated_id_err = reject_deprecated_workmem_id(request)
    if deprecated_id_err then
        return render_error("INPUT_ERROR", deprecated_id_err)
    end

    if action == "task-create" then
        return action_task_create(request)
    end
    if action == "set" then
        return action_set(request)
    end
    if action == "list" then
        return action_list(request)
    end
    if action == "get" then
        return action_get(request)
    end
    if action == "get-all" then
        return action_get_all(request)
    end
    if action == "del" then
        return action_del(request)
    end
    if action == "task-list" then
        return action_task_list(request)
    end
    return action_task_close(request)
end

--- Handle one Vulcan WorkMem request through the shared dispatcher.
--- 通过共享分发器处理一次 Vulcan WorkMem 请求。
--- @param args table|nil Tool arguments.
--- @return string
local function handle(args)
    local request = type(args) == "table" and args or {}

    local sqlite_err = ensure_sqlite_enabled()
    if sqlite_err then
        return render_error("SQLITE_DISABLED", sqlite_err)
    end

    local schema_err = ensure_schema()
    if schema_err then
        return render_error("SQLITE_ERROR", schema_err)
    end

    return dispatch(request)
end

--- Return a shallow request copy with one fixed action injected.
--- 返回注入固定 action 的请求浅拷贝。
--- @param args table|nil Tool arguments.
--- @param action string Fixed WorkMem action.
--- @return table
local function with_action(args, action)
    local request = {}
    if type(args) == "table" then
        for key, value in pairs(args) do
            request[key] = value
        end
    end

    request.action = action
    return request
end

--- Export the WorkMem core as a reusable module for split MCP entries.
--- 将 WorkMem 核心导出为可供拆分 MCP 入口复用的模块。
return {
    handle = handle,
    with_action = with_action,
}
