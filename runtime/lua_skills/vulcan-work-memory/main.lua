-- 中文：`vulcan-work-memory` 的 SQLite 测试工具入口。
-- English: SQLite test tool entry for `vulcan-work-memory`.

--- 中文：返回当前 skill 目录，优先使用宿主注入的 skill 目录变量。
--- English: Resolve the current skill directory, preferring the host-injected skill directory variable.
--- @return string
local function get_skill_dir()
    return __skill_dir_vulcan_work_memory_sqlite_test or "."
end

--- 中文：返回当前 skill 所在的 `lua_skills` 根目录，避免把 `..` 片段直接交给宿主文件接口。
--- English: Return the current `lua_skills` root directory so host file APIs never receive raw `..` segments.
--- @return string
local function get_skills_root()
    local skill_dir = get_skill_dir()
    local normalized = tostring(skill_dir):gsub("\\", "/")
    local parent = normalized:match("^(.*)/[^/]+$")
    if parent == nil or parent == "" then
        return "."
    end

    if vulcan.osinfo().os == "windows" then
        return parent:gsub("/", "\\")
    end
    return parent
end

--- 中文：将路径规整为宿主文件接口更容易接受的形式，重点处理 Windows 下的分隔符。
--- English: Normalize paths into a host-friendly form, especially for Windows separator handling.
--- @param path_text string 原始路径文本 / Raw path text.
--- @return string
local function normalize_host_path(path_text)
    local normalized = tostring(path_text or "")
    if vulcan.osinfo().os == "windows" then
        normalized = normalized:gsub("/", "\\")
        normalized = normalized:gsub("\\+", "\\")
    else
        normalized = normalized:gsub("\\", "/")
        normalized = normalized:gsub("/+", "/")
    end
    return normalized
end

--- 中文：安全关闭 SQLite 连接，避免错误路径再次抛出异常。
--- English: Close the SQLite connection safely so cleanup never raises a second error.
--- @param db table|nil SQLite 连接对象 / SQLite database handle.
local function safe_close_database(db)
    if db ~= nil and type(db.close) == "function" then
        pcall(function()
            db:close()
        end)
    end
end

--- 中文：优先加载 LuaFileSystem，用于递归创建数据库目录。
--- English: Load LuaFileSystem first so directory creation can stay inside Lua when possible.
--- @return table|nil, string|nil
local function load_lfs()
    local ok, module_or_error = pcall(require, "lfs")
    if ok and type(module_or_error) == "table" then
        return module_or_error, nil
    end
    return nil, tostring(module_or_error)
end

--- 中文：加载 SQLite Lua 绑定，优先使用 `lsqlite3complete`，并在必要时回退到 `lsqlite3`。
--- English: Load the Lua SQLite binding, preferring `lsqlite3complete` and falling back to `lsqlite3` when needed.
--- @return table|nil, string|nil, string|nil
local function load_sqlite_binding()
    local candidate_modules = { "lsqlite3complete", "lsqlite3" }
    local errors = {}

    for _, module_name in ipairs(candidate_modules) do
        local ok, module_or_error = pcall(require, module_name)
        if ok and type(module_or_error) == "table" then
            return module_or_error, nil, module_name
        end
        table.insert(errors, module_name .. ": " .. tostring(module_or_error))
    end

    return nil, table.concat(errors, " | "), nil
end

--- 中文：拼出共享数据库根目录，遵循 `__tools` 同级共享目录约定。
--- English: Build the shared database root by following the same sibling shared-directory pattern used by `__tools`.
--- @return string
local function get_database_root()
    return normalize_host_path(vulcan.path_join(get_skills_root(), "__database", "vulcan-work-memory"))
end

--- 中文：返回当前测试数据库文件路径。
--- English: Return the current test database file path.
--- @return string
local function get_database_path()
    return normalize_host_path(vulcan.path_join(get_database_root(), "work-memory.sqlite3"))
end

