# 任务目标

完善当前 `vulcan-mcp-client` 中的 skill 级 LanceDB 宿主集成，重点解决以下问题：

- 为未启用 LanceDB 的 skill 提供稳定、可判定的 `vulcan.lancedb` 状态接口，而不是仅依赖报错
- 将 `skill.json` 中的 LanceDB 配置从单一布尔开关扩展为对象配置，支持日志级别与慢操作日志控制
- 在宿主层补充 LanceDB 调用日志与慢操作日志策略
- 明确并验证多 Lua VM / 多 Lua 池并发调用同一 skill LanceDB 实例时不会出现宿主级句柄竞争问题
- 保持原有 MCP 主配置与既有运行逻辑不被破坏

# 执行步骤

1. 梳理当前 LanceDB skill 宿主链路与已实现的并发保护点。
   - 核对 `skill.json` 解析结构
   - 核对 `lancedb_host` 当前调用串行化方式
   - 确认 `vulcan.lancedb` 未启用时的现状

2. 扩展 skill 元数据中的 LanceDB 配置模型。
   - 设计 `lancedb` 对象结构
   - 支持 `enable/log_level/slow_log_enabled/slow_log_threshold_ms`
   - 保持兼容当前 `lancedb_enable` 简单开关

3. 实现宿主日志与状态接口。
   - 为 `vulcan.lancedb` 增加稳定 `status()/info()` 返回
   - 为宿主 LanceDB 调用增加日志级别控制
   - 为慢操作增加单独阈值与日志输出

4. 强化并发安全验证。
   - 明确 skill 级实例互斥策略
   - 补充并发测试 skill/tool 或脚本验证
   - 确认多 Lua 池并发调用时不会错误共享或交叉释放句柄

5. 回归验证与归档。
   - 执行编译检查与并发测试
   - 补写执行变更总结
   - 归档到 `docs/completed/20260416/`

# 技术选型

- 继续由宿主管理 LanceDB 动态库与实例生命周期
- skill 级 LanceDB 配置收敛为对象，不扩散到全局主配置
- 对每个 skill 的 LanceDB 实例继续使用宿主级串行互斥，优先保证稳定性而不是抢并发
- 宿主日志与慢操作日志在 Rust 层处理，不让 Lua 直接管理句柄级日志

# 验收标准

1. `skill.json` 支持 `lancedb` 对象配置，至少支持 `enable`、`log_level`、`slow_log_enabled`。
2. 未启用 LanceDB 的 skill 可以通过稳定状态字段判断不可用，而不会误连数据库。
3. 宿主可根据 skill 配置输出 LanceDB 普通日志与慢操作日志。
4. 多 Lua VM 并发调用同一 skill 的 LanceDB 实例不会出现错误绑定、交叉释放或宿主级竞争异常。
5. 编译与最小运行验证通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 完成了 skill 级 LanceDB 配置模型扩展，将原本单一的布尔开关升级为 `lancedb` 对象配置，支持启用开关、日志级别、慢操作日志开关与阈值。
- 在宿主侧新增了 LanceDB 动态库桥接与 skill 级实例管理能力，按 skill 目录名固定映射到 `__lancedb/<skill_dir_name>`，并通过宿主互斥保护串行化 FFI 调用。
- 为未启用 LanceDB 的 skill 提供了稳定的 `status()/info()` 状态返回；启用写操作时若未初始化，则返回明确错误而不是错误串库。
- 对 `vulcan-codekit` 的 overflow 指针文案做了语义修正，去掉了误导性的 “Host Safe” 表述，改为明确的 raw file 读取计划说明。

## 2. 📂文件变更清单

### 新增

- `D:\projects\vulcan-mcp-client\src\lancedb_host.rs`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-ai-memory\skill.json`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-ai-memory\main.lua`

### 修改

- `D:\projects\vulcan-mcp-client\src\lua_skill.rs`
- `D:\projects\vulcan-mcp-client\src\lua_engine.rs`
- `D:\projects\vulcan-mcp-client\docs\lua_skills.md`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\shared_overflow.lua`
- `D:\projects\vulcan-mcp-client\.gitignore`
- `D:\projects\vulcan-mcp-client\Cargo.toml`
- `D:\projects\vulcan-mcp-client\Cargo.lock`
- `D:\projects\vulcan-mcp-client\src\main.rs`

## 3. 💻关键代码调整详情

- `src/lua_skill.rs`
  - 新增 `SkillLanceDbMeta` 与 `SkillLanceDbLogLevel`，并通过 `effective_lancedb()` 兼容旧 `lancedb_enable`。
- `src/lancedb_host.rs`
  - 新增宿主级 LanceDB FFI 加载、skill 专属数据库路径推导、日志输出、慢日志输出以及 `status_json()/info_json()`。
  - 使用 `Mutex<SkillHandleState>` 串行化 skill 级 runtime/engine 句柄访问，优先保证多 Lua 池稳定性。
- `src/lua_engine.rs`
  - 按 skill 注入 `vulcan.lancedb`，启用 skill 提供完整操作接口，未启用 skill 提供稳定 disabled 状态与统一错误。
  - 在跨 skill `vulcan.call` 场景中切换并恢复对应的 LanceDB 绑定上下文。
- `runtime/lua_skills/vulcan-codekit/shared_overflow.lua`
  - 将 `Host Safe Read Chunks` 改为 `Raw File Read Chunks`。
  - 将 overflow 原因与读取策略改为基于 raw file 行边界的表述，明确说明 `offset + limit` 仅作为首轮读取建议，而不是宿主绝对安全保证。

## 4. ⚠️遗留问题与注意事项

- 当前 `vulcan-ai-memory` 仍以测试工具为主，后续若要作为正式工作记忆 skill 使用，还需要继续抽象表初始化、记忆写入与检索接口。
- `vulcan-codekit` 的历史归档文档中仍保留旧的 “Host Safe Read Chunks” 描述；本次未回写历史记录，仅修正当前运行时行为与文案。
- 当前工作区里仍存在本任务之外的既有未跟踪归档文件与 `tmp/` 目录，本次没有清理。
