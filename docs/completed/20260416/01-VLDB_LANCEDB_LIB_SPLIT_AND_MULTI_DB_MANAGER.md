# 任务目标

对 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 进行结构化改造，使其形成“可执行 gRPC 入口 + 可复用库入口”的双出口形态；在不破坏现有 gRPC 主流程可用性的前提下，将核心 LanceDB 处理逻辑下沉到 `lib`，并补齐多库管理的基础能力，为后续 Lua/本地 Rust 复用打基础。

# 执行步骤

1. 梳理当前仓库结构与职责边界。
   - 确认 `main.rs`、`config.rs`、`service.rs`、proto 生成代码之间的关系。
   - 明确哪些逻辑属于 gRPC 传输层，哪些逻辑应沉到核心库层。

2. 设计并落地 `lib` 入口。
   - 新增 `src/lib.rs`。
   - 将可复用的配置解析、数据库连接创建、服务状态构建、多库管理能力迁入 `lib`。
   - 保证 `main.rs` 只承担薄入口职责。

3. 补齐多库管理基础能力。
   - 设计数据库管理器，支持按名称/路径创建与获取数据库连接。
   - 保持现有单库 gRPC 默认行为可继续工作。
   - 为后续库模式复用预留清晰接口。

4. 校验 gRPC 原始能力不被破坏。
   - 确认原有服务初始化路径仍然成立。
   - 如有必要，调整配置模型以兼容默认库和多库管理。

5. 完成自检与记录。
   - 执行编译检查与必要验证。
   - 对照计划补齐“执行变更总结”。
   - 完成后迁移到 `docs/completed/20260416/`。

# 技术选型

- 采用“单 crate 内部模块分层”的方式先完成本轮拆分，不急于一开始拆成多 crate workspace。
- `lib` 负责核心能力与多库管理，`main` 负责 gRPC 启动入口。
- 优先保持现有 proto 与 gRPC 接口兼容，避免本轮引入过大的协议面变化。

# 验收标准

1. `vldb-lancedb` 形成明确的 `lib + bin` 双入口结构。
2. `lib` 不再以 gRPC 逻辑为中心，而是以核心能力与数据库管理为中心。
3. 保留现有 gRPC 主流程可编译、可启动。
4. 多库管理具备基础创建/获取能力，后续可供 Lua 或本地 Rust 直接复用。

## 执行变更总结

### 1. 核心修复与调整概述

本次已完成 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 的第一阶段结构拆分：新增 `lib` 入口，将配置解析、日志能力与多库数据库管理器下沉到库层；原有 `main.rs` 继续作为 gRPC 薄入口，仅负责启动默认数据库服务。与此同时，配置模型新增了可选 `db_root`，为后续按名称创建/打开多个 LanceDB 数据库提供稳定入口。

### 2. 📂文件变更清单

新增：
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\manager.rs`

修改：
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\config.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\main.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\vldb-lancedb.json.example`

### 3. 💻关键代码调整详情

- 在 `config.rs` 中新增 `db_root` 配置项，并为 `ResolvedConfig` 增加 `database_path_for_name()` 等多库解析能力。
- 新增 `manager.rs`，实现 `DatabaseManager`，支持默认库与命名子库的惰性打开、连接缓存和本地目录创建。
- 新增 `lib.rs`，正式对外导出 `config`、`logging`、`manager` 三个库层模块，使项目同时具备 `bin + lib` 双入口能力。
- 调整 `main.rs`，改为通过 `DatabaseManager` 获取默认数据库连接，保持原有 gRPC 服务启动路径可用。
- 调整 `service.rs`，使其继续作为 gRPC transport 适配层工作，但默认数据库连接来自库层管理器，不再自行承担连接创建职责。
- 更新示例配置与 README，补充 `db_root` 与库模式的使用说明。

### 4. ⚠️遗留问题与注意事项

- 当前多库能力已经在库层就绪，但现有 gRPC 协议仍然默认绑定单个“默认库”；如果要让 gRPC 客户端按请求选择库，还需要后续扩展 proto 与请求路由。
- `service.rs` 里的核心表操作逻辑目前仍偏向 gRPC 请求/响应类型，尚未完全下沉成纯 Rust core API；本次完成的是第一阶段拆层，而不是最终的完整 core 抽离。
- 本次验证已通过：
  - `cargo check`
  - `cargo test --lib`
