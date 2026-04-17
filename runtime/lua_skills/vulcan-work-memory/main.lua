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

--- 中文：工具主入口，验证宿主管理 SQLite 的通用 SQL 与 FTS 能力闭环。
--- English: Main tool entry that verifies both generic SQL and FTS flows for the host-managed SQLite integration.
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
    local sql_table = "work_memory_notes"

    local execute_result = vulcan.sqlite.execute_script({
        sql = [[
            CREATE TABLE IF NOT EXISTS work_memory_notes(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                note TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
        ]],
    })

    local batch_result = vulcan.sqlite.execute_batch({
        sql = "INSERT INTO work_memory_notes(note, created_at) VALUES (?1, ?2)",
        items = {
            { note, created_at },
        },
    })

    local query_json_result = vulcan.sqlite.query_json({
        sql = "SELECT id, note, created_at FROM work_memory_notes ORDER BY id DESC LIMIT 5",
    })

    local stream_result = vulcan.sqlite.query_stream({
        sql = "SELECT id, note, created_at FROM work_memory_notes ORDER BY id DESC LIMIT 5",
        chunk_bytes = 4096,
    })

    local stream_chunk_result = nil
    local stream_metrics_result = nil
    local stream_close_result = nil
    if type(stream_result) == "table" and stream_result.success then
        stream_chunk_result = vulcan.sqlite.query_stream_chunk({
            stream_id = stream_result.stream_id,
            index = 0,
        })
        stream_metrics_result = vulcan.sqlite.query_stream_wait_metrics({
            stream_id = stream_result.stream_id,
        })
        stream_close_result = vulcan.sqlite.query_stream_close({
            stream_id = stream_result.stream_id,
        })
    end

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
        message = "Host-managed SQLite generic SQL and FTS test completed for vulcan-work-memory.",
        status = status,
        execute_result = execute_result,
        batch_result = batch_result,
        query_json_result = query_json_result,
        query_stream_result = stream_result,
        query_stream_chunk_result = stream_chunk_result,
        query_stream_metrics_result = stream_metrics_result,
        query_stream_close_result = stream_close_result,
        ensure_result = ensure_result,
        upsert_result = upsert_result,
        search_result = search_result,
        dictionary_result = dictionary_result,
        inserted = {
            table = sql_table,
            id = document_id,
            file_path = file_path,
            title = title,
            content = content,
        },
    }
end
