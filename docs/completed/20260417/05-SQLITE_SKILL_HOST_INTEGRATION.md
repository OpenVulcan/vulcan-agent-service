# SQLite Skill 宿主接入与独立配置集成计划

## 任务目标

为 `vulcan-mcp-client` 增加与 `lancedb` 对齐的 `sqlite` skill 级宿主接入能力，确保：

1. skill 可以像 `lancedb` 一样通过独立配置声明启用 `sqlite`；
2. 宿主为启用的 skill 自动分配固定数据库路径；
3. Lua 侧获得统一的 `vulcan.sqlite` 能力入口；
4. 未启用的 skill 不会误接 SQLite；
5. 该能力默认走宿主侧统一管理，而不是让 skill 自己直接管理数据库文件。

## 设计原则

1. **配置隔离**
   - SQLite 需要有独立于 `lancedb` 的 skill 级配置结构；
   - 不与远程 gRPC `sqlite` 配置混淆。

2. **路径固定**
   - 每个启用 SQLite 的 skill 仅绑定一个数据库；
   - 数据库目录/文件规则固定，避免串库。

3. **宿主管理**
   - 宿主负责数据库创建、打开、复用、关闭；
   - Lua 不直接决定数据库路径和宿主级生命周期。

4. **行为与 LanceDB 对齐**
   - 启用 skill 注入能力；
   - 未启用 skill 给出稳定报错或状态对象；
   - 结构和接入体验尽量与 `lancedb` 保持一致。

## 执行步骤

### 步骤 1：梳理当前 LanceDB skill 接入实现

- 检查：
  - `src/lua_skill.rs`
  - `src/lua_engine.rs`
  - `src/lancedb_host.rs`
- 提炼：
  - skill 元数据结构
  - 启用开关与配置形态
  - 宿主注册与 Lua 注入方式

### 步骤 2：设计 SQLite 的 skill 配置结构

- 在 `skill.json` 元数据层增加 SQLite 独立配置；
- 与 `lancedb` 配置保持风格一致；
- 明确启用、日志、路径等字段边界。

### 步骤 3：实现 SQLite 宿主级 skill 绑定

- 新增或启用 `sqlite_host` 作为宿主级 skill 管理器；
- 为每个启用 SQLite 的 skill：
  - 自动解析固定数据库路径
  - 自动创建数据库目录/文件
  - 注册宿主绑定

### 步骤 4：Lua 注入 `vulcan.sqlite`

- 仅对启用 skill 注入 `vulcan.sqlite`
- 未启用时返回稳定状态或明确错误
- 方法面与当前宿主 typed 包装能力对齐

### 步骤 5：补示例与验证

- 创建或更新一个 SQLite 测试 skill
- 验证：
  - 启用 skill 能正常用 SQLite
  - 未启用 skill 不会误连
  - 宿主路径和生命周期符合预期

## 验收标准

1. `skill.json` 已支持 SQLite 独立配置
2. 宿主能为启用 SQLite 的 skill 自动创建并绑定数据库
3. Lua 侧可稳定使用 `vulcan.sqlite`
4. 未启用 SQLite 的 skill 不会误接数据库
5. 至少一个 SQLite 测试 skill 能跑通完整调用链

---

## 执行变更总结

### 1. 核心修复与调整概述

- 为 `vulcan-mcp-client` 新增了与 `lancedb` 对称的 `sqlite` skill 宿主接入链路；
- `skill.json` 元数据层新增 SQLite 独立配置对象，并支持旧布尔字段兼容合并；
- 宿主新增动态库版 `sqlite_host`，按 skill 自动分配 `__database/<skill_dir_name>/<skill_dir_name>.sqlite3`；
- Lua 侧新增 `vulcan.sqlite` 注入对象，覆盖状态、分词、词典、FTS 索引与检索能力；
- 将 `vulcan-work-memory` 改造成真正走宿主管理 SQLite 的测试 skill，并完成端到端验证。

### 2. 📂文件变更清单

- 新增：
  - `D:\projects\vulcan-mcp-client\src\sqlite_host.rs`
- 修改：
  - `D:\projects\vulcan-mcp-client\src\lua_skill.rs`
  - `D:\projects\vulcan-mcp-client\src\lua_engine.rs`
  - `D:\projects\vulcan-mcp-client\src\main.rs`
  - `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-work-memory\skill.json`
  - `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-work-memory\main.lua`
- 迁移归档：
  - `D:\projects\vulcan-mcp-client\docs\plan\20260417-05-SQLITE_SKILL_HOST_INTEGRATION.md`

### 3. 💻关键代码调整详情

- `src/sqlite_host.rs`
  - 基于 `vldb_sqlite.dll/.so/.dylib` 动态加载实现 `SqliteSkillHost` 与 `SqliteSkillBinding`；
  - 通过非 JSON C ABI 主接口完成：
    - runtime 创建
    - 数据库打开
    - 分词
    - 自定义词写入/删除/列表
    - FTS 索引确保/重建
    - 文档写入/删除
    - FTS 检索
  - 按 skill 目录固定数据库路径为 `__database/<skill_dir_name>/<skill_dir_name>.sqlite3`。
- `src/lua_skill.rs`
  - 新增 `SkillSqliteLogLevel`、`SkillSqliteMeta`；
  - 新增 `sqlite_enable` 与 `sqlite` 配置；
  - 新增 `effective_sqlite()` 合并逻辑。
- `src/lua_engine.rs`
  - 为每个已加载 skill 保存 `sqlite_binding`；
  - 宿主启动时注册 SQLite skill 绑定；
  - 新增 `populate_vulcan_sqlite_context()`；
  - `vulcan.call` 跨 skill 调用时，增加 SQLite 上下文切换与恢复逻辑。
- `runtime/lua_skills/vulcan-work-memory/main.lua`
  - 去掉原先直接依赖 `lsqlite3` 的数据库文件管理逻辑；
  - 改为通过 `vulcan.sqlite` 完成：
    - 索引确保
    - 文档写入
    - 检索验证
    - 词典列表读取
  - 验证宿主管理 SQLite 的完整闭环。

### 4. ⚠️遗留问题与注意事项

- 当前 `vulcan.sqlite` 宿主层主要使用的是 `vldb-sqlite` 的非 JSON C ABI 主接口，并在 Lua 边界转换成 table；
- 现阶段主要验证的是“启用 skill 正常绑定与使用”的链路，未单独新增一个“未启用 SQLite 且主动误调用”的专门测试 skill；
- `vulcan-work-memory` 目前已切到宿主管理 SQLite，旧的 Lua 侧 `lsqlite3` 直连测试路径已不再作为主验证链路；
- 若后续要继续补统一错误码、更多宿主诊断字段或专门的禁用场景测试，可以在此基础上继续扩展。
