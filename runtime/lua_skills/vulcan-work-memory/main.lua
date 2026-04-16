-- 中文：`vulcan-work-memory` 的宿主管理 SQLite 测试工具入口。
-- English: Host-managed SQLite test entry for `vulcan-work-memory`.

--- 中文：构造稳定的默认测试说明，确保无参调用也能看到持久化结果。
--- English: Build a stable default note so parameter-free calls still produce a visible persisted result.
--- @return string, string
local function build_default_note()
    local timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ")
    local note = "vulcan-work-memory sqlite host test entry"
    return note, timestamp
end

--- 中文：安全读取当前 skill 的 SQLite 状态对象。
--- English: Safely read the current SQLite status object for the skill.
--- @return table
local function get_sqlite_status()
    if type(vulcan.sqlite) ~= "table" or type(vulcan.sqlite.status) ~= "function" then
        return {
            enabled = false,
            initialized = false,
            reason = "vulcan.sqlite is unavailable",
        }
    end

    local ok, status = pcall(vulcan.sqlite.status)
    if not ok or type(status) ~= "table" then
        return {
            enabled = false,
            initialized = false,
            reason = tostring(status),
        }
    end
    return status
end

--- 中文：工具主入口，验证宿主管理 SQLite 的状态、建索引、写入文档与检索闭环。
--- English: Main tool entry that verifies the host-managed SQLite status plus the full ensure-index, document-upsert, and search loop.
--- @param args table|nil 工具参数 / Tool arguments.
--- @return table
return function(args)
    local status = get_sqlite_status()
    if not status.enabled then
        return {
            ok = false,
            error = "sqlite_not_enabled",
            message = tostring(status.reason or "current skill has not enabled sqlite"),
            status = status,
        }
    end

    local note, created_at = build_default_note()
    if type(args) == "table" and args.note ~= nil and tostring(args.note) ~= "" then
        note = tostring(args.note)
    end

    local index_name = "work_memory_entries"
    local document_id = "work-memory-" .. created_at
    local file_path = "/skills/vulcan-work-memory"
    local title = "Work Memory Entry"
    local content = note .. " @ " .. created_at

    local ensure_result = vulcan.sqlite.ensure_fts_index({
        index_name = index_name,
        tokenizer_mode = "jieba",
    })

    local upsert_result = vulcan.sqlite.upsert_fts_document({
        index_name = index_name,
        tokenizer_mode = "jieba",
        id = document_id,
        file_path = file_path,
        title = title,
        content = content,
    })

    local search_result = vulcan.sqlite.search_fts({
        index_name = index_name,
        tokenizer_mode = "jieba",
        query = note,
        limit = 5,
        offset = 0,
    })

    local dictionary_result = vulcan.sqlite.list_custom_words()

    return {
        ok = true,
        message = "Host-managed SQLite test completed for vulcan-work-memory.",
        status = status,
        ensure_result = ensure_result,
        upsert_result = upsert_result,
        search_result = search_result,
        dictionary_result = dictionary_result,
        inserted = {
            id = document_id,
            file_path = file_path,
            title = title,
            content = content,
        },
    }
end