--- 中文：SQLite 锁竞争时默认等待的毫秒数。
--- English: Default number of milliseconds that SQLite should wait during lock contention.
local SQLITE_BUSY_TIMEOUT_MS = 5000

--- 中文：写事务遇到短暂锁竞争时的最大重试次数。
--- English: Maximum retry count when a write transaction hits transient lock contention.
local SQLITE_WRITE_RETRY_MAX_ATTEMPTS = 4

--- 中文：写事务重试前的退避等待毫秒数。
--- English: Backoff wait time in milliseconds before retrying a write transaction.
local SQLITE_WRITE_RETRY_SLEEP_MS = 120

--- 中文：使用 LuaFileSystem 递归创建目录。
--- English: Recursively create the directory tree with LuaFileSystem.
--- @param lfs table LuaFileSystem 模块 / LuaFileSystem module.
--- @param directory_path string 目标目录 / Target directory.
--- @return boolean, string|nil
local function ensure_directory_with_lfs(lfs, directory_path)
    if vulcan.fs_exists(directory_path) then
        if vulcan.fs_is_dir(directory_path) then
            return true, nil
        end
        return false, "target path exists but is not a directory"
    end

    local separator_pattern = "[/\\]+"
    local segments = {}
    for segment in string.gmatch(directory_path, "[^/\\]+") do
        table.insert(segments, segment)
    end

    local prefix = ""
    local drive_prefix = directory_path:match("^([A-Za-z]:)")
    if drive_prefix ~= nil then
        prefix = drive_prefix
    elseif directory_path:match("^[/\\]") then
        prefix = ""
    end

    local current = prefix
    for _, segment in ipairs(segments) do
        if current == "" then
            current = segment
        elseif current:match("^[A-Za-z]:$") then
            current = current .. "\\" .. segment
        else
            current = current .. "/" .. segment
        end

        if not vulcan.fs_exists(current) then
            local ok, mkdir_error = pcall(function()
                local result, error_code = lfs.mkdir(current)
                if result == nil then
                    error(error_code or ("failed to create directory: " .. current))
                end
            end)
            if not ok and not vulcan.fs_exists(current) then
                return false, tostring(mkdir_error)
            end
        elseif not vulcan.fs_is_dir(current) then
            return false, "path component is not a directory: " .. current
        end
    end

    return true, nil
end

--- 中文：在 LuaFileSystem 不可用时，回退到宿主 `vulcan.exec` 创建目录。
--- English: Fall back to host-side `vulcan.exec` directory creation when LuaFileSystem is unavailable.
--- @param directory_path string 目标目录 / Target directory.
--- @return boolean, string|nil
local function ensure_directory_with_exec(directory_path)
    if type(vulcan.exec) ~= "function" then
        return false, "vulcan.exec is unavailable"
    end

    local os_info = vulcan.osinfo()
    if os_info.os == "windows" then
        local ok, result = pcall(vulcan.exec, {
            program = "powershell.exe",
            args = {
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                string.format("New-Item -ItemType Directory -Force -Path '%s' | Out-Null", directory_path:gsub("'", "''")),
            },
            timeout_ms = 15000,
        })
        if not ok then
            return false, tostring(result)
        end
        if type(result) ~= "table" or not result.ok or not result.success then
            return false, (type(result) == "table" and tostring(result.stderr or result.error or "exec failed")) or "exec failed"
        end
    else
        local ok, result = pcall(vulcan.exec, {
            command = string.format('mkdir -p "%s"', directory_path),
            timeout_ms = 15000,
        })
        if not ok then
            return false, tostring(result)
        end
        if type(result) ~= "table" or not result.ok or not result.success then
            return false, (type(result) == "table" and tostring(result.stderr or result.error or "exec failed")) or "exec failed"
        end
    end

    if vulcan.fs_exists(directory_path) and vulcan.fs_is_dir(directory_path) then
        return true, nil
    end
    return false, "directory was not created by exec fallback"
end

