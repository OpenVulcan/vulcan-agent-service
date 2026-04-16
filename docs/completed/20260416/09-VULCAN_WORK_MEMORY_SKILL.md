# 任务目标

创建一个新的 Lua skill：`vulcan-work-memory`，并注册一个用于验证 SQLite 可用性的测试工具。该工具需要将数据库文件保存在 `runtime/lua_skills/__database/vulcan-work-memory/` 目录下，并遵循现有 Lua skill 共享目录约定，使用 `vulcan.path_join(...)` 结合当前 skill 目录推导数据库路径。

# 执行步骤

1. 创建计划文件并梳理当前 Lua skill 的目录结构、`skill.json` 声明方式以及共享目录（如 `__tools`）的路径约定。
2. 新建 `runtime/lua_skills/vulcan-work-memory/`，补齐 `skill.json` 与 Lua 工具入口文件。
3. 在 Lua 工具中实现 SQLite 测试逻辑：自动创建 `__database/vulcan-work-memory` 目录、初始化数据库、建测试表、写入一条记录并返回查询结果。
4. 补充仓库忽略规则，避免运行期数据库文件进入版本控制。
5. 进行至少一轮静态与真实调用验证，确认 skill 能被加载，SQLite 工具能成功创建数据库并完成读写。
6. 在计划文件末尾补充执行变更总结，并归档到 `docs/completed/20260416/`。

# 技术选型

1. Skill 形态沿用现有 `runtime/lua_skills/` 规范，使用 `skill.json + main.lua`。
2. SQLite 绑定优先使用已接入 Lua 依赖体系的 `lsqlite3complete`。
3. 数据库目录通过当前 skill 目录的上级共享目录推导：
   - `__skill_dir_<lua_module>` -> `../__database/vulcan-work-memory`
4. 目录创建优先使用 LuaFileSystem；如不可用，则回退到宿主 `vulcan.exec`。

# 验收标准

1. 新 skill `vulcan-work-memory` 已存在于 `runtime/lua_skills/` 下，并能被现有加载逻辑识别。
2. 测试工具可以自动创建 `__DATABASE/vulcan-work-memory` 目录和 SQLite 数据库文件。
3. 测试工具至少能完成一次建表、插入、查询闭环，并返回结构化结果。
4. 数据库产物不会进入 Git 版本控制。
5. 至少完成一轮真实调用或等价验证，并记录结果与限制。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次新增了一个独立的 Lua skill：`vulcan-work-memory`，并为其注册了一个最小可运行的 SQLite 测试工具 `vulcan-work-memory-sqlite-test`。该工具会按照与 `__tools` 相同的共享目录思路，将数据库文件落到 `runtime/lua_skills/__database/vulcan-work-memory/` 对应的运行时目录中，并完成建表、插入、查询的完整闭环验证。

同时，本次补充了数据库目录的 Git 忽略规则，避免运行时生成的 SQLite 文件进入版本控制。

## 2. 📂文件变更清单

### 新增
- `runtime/lua_skills/vulcan-work-memory/skill.json`
- `runtime/lua_skills/vulcan-work-memory/main.lua`

### 修改
- `.gitignore`
- `docs/plan/20260416-09-VULCAN_WORK_MEMORY_SKILL.md`

## 3. 💻关键代码调整详情

### 3.1 新增 Lua skill 元数据
- 在 `runtime/lua_skills/vulcan-work-memory/skill.json` 中新增 `vulcan-work-memory` skill。
- 注册工具：
  - `vulcan-work-memory-sqlite-test`
- 参数设计：
  - `note`：可选测试内容，用于插入测试表。

### 3.2 新增 SQLite 测试工具入口
- 在 `runtime/lua_skills/vulcan-work-memory/main.lua` 中实现完整工具逻辑：
  - 解析当前 skill 目录
  - 推导共享数据库目录 `../__database/vulcan-work-memory`
  - Windows 下优先使用宿主 `vulcan.exec + powershell.exe` 创建目录
  - Unix 类环境优先使用 LuaFileSystem，失败时回退到宿主命令
  - 优先加载 `lsqlite3complete`，必要时回退到 `lsqlite3`
  - 自动建表 `work_memory_test_entries`
  - 插入一条测试记录
  - 查询总记录数与最新记录
  - 返回结构化结果（数据库路径、插入 ID、总数、最新记录等）

### 3.3 运行时数据库目录忽略
- 在 `.gitignore` 中新增：
  - `/runtime/lua_skills/__database/`
- 这样数据库文件只作为运行时产物存在，不会污染仓库提交面。

## 4. ⚠️遗留问题与注意事项

1. 真实验证是通过 `output/debug/vulcan-mcp.exe --call-tools` 完成的，因此数据库实际生成在运行时同步后的目录：
   - `output/lua_skills/__database/vulcan-work-memory/work-memory.sqlite3`
   这与源目录 `runtime/lua_skills/__database/...` 的约定是一致的，只是运行时会先同步到 `output/`。
2. Windows 宿主的路径校验较严格，直接把带 `..` 或混合分隔符的路径交给 `vulcan.fs_exists` 会报错，因此最终采用了“先规整路径，再在 Windows 上优先使用 `vulcan.exec` 建目录”的实现方式。
3. 目前该 skill 只提供最小测试工具，后续如果要将工作记忆逻辑真正迁入 Lua，可以继续在这个 skill 下扩展：
   - schema 初始化
   - 记忆写入策略
   - 查询/召回接口
   - 清理与迁移逻辑
4. 本次没有引入新的 Rust 宿主桥接层，SQLite 读写完全通过 Lua 侧 `lsqlite3complete` 完成，符合“主程序不再承担 SQLite 中转”的目标方向。
