# 任务目标

在不改变 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 原始 gRPC 配置方式、运行方式与单库服务行为的前提下，设计并实现一个真正可嵌入的 `lib` 核心层。该 `lib` 不依赖配置文件、不负责服务监听、不持有固定日志策略，只通过导出方法和类型提供 LanceDB 实例化与核心数据库操作能力，供 MCP/Lua/本地 Rust 直接调用。

# 执行步骤

1. 梳理当前 `service.rs` 中可下沉为纯库能力的核心逻辑。
   - 识别与 gRPC 强绑定的 request/response/status 部分。
   - 识别可抽象为纯 Rust 输入/输出结构的 LanceDB 操作逻辑。

2. 设计嵌入式库接口。
   - 设计实例配置类型。
   - 设计引擎对象与方法。
   - 设计纯 Rust 输入/输出类型。
   - 保证库层不读取配置文件、不依赖日志配置。

3. 实现核心引擎。
   - 新增 `engine` 与 `types` 等模块。
   - 将建表、写入、检索、删除、删表等核心逻辑迁入引擎。
   - 保留必要的并发控制与错误处理。

4. 保持原始 gRPC 行为不变。
   - `main.rs` 继续走原配置加载与原服务启动路径。
   - `service.rs` 改为调用 lib engine，而不是自己承载核心操作。

5. 完成验证与记录。
   - 执行编译和测试校验。
   - 追加执行变更总结并归档。

# 技术选型

- 采用单 crate 内模块化改造。
- `lib` 仅导出核心引擎与多实例管理能力。
- `bin` 继续保留原配置文件与日志策略。
- gRPC 层仅作为 transport adapter。

# 验收标准

1. `lib` 不依赖配置文件、不依赖 gRPC、不内置固定日志策略。
2. `lib` 提供明确的实例化入口与核心 LanceDB 操作方法。
3. 原始 gRPC 二进制配置方式与服务行为保持兼容。
4. 核心操作从 `service.rs` 显著下沉到 `lib`。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次围绕“保持原始 gRPC 服务运行逻辑不变，同时新增可嵌入的纯程序化 lib 能力”完成了三类核心调整：

- 将 `vldb-lancedb` 拆分为更清晰的 `lib + bin + gRPC transport` 结构。
- 在 `lib` 中新增纯 Rust 的 LanceDB 类型定义、嵌入式引擎与多库管理能力，不再依赖配置文件或 gRPC 请求类型。
- 将前一轮错误引入到原始配置协议中的 `db_root` 扩展完全收回，恢复原始二进制的单库配置模型与外部使用方式。

## 2. 📂 文件变更清单

### 新增文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\types.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\engine.rs`

### 修改文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\lib.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\manager.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\config.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\logging.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\service.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\main.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\README.md`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\vldb-lancedb.json.example`

## 3. 💻 关键代码调整详情

### `lib` 纯化与核心能力下沉

- `src/lib.rs` 现在只导出 `manager`、`types`、`engine` 三个纯库模块，不再导出配置解析或 gRPC 相关内容。
- `src/types.rs` 新增纯 Rust 输入输出结构，专门承接建表、写入、检索、删除、删表等操作，彻底摆脱 protobuf request/response 类型。
- `src/engine.rs` 新增 `LanceDbEngine`，统一承载 LanceDB 核心操作、并发控制、表级读写协调与结果编码逻辑，供 gRPC 与未来 Lua/MCP 上层共同复用。

### 多库能力只保留在程序化入口

- `src/manager.rs` 保留 `DatabaseRuntimeConfig` 与 `DatabaseManager`，允许调用方通过代码传入 `default_db_path`、`db_root` 等运行时参数。
- 多库能力不再由配置文件驱动，而是只存在于 `lib` 的实例化 API 里，符合“原始服务不改、嵌入式调用可扩展”的目标。

### 原始二进制行为回正

- `src/config.rs` 移除了 `Config` 中的 `db_root` 字段，恢复原始配置协议只负责默认数据库路径。
- `src/main.rs` 继续使用配置文件加载默认库、初始化日志并启动 gRPC 服务，没有改变原始单库启动方式。
- `src/service.rs` 继续作为 gRPC transport adapter，但内部已改为调用 `LanceDbEngine`，自身不再承载核心数据库逻辑。

### 编译与适配修复

- 修复了 `main.rs` 中错误的 `BoxError` 导入路径。
- 修复了 `service.rs` 中 `vector_upsert` 与 `vector_search` 的部分移动问题，避免在移动字段后再次访问 `req` 导致的编译失败。
- 同步收回 README 与示例配置中关于 `db_root` 的公开描述，避免误导外部使用者认为原始 gRPC 服务支持该配置。

## 4. ⚠️ 遗留问题与注意事项

- 当前 `lib` 已经具备纯程序化实例化与核心操作能力，但 gRPC 协议仍默认只绑定“默认库”，没有扩展“请求中显式指定库名”的多库路由，这与当前“原始服务保持不变”的目标一致。
- `manager` 中的多库能力依然存在，并且是后续 Lua/MCP 嵌入式调用的主要入口；调用方如果要做每 skill 独立库，需要自行在程序层传入 `DatabaseRuntimeConfig`。
- `logging` 仍属于二进制侧组件，`lib` 不负责任何固定日志输出策略；后续如果要给 Lua 或宿主层暴露日志控制，需要在上层自行注入。
- 本次验证已通过：
  - `cargo check`
  - `cargo test --lib`
  - `cargo test --bin vldb-lancedb`