--- 中文：确保共享数据库目录存在，优先使用 LuaFileSystem，失败时回退到宿主命令执行。
--- English: Ensure the shared database directory exists, preferring LuaFileSystem and falling back to host command execution.
--- @param directory_path string 目标目录 / Target directory.
--- @return boolean, string|nil
local function ensure_directory(directory_path)
    if vulcan.fs_exists(directory_path) then
        if vulcan.fs_is_dir(directory_path) then
            return true, nil
        end
        return false, "database root exists but is not a directory"
    end

    if vulcan.osinfo().os == "windows" then
        return ensure_directory_with_exec(directory_path)
    end

    local lfs, lfs_error = load_lfs()
    if lfs ~= nil then
        local ok, error_message = ensure_directory_with_lfs(lfs, directory_path)
        if ok then
            return true, nil
        end
        lfs_error = error_message
    end

    local ok, exec_error = ensure_directory_with_exec(directory_path)
    if ok then
        return true, nil
    end

    return false, tostring(lfs_error or exec_error or "failed to create database directory")
end

--- 中文：执行单条 SQL，并在失败时返回数据库错误信息。
--- English: Execute one SQL statement and return the database error text on failure.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @param sql_text string SQL 文本 / SQL text.
--- @return boolean, string|nil
local function exec_sql(db, sql_text)
    local result = db:exec(sql_text)
    if result ~= 0 then
        return false, tostring(db:errmsg())
    end
    return true, nil
end

--- 中文：在宿主环境中等待指定毫秒数，用于写锁竞争时的退避重试。
--- English: Sleep for a given number of milliseconds in the host environment, used for backoff during write-lock contention.
--- @param milliseconds number 等待毫秒数 / Milliseconds to wait.
local function host_sleep(milliseconds)
    local delay = tonumber(milliseconds) or 0
    if delay <= 0 or type(vulcan.exec) ~= "function" then
        return
    end

    local os_name = vulcan.osinfo().os
    if os_name == "windows" then
        pcall(vulcan.exec, {
            program = "powershell.exe",
            args = {
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                string.format("Start-Sleep -Milliseconds %d", delay),
            },
            timeout_ms = delay + 2000,
        })
    else
        local seconds = string.format("%.3f", delay / 1000)
        pcall(vulcan.exec, {
            command = string.format("sleep %s", seconds),
            timeout_ms = delay + 2000,
        })
    end
end

--- 中文：判断当前 SQLite 错误是否属于可恢复的锁竞争类型。
--- English: Determine whether the current SQLite error represents a recoverable lock-contention condition.
--- @param sqlite table SQLite 模块 / SQLite module.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @param error_message string|nil 错误文本 / Error text.
--- @return boolean
local function is_retryable_lock_error(sqlite, db, error_message)
    local err_text = string.lower(tostring(error_message or ""))
    if string.find(err_text, "locked", 1, true) ~= nil or string.find(err_text, "busy", 1, true) ~= nil then
        return true
    end

    local errcode = nil
    if db ~= nil and type(db.errcode) == "function" then
        local ok, code = pcall(function()
            return db:errcode()
        end)
        if ok then
            errcode = tonumber(code)
        end
    end

    return errcode == sqlite.BUSY or errcode == sqlite.LOCKED
end

--- 中文：为 SQLite 连接启用更适合多线程/多连接写入的数据库设置。
--- English: Enable database settings that are better suited for multi-threaded or multi-connection writes.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @return boolean, string|nil
local function configure_database(db)
    if type(db.busy_timeout) == "function" then
        local ok, timeout_result = pcall(function()
            return db:busy_timeout(SQLITE_BUSY_TIMEOUT_MS)
        end)
        if not ok then
            return false, tostring(timeout_result)
        end
    end

    local pragmas = {
        string.format("PRAGMA busy_timeout = %d;", SQLITE_BUSY_TIMEOUT_MS),
        "PRAGMA journal_mode = WAL;",
        "PRAGMA synchronous = NORMAL;",
        "PRAGMA foreign_keys = ON;",
    }

    for _, sql_text in ipairs(pragmas) do
        local ok, exec_error = exec_sql(db, sql_text)
        if not ok then
            return false, exec_error
        end
    end

    return true, nil
