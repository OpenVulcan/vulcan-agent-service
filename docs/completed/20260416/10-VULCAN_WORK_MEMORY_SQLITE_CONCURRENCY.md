# 任务目标

为 `vulcan-work-memory` 的 SQLite 访问逻辑预先加入并发写保护，降低未来多 Lua VM / 多线程池同时写入同一数据库时出现 `database is locked`、`SQLITE_BUSY`、`SQLITE_LOCKED` 等问题的概率，避免后续在工作记忆能力扩展后再集中返工。

# 执行步骤

1. 梳理当前 `vulcan-work-memory` 的 SQLite 打开、建表、插入流程，定位多线程并发写的风险点。
2. 在 Lua 侧补充更适合并发写的数据库初始化策略，例如 `WAL`、`busy_timeout`、写事务控制。
3. 为写操作增加显式事务与可恢复重试逻辑，确保遇到短暂锁竞争时优先等待并重试，而不是立即失败。
4. 对读取与写入路径进行必要收敛，避免事务边界和错误回滚遗漏。
5. 通过真实调用和并发调用模拟验证基础行为，确认数据库在重复写入时仍可稳定工作。
6. 在计划文件末尾补充执行变更总结，并归档到 `docs/completed/20260416/`。

# 技术选型

1. 优先使用 SQLite 自身的并发能力，而不是额外引入宿主级全局锁。
2. 连接打开后统一设置：
   - `PRAGMA journal_mode = WAL`
   - `PRAGMA synchronous = NORMAL`
   - `PRAGMA busy_timeout = <毫秒>`
3. 写操作通过显式事务控制：
   - `BEGIN IMMEDIATE`
   - 执行写入
   - `COMMIT`
   - 失败时 `ROLLBACK`
4. 对 `SQLITE_BUSY` / `SQLITE_LOCKED` 相关错误增加有限次数重试，避免瞬时锁竞争直接暴露给调用方。

# 验收标准

1. `vulcan-work-memory` 已具备并发写友好的 SQLite 初始化设置。
2. 写操作已纳入显式事务与失败回滚控制。
3. 遇到锁竞争时，工具会优先等待或重试，而不是立即报错退出。
4. 至少完成一轮真实调用和一轮并发压力式验证，并记录结果。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次为 `vulcan-work-memory` 的 SQLite 访问路径补齐了并发写保护，重点不是新增宿主级锁，而是优先把 SQLite 自身最有效的并发设置和写事务边界提前加进去。最终实现采用了：

- `PRAGMA journal_mode = WAL`
- `PRAGMA synchronous = NORMAL`
- `PRAGMA busy_timeout = 5000`
- `BEGIN IMMEDIATE` 显式写事务
- 写失败时 `ROLLBACK`
- 针对 `BUSY / LOCKED` 的有限重试与退避等待

这样即使后续 Lua VM 池中有多个实例同时写同一个数据库，短暂锁竞争也会优先被吸收在技能内部，而不是立刻暴露成用户侧错误。

## 2. 📂文件变更清单

### 修改
- `runtime/lua_skills/vulcan-work-memory/main.lua`
- `docs/plan/20260416-10-VULCAN_WORK_MEMORY_SQLITE_CONCURRENCY.md`

## 3. 💻关键代码调整详情

### 3.1 SQLite 并发初始化配置
- 新增并发相关常量：
  - `SQLITE_BUSY_TIMEOUT_MS = 5000`
  - `SQLITE_WRITE_RETRY_MAX_ATTEMPTS = 4`
  - `SQLITE_WRITE_RETRY_SLEEP_MS = 120`
- 新增 `configure_database(db)`：
  - 优先调用 `db:busy_timeout(...)`
  - 执行：
    - `PRAGMA busy_timeout`
    - `PRAGMA journal_mode = WAL`
    - `PRAGMA synchronous = NORMAL`
    - `PRAGMA foreign_keys = ON`

### 3.2 写事务边界收口
- 新增事务辅助函数：
  - `begin_write_transaction(db)`
  - `commit_transaction(db)`
  - `rollback_transaction(db)`
- 写入逻辑不再是裸 `INSERT`，而是：
  - `BEGIN IMMEDIATE`
  - prepared statement 执行写入
  - `COMMIT`
  - 任一环节失败则 `ROLLBACK`

### 3.3 锁竞争识别与重试
- 新增 `is_retryable_lock_error(sqlite, db, error_message)`：
  - 文本匹配 `locked` / `busy`
  - 同时兼容 `db:errcode()` 与 `sqlite.BUSY / sqlite.LOCKED`
- 新增 `host_sleep(milliseconds)`：
  - Windows 下通过 `powershell.exe Start-Sleep`
  - Unix 下通过 `sleep`
- `insert_test_entry(...)` 现在会在锁竞争时按有限次数退避重试，而不是首次失败就返回错误

## 4. ⚠️遗留问题与注意事项

1. 当前方案优先依赖 SQLite 自身并发能力，适合：
   - 同一进程内多个 Lua VM
   - 多个 `--call-tools` 进程同时写入同一数据库
   这比额外引入宿主全局锁更适合作为基础实现。
2. 并发验证时，第一次脚本曾因 PowerShell JSON 参数转义错误导致 8 个进程都在参数解析阶段失败；修正为无参数并发调用后，8 个进程均成功退出。
3. 真实并发验证结果：
   - 并发启动 8 个 `vulcan-work-memory-sqlite-test`
   - 全部退出码为 `0`
   - 数据库总行数从 `3` 增长到 `12`
   说明当前 `WAL + busy timeout + 显式事务 + 重试` 的组合已经能覆盖基础并发写场景。
4. 如果后续工作记忆写入复杂度进一步上升（例如长事务、批量 upsert、跨表写入），仍然建议继续控制单次事务粒度，避免把写事务做得过大。
