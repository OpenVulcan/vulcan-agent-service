# 任务目标

在当前 `D:\projects\vulcan-mcp-client` 中实现基于 skill 的 LanceDB 宿主管理能力，并新增一个名为 `vulcan-ai-memory` 的 Lua skill。该能力要求：

- `skill.json` 仅通过 `lancedb_enable: true/false` 声明是否启用 LanceDB
- 每个启用 LanceDB 的 skill 只能绑定一个数据库
- 数据库目录固定为 `__lancedb/<skill_dir_name>`
- 宿主自动为已启用的 skill 初始化并管理 LanceDB 运行时
- Lua 不直接接触 FFI 和数据库创建逻辑，只通过宿主注入的 `vulcan.lancedb` 接口访问当前 skill 对应的数据库

# 执行步骤

1. 梳理当前 Lua skill 元数据加载流程与 `vulcan` 注入逻辑。
   - 找出 `skill.json` 解析结构
   - 找出 Lua VM 中 `vulcan.*` 能力注入点

2. 设计 LanceDB skill 级能力模型。
   - 增加 `lancedb_enable` 元数据
   - 设计宿主级 skill→数据库路径映射
   - 设计 `vulcan.lancedb` Lua API 的最小能力面

3. 实现宿主集成。
   - 在 skill 加载阶段识别 `lancedb_enable`
   - 按 skill 目录名映射到 `__lancedb/<skill_dir_name>`
   - 初始化或懒加载 LanceDB 运行时
   - 在 Lua 中只对已启用 skill 注入 `vulcan.lancedb`

4. 新增 `vulcan-ai-memory` skill。
   - 创建 skill 目录与 `skill.json`
   - 注册一个最小测试工具或调用入口
   - 验证其能获得宿主注入的 LanceDB 接口

5. 回归验证与归档。
   - 执行编译检查与必要调用验证
   - 记录执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 以宿主封装为主，不让 Lua 直接调用 FFI
- 每个 skill 只允许一个 LanceDB 库
- 数据目录固定为 `__lancedb/<skill_dir_name>`
- 先实现最小可用 API，再逐步扩展能力面

# 验收标准

1. `skill.json` 支持 `lancedb_enable` 元数据。
2. 宿主能为启用该能力的 skill 建立固定目录的 LanceDB 上下文。
3. `vulcan.lancedb` 仅对启用 skill 可用，未启用时不会误开放。
4. 新 skill `vulcan-ai-memory` 已创建并接通宿主能力。
5. 编译与最小运行验证通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次在当前 `vulcan-mcp-client` 中完成了基于 skill 的宿主管理 LanceDB 集成，并新增了 `vulcan-ai-memory` Lua skill。

核心结果如下：

- 为 `skill.json` 增加了顶层 `lancedb_enable` 元数据，允许 skill 显式声明是否需要宿主管理的 LanceDB 实例
- 新增宿主级 `lancedb_host` 动态库桥接模块，负责加载 `vldb_lancedb.dll/.so/.dylib`，并按 skill 目录名自动创建固定数据库目录 `__lancedb/<skill_dir_name>`
- 在 Lua 调用链中新增 `vulcan.lancedb` 注入，仅对启用该能力的 skill 开放；未启用时返回统一“当前 skill 未启用 lancedb”错误
- 新增 `vulcan-ai-memory` skill，并通过真实调用验证了建表、写入和向量检索链路
- 保持原有 MCP 配置协议与原始 gRPC/Lua skill 运行逻辑不变，没有把 LanceDB 多库配置反向侵入主配置文件

## 2. 📂文件变更清单

### 新增文件

- `src/lancedb_host.rs`
- `runtime/lua_skills/vulcan-ai-memory/skill.json`
- `runtime/lua_skills/vulcan-ai-memory/main.lua`

### 修改文件

- `.gitignore`
- `Cargo.toml`
- `Cargo.lock`
- `src/main.rs`
- `src/lua_skill.rs`
- `src/lua_engine.rs`
- `docs/lua_skills.md`

## 3. 💻关键代码调整详情

### 3.1 skill 元数据扩展

- 在 `src/lua_skill.rs` 的 `SkillMeta` 中新增 `lancedb_enable: bool`
- 保持默认值为 `false`，不影响现有未启用 LanceDB 的 skill

### 3.2 宿主 LanceDB 动态库桥接

- 在 `src/lancedb_host.rs` 中实现了对 `vldb_lancedb` FFI 导出的宿主封装
- 支持按优先级查找动态库：
  - `VLDB_LANCEDB_LIBRARY`
  - 运行时 `libs/`
  - `output/libs/`
  - 本地开发期 `../VulcanLocalDataGateway/vldb-lancedb/target/release/`
- 宿主负责：
  - 为每个启用 skill 自动创建 `__lancedb/<skill_dir_name>`
  - 创建 runtime
  - 打开默认 engine
  - 长期持有并在释放时统一销毁

### 3.3 Lua 上下文注入

- 在 `src/lua_engine.rs` 中新增 `populate_vulcan_lancedb_context`
- `call_skill`、`run_skill_helper`、`vulcan.call` 现在都会按目标 skill 切换 `vulcan.lancedb` 上下文
- 启用 skill 时开放：
  - `info`
  - `create_table`
  - `vector_upsert`
  - `vector_search`
  - `delete`
  - `drop_table`
- 未启用时注入禁用代理，统一返回明确错误，不再暴露空引用式模糊失败

### 3.4 `vulcan-ai-memory` skill

- 新建 `runtime/lua_skills/vulcan-ai-memory`
- 在 `skill.json` 中开启 `lancedb_enable: true`
- 新增测试工具 `vulcan-ai-memory-lancedb-test`
- 工具内部完成：
  - 读取宿主管理的 LanceDB 信息
  - 创建 demo 表
  - upsert 一条向量记录
  - 执行一次向量检索

### 3.5 文档与目录约定

- 在 `docs/lua_skills.md` 中补充：
  - `lancedb_enable`
  - 固定目录 `__lancedb/<skill_dir_name>`
  - `vulcan.lancedb` 最小能力面
- 在 `.gitignore` 中增加 `runtime/lua_skills/__lancedb/`

## 4. ⚠️遗留问题与注意事项

- 当前宿主已经能真实加载 `vldb_lancedb` release 动态库，但还没有把该动态库自动纳入当前仓库的标准构建产物分发链；本次验证阶段通过手动编译 `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb` 的 release DLL 并放入 `output/libs/` 完成验证
- 当前 `vulcan.lancedb` 已支持基本表操作，但还没有继续向更高层记忆工作流抽象（如固定 schema 初始化、业务级检索包装）；这更适合作为 `vulcan-ai-memory` 后续演进工作
- 工作区中仍存在与本任务无关的未跟踪文件：
  - `docs/completed/20260416/01-06` 相关记录文件
  - `tmp/`
  本次未处理

## 5. 验证记录

本次已实际执行并通过以下验证：

- `cargo build --release --lib`（在 `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb`）
- `cargo check`
- `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`
- `output\\debug\\vulcan-mcp.exe --call-tools vulcan-ai-memory-lancedb-test "{}"`
- `output\\debug\\vulcan-mcp.exe --call-tools vulcan-work-memory-sqlite-test '{"note":"verify sqlite unaffected"}'`