end

--- 中文：初始化工作记忆测试表，保证后续插入和查询语句可用。
--- English: Initialize the work-memory test table so later insert and query statements always have a stable schema.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @return boolean, string|nil
local function ensure_schema(db)
    local sql = [[
CREATE TABLE IF NOT EXISTS work_memory_test_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    note TEXT NOT NULL,
    created_at TEXT NOT NULL
);
]]
    local result = db:exec(sql)
    if result ~= 0 then
        return false, tostring(db:errmsg())
    end
    return true, nil
end

--- 中文：开始显式写事务，优先抢占写锁，从而让锁竞争尽早暴露并进入重试逻辑。
--- English: Start an explicit write transaction with immediate lock acquisition so contention is surfaced early and routed into retry logic.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @return boolean, string|nil
local function begin_write_transaction(db)
    return exec_sql(db, "BEGIN IMMEDIATE;")
end

--- 中文：提交当前事务。
--- English: Commit the current transaction.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @return boolean, string|nil
local function commit_transaction(db)
    return exec_sql(db, "COMMIT;")
end

--- 中文：安全回滚当前事务，避免写入失败后留下未结束事务。
--- English: Roll back the current transaction safely so failed writes never leave an open transaction behind.
--- @param db table SQLite 连接对象 / SQLite database handle.
local function rollback_transaction(db)
    pcall(function()
        db:exec("ROLLBACK;")
    end)
end

--- 中文：向测试表插入一条记录，并返回最新插入的自增主键。
--- English: Insert one row into the test table and return the newest auto-increment primary key.
--- @param sqlite table SQLite 模块 / SQLite module.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @param note string 写入的测试说明 / Test note to persist.
--- @param created_at string ISO 风格时间文本 / ISO-style timestamp text.
--- @return number|nil, string|nil
local function insert_test_entry(sqlite, db, note, created_at)
    local last_error = nil

    for attempt = 1, SQLITE_WRITE_RETRY_MAX_ATTEMPTS do
        local begin_ok, begin_error = begin_write_transaction(db)
        if not begin_ok then
            last_error = tostring(begin_error)
            if is_retryable_lock_error(sqlite, db, last_error) and attempt < SQLITE_WRITE_RETRY_MAX_ATTEMPTS then
                host_sleep(SQLITE_WRITE_RETRY_SLEEP_MS * attempt)
            else
                return nil, last_error
            end
        else
            local statement = db:prepare("INSERT INTO work_memory_test_entries(note, created_at) VALUES(?, ?);")
            if statement == nil then
                rollback_transaction(db)
                return nil, tostring(db:errmsg())
            end

            local write_ok, write_error = pcall(function()
                assert(statement:bind_values(note, created_at))
                local step_result = statement:step()
                if step_result ~= sqlite.DONE then
                    error("unexpected sqlite step result: " .. tostring(step_result))
                end
                statement:finalize()
            end)

            if not write_ok then
                pcall(function()
                    statement:finalize()
                end)
                rollback_transaction(db)
                last_error = tostring(write_error)
                if is_retryable_lock_error(sqlite, db, last_error) and attempt < SQLITE_WRITE_RETRY_MAX_ATTEMPTS then
                    host_sleep(SQLITE_WRITE_RETRY_SLEEP_MS * attempt)
                else
                    return nil, last_error
                end
            else
                local commit_ok, commit_error = commit_transaction(db)
                if commit_ok then
                    return tonumber(db:last_insert_rowid()), nil
                end

                rollback_transaction(db)
                last_error = tostring(commit_error)
                if is_retryable_lock_error(sqlite, db, last_error) and attempt < SQLITE_WRITE_RETRY_MAX_ATTEMPTS then
                    host_sleep(SQLITE_WRITE_RETRY_SLEEP_MS * attempt)
                else
                    return nil, last_error
                end
            end
        end
    end

    return nil, tostring(last_error or "sqlite write retry limit reached")
