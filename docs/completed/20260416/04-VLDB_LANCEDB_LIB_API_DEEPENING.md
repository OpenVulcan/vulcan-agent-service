# 任务目标

在保持 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 原始 gRPC 服务行为不变的前提下，继续将当前仍停留在 gRPC 适配层中的高层操作辅助逻辑下沉到 `lib`，让 `lib` 更接近一个可直接被 Lua/MCP/本地 Rust 复用的 LanceDB 能力核心。

# 执行步骤

1. 盘点当前 `service.rs` 中仍然属于“库能力”的逻辑。
   - 区分纯 transport 适配逻辑与可复用的高层辅助逻辑。
   - 明确哪些内容应该继续下沉到 `lib`。

2. 设计下一层 `lib API`。
   - 补充更适合调用方直接消费的高层方法或辅助结构。
   - 保证新增 API 仍然不依赖配置文件、日志实现和 gRPC。

3. 实现下沉改造。
   - 调整 `engine.rs` / `types.rs` / `manager.rs` 等库层模块。
   - 收窄 `service.rs`，让其只承担 protobuf 与库层对象之间的映射。

4. 完成回归验证。
   - 执行 `cargo check`
   - 执行 `cargo test --lib`
   - 执行 `cargo test --bin vldb-lancedb`

5. 记录并归档。
   - 在计划文件末尾追加执行变更总结。
   - 完成后迁移到 `docs/completed/20260416/`。

# 技术选型

- 继续采用单 crate 内模块化演进。
- 保持 `lib` 为纯程序化 API。
- 保持 gRPC 为 transport adapter，不承载可复用业务逻辑。

# 验收标准

1. `service.rs` 中的纯辅助逻辑继续减少。
2. `lib` 暴露的能力更接近调用方直接需要的 LanceDB 高层接口。
3. 原始 gRPC 服务配置方式与行为保持兼容。
4. 编译与测试全部通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次围绕“继续深化 `lib` 的直接可用性，同时不破坏原始 gRPC 单库服务行为”完成了三项关键调整：

- 在 `lib` 中新增了面向调用方的 `runtime` 入口，把 `DatabaseManager + LanceDbEngineOptions + LanceDbEngine` 的组合流程收敛成统一 API。
- 让二进制入口 `main.rs` 也改为通过 `runtime` 打开默认库引擎，进一步验证这层抽象已经足够稳定。
- 收窄了 `service.rs` 的职责，移除已不再需要的构造路径与冗余传输层配置字段，让 gRPC 适配层更加纯粹。

## 2. 📂 文件变更清单

### 新增文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\runtime.rs`

### 修改文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\lib.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\main.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\service.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\README.md`

## 3. 💻 关键代码调整详情

### 新增统一运行时入口

- `src/runtime.rs` 新增 `LanceDbRuntime`：
  - 负责持有 `DatabaseManager`
  - 负责持有默认 `LanceDbEngineOptions`
  - 直接提供 `open_default_engine()` 与 `open_named_engine()` 等方法
- 这样本地 Rust、Lua 宿主或后续 MCP 嵌入层不需要再手动拼装 manager 和 engine，直接拿到可执行核心操作的引擎实例。

### gRPC 入口改为消费 lib runtime

- `src/main.rs` 现在不再直接操作 `DatabaseManager`
- 改为：
  - 从配置解析得到运行时数据库配置
  - 构造 `LanceDbRuntime`
  - 通过 `runtime.open_default_engine()` 获取默认库引擎
  - 再交给 gRPC transport 层使用
- 这一步验证了当前 `lib` 已经足以支撑原始服务启动流程，而不只是给测试代码使用。

### transport 层继续收窄

- `src/service.rs` 删除了不再需要的 `new(db, ...)` 构造路径，统一只保留 `from_engine(...)`
- `ServiceConfig` 中删除了已经下沉到 runtime/engine 的 `max_concurrent_requests`
- 现在 `service.rs` 更接近真正的 protobuf ↔ engine 适配层，而不是半个业务实现层

### 文档同步

- `README.md` 新增 `Library 使用方向`，明确说明：
  - 先用 `runtime::LanceDbRuntime`
  - 再打开默认库或命名库的 `engine`
  - 最后通过 `engine` 执行 LanceDB 核心操作

## 4. ⚠️ 遗留问题与注意事项

- 当前 `lib` 已经具备更直接的 runtime 入口，但如果后续目标是让非 Rust 侧直接消费，还需要下一阶段单独补：
  - `extern "C"` 导出层
  - `.h` 头文件
  - 稳定句柄与错误码协议
- `service.rs` 里仍然保留了 protobuf ↔ 纯 Rust 类型的映射逻辑，这一部分属于 transport adapter 的合理职责，目前不建议继续强行下沉。
- 多库能力依旧只存在于 `lib` 的程序化入口中，原始 gRPC 服务保持单库配置模型不变。
- 本次验证已通过：
  - `cargo check`
  - `cargo test --lib`
  - `cargo test --bin vldb-lancedb`
