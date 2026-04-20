-- 中文：`vulcan-ai-memory` 的 LanceDB 测试工具入口。
-- English: LanceDB test tool entry for `vulcan-ai-memory`.

--- 中文：统一返回稳定默认值，避免测试参数缺失时影响链路验证。
--- English: Provide stable defaults so missing test arguments never break the validation flow.
--- @param args table|nil 工具入参 / Tool arguments.
--- @return table
local function resolve_test_config(args)
    local config = {
        table_name = "ai_memory_test",
        record_id = "memory-test-001",
        memory_text = "这是一条用于验证宿主管理 LanceDB 的测试记忆。 / This is a test memory used to verify the host-managed LanceDB binding.",
        overwrite_if_exists = true,
    }

    if type(args) ~= "table" then
        return config
    end

    if args.table_name ~= nil and tostring(args.table_name) ~= "" then
        config.table_name = tostring(args.table_name)
    end
    if args.record_id ~= nil and tostring(args.record_id) ~= "" then
        config.record_id = tostring(args.record_id)
    end
    if args.memory_text ~= nil and tostring(args.memory_text) ~= "" then
        config.memory_text = tostring(args.memory_text)
    end
    if args.overwrite_if_exists ~= nil then
        config.overwrite_if_exists = args.overwrite_if_exists == true
    end

    return config
end

--- 中文：生成稳定的测试向量，确保每次检索都能命中刚写入的记录。
--- English: Build a stable test vector so each search can deterministically hit the freshly inserted row.
--- @return table
local function build_test_embedding()
    return { 0.11, 0.22, 0.33, 0.44 }
end

--- 中文：构造用于宿主建表接口的列定义。
--- English: Build the column definition expected by the host-side create-table interface.
--- @return table
local function build_table_columns()
    return {
        {
            name = "id",
            column_type = "string",
            nullable = false,
        },
        {
            name = "memory_text",
            column_type = "string",
            nullable = false,
        },
        {
            name = "embedding",
            column_type = "vector_float32",
            vector_dim = 4,
            nullable = false,
        },
    }
end

--- 中文：构造 JSON Rows 形式的写入数据。
--- English: Build the JSON rows payload used by the host-side vector upsert interface.
--- @param config table 已解析的测试配置 / Resolved test config.
--- @param embedding table 测试向量 / Test embedding vector.
--- @return table
local function build_upsert_rows(config, embedding)
    return {
        {
            id = config.record_id,
            memory_text = config.memory_text,
            embedding = embedding,
        },
    }
end

--- 中文：封装宿主 LanceDB 可用性检查，优先给出稳定状态而不是直接抛 Lua 空引用错误。
--- English: Wrap host-side LanceDB availability checks so we return stable status diagnostics instead of a raw Lua nil-reference failure.
--- @return boolean, table
local function check_lancedb_ready()
    if type(vulcan) ~= "table" or type(vulcan.lancedb) ~= "table" then
        return false, {
            enabled = false,
            initialized = false,
            reason = "vulcan.lancedb is unavailable / vulcan.lancedb 不可用",
        }
    end

    local status_ok, status_result = pcall(vulcan.lancedb.status)
    if not status_ok then
        return false, {
            enabled = false,
            initialized = false,
            reason = tostring(status_result),
        }
    end

    if type(status_result) ~= "table" then
        return false, {
            enabled = false,
            initialized = false,
            reason = "lancedb.status returned a non-table result / lancedb.status 返回了非 table 结果",
        }
    end

    if status_result.enabled ~= true then
        return false, status_result
    end

    return true, status_result
end

--- 中文：工具主入口，验证当前 skill 的 LanceDB 绑定是否可正常完成建表、写入与检索闭环。
--- English: Main tool entry that verifies the current skill's LanceDB binding can complete the full create-table, upsert, and search loop.
--- @param args table|nil 工具参数 / Tool arguments.
--- @return string
return function(args)
    local ready, status = check_lancedb_ready()
    if not ready then
        return vulcan.json.encode({
            ok = false,
            error = "lancedb_not_ready",
            message = tostring(status.reason or "current skill has not enabled lancedb"),
            lancedb_status = status,
        })
    end

    local info_ok, info_result = pcall(vulcan.lancedb.info)
    if not info_ok then
        return vulcan.json.encode({
            ok = false,
            error = "lancedb_info_failed",
            message = tostring(info_result),
            lancedb_status = status,
        })
    end

    local config = resolve_test_config(args)
    local embedding = build_test_embedding()

    local create_result = vulcan.lancedb.create_table({
        table_name = config.table_name,
        columns = build_table_columns(),
        overwrite_if_exists = config.overwrite_if_exists,
    })

    local upsert_result = vulcan.lancedb.vector_upsert({
        table_name = config.table_name,
        key_columns = { "id" },
        rows = build_upsert_rows(config, embedding),
    })

    local search_result = vulcan.lancedb.vector_search({
        table_name = config.table_name,
        vector = embedding,
        limit = 3,
        vector_column = "embedding",
        output_format = "json",
    })

    return vulcan.json.encode({
        ok = true,
        message = "LanceDB test completed for vulcan-ai-memory.",
        lancedb_status = status,
        lancedb_info = info_result,
        table_name = config.table_name,
        record_id = config.record_id,
        create_table = create_result,
        vector_upsert = upsert_result,
        vector_search = search_result,
    })
end