end

--- 中文：读取测试表中的统计信息和最新一条记录，便于验证读写闭环。
--- English: Read aggregate stats and the latest row from the test table so the read/write loop can be verified.
--- @param db table SQLite 连接对象 / SQLite database handle.
--- @return table, string|nil
local function query_snapshot(db)
    local snapshot = {
        total_rows = 0,
        latest = nil,
    }

    for row in db:nrows("SELECT COUNT(*) AS total_rows FROM work_memory_test_entries;") do
        snapshot.total_rows = tonumber(row.total_rows) or 0
        break
    end

    for row in db:nrows("SELECT id, note, created_at FROM work_memory_test_entries ORDER BY id DESC LIMIT 1;") do
        snapshot.latest = {
            id = tonumber(row.id),
            note = tostring(row.note or ""),
            created_at = tostring(row.created_at or ""),
        }
        break
    end

    return snapshot, nil
end

--- 中文：生成稳定的默认测试内容，确保无参调用也能观察到数据库写入结果。
--- English: Generate a stable default test note so even parameter-free calls visibly write to the database.
--- @return string, string
local function build_default_note()
    local timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ")
    local note = "vulcan-work-memory sqlite test entry"
    return note, timestamp
end

--- 中文：工具主入口，创建共享数据库目录与 SQLite 数据库，完成一次最小的建表、插入、查询验证闭环。
--- English: Main tool entry that creates the shared database directory and SQLite database, then completes one minimal schema/insert/query verification loop.
--- @param args table|nil 工具参数 / Tool arguments.
--- @return table
return function(args)
    local database_root = get_database_root()
    local database_path = get_database_path()

    local directory_ok, directory_error = ensure_directory(database_root)
    if not directory_ok then
        return {
            ok = false,
            error = "database_directory_create_failed",
            message = tostring(directory_error),
            database_root = database_root,
            database_path = database_path,
        }
    end

    local sqlite, sqlite_error, sqlite_module_name = load_sqlite_binding()
    if sqlite == nil then
        return {
            ok = false,
            error = "sqlite_binding_unavailable",
            message = tostring(sqlite_error),
            database_root = database_root,
            database_path = database_path,
        }
    end

    local db = sqlite.open(database_path)
    if db == nil then
        return {
            ok = false,
            error = "sqlite_open_failed",
            message = "sqlite.open returned nil",
            database_root = database_root,
            database_path = database_path,
        }
    end

    local configure_ok, configure_error = configure_database(db)
    if not configure_ok then
        safe_close_database(db)
        return {
            ok = false,
            error = "sqlite_configure_failed",
            message = tostring(configure_error),
            database_root = database_root,
            database_path = database_path,
        }
    end

    local note, created_at = build_default_note()
    if type(args) == "table" and args.note ~= nil and tostring(args.note) ~= "" then
        note = tostring(args.note)
    end

    local schema_ok, schema_error = ensure_schema(db)
    if not schema_ok then
        safe_close_database(db)
        return {
            ok = false,
            error = "sqlite_schema_failed",
            message = tostring(schema_error),
            database_root = database_root,
            database_path = database_path,
        }
    end

    local inserted_id, insert_error = insert_test_entry(sqlite, db, note, created_at)
    if inserted_id == nil then
        safe_close_database(db)
        return {
            ok = false,
            error = "sqlite_insert_failed",
            message = tostring(insert_error),
            database_root = database_root,
            database_path = database_path,
        }
    end

    local snapshot = query_snapshot(db)
    safe_close_database(db)

    return {
        ok = true,
        message = "SQLite test entry inserted into vulcan-work-memory database.",
        database_root = database_root,
        database_path = database_path,
        inserted_id = inserted_id,
        total_rows = snapshot.total_rows,
        latest = snapshot.latest,
        sqlite_module = sqlite_module_name,
    }
end
