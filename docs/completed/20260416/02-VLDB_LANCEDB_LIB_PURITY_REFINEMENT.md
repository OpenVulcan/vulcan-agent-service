# 任务目标

进一步收敛 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 的 `lib` 边界，使库层尽可能只保留与 LanceDB 核心能力、多库管理、通用日志和必要配置相关的内容，不掺杂 gRPC 启动、监听地址、传输层超时等不必要职责。

# 执行步骤

1. 重新梳理当前 `lib` 与 `bin` 的职责边界。
   - 确认哪些配置项、类型和逻辑仅服务于 gRPC 入口。
   - 确认哪些内容应沉淀为纯库层能力。

2. 调整配置模型与模块暴露方式。
   - 将 `lib` 只保留数据库与日志真正需要的配置。
   - 将仅供 gRPC 启动使用的配置留在 `bin` 侧或单独的 server 配置层。

3. 让 `lib` 对外提供更纯净的核心入口。
   - 保留数据库管理器、默认库打开、多库路径解析等核心能力。
   - 避免 `lib` 暴露任何与 gRPC transport 强绑定的对象。

4. 继续验证现有二进制入口不被破坏。
   - 保证 `main.rs` 仍可使用新的纯净 `lib` 入口启动默认服务。

5. 完成验证与记录。
   - 运行编译/测试检查。
   - 追加执行变更总结并归档。

# 技术选型

- 保持单 crate 模式，不额外拆 workspace。
- 通过“核心配置 / 服务配置”分层来提纯库入口。
- `lib` 面向“嵌入与复用”，`main/bin` 面向“gRPC 服务进程”。

# 验收标准

1. `lib` 不再承载与 gRPC 启动直接相关的非必要内容。
2. `main.rs` 仍然可以顺利编译并启动默认服务。
3. `lib` 对外暴露的接口更聚焦于 LanceDB 核心能力与多库管理。

## 执行变更总结

### 1. 核心修复与调整概述

本轮将 `vldb-lancedb` 的库层进一步提纯，明确实现了“`lib` 不读取配置文件、也不依赖 gRPC 传输层”的目标。现在 `lib` 仅导出两类能力：通用日志能力与数据库运行时/多库管理能力；而配置文件解析、服务监听地址、gRPC 超时与并发限制等服务进程专属内容，全部回归 `bin` 入口处理。

### 2. 📂文件变更清单

修改：
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\logging.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\manager.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\config.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\main.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\src\service.rs`

### 3. 💻关键代码调整详情

- `lib.rs` 仅保留 `logging` 与 `manager` 两个模块导出，不再暴露 `config`。
- `logging.rs` 中新增并承接了 `LoggingConfig` 与 `BoxError`，使日志模块成为真正可独立复用的库层组件。
- `manager.rs` 中新增 `DatabaseRuntimeConfig`，承接默认库路径、多库根目录、读一致性与日志配置；`DatabaseManager` 现在只依赖这个纯运行时对象，不依赖配置文件解析结果。
- `config.rs` 调整为二进制专用配置加载器，只负责从 JSON 配置文件解析出 `ResolvedConfig`，再向库层导出 `DatabaseRuntimeConfig`。
- `main.rs` 重新切回本地 `config` 模块，说明配置加载已完全回到二进制入口；库层只接收解析后的运行时参数。
- `service.rs` 改为从库层日志模块获取 `LoggingConfig` 与 `ServiceLogger`，进一步减少 bin 内部重复定义。

### 4. ⚠️遗留问题与注意事项

- 当前 `lib` 已经不掺杂配置文件解析与 gRPC 启动逻辑，但核心 LanceDB 表操作仍主要位于 `service.rs`，尚未完全抽成“纯 Rust core API”；这属于下一阶段工作。
- 当前多库能力已在库层可用，但 gRPC 协议仍默认只服务单个默认数据库；如果后续要让远端请求显式选择库，还需要扩展 proto 与路由。
- 本轮验证已通过：
  - `cargo check`
  - `cargo test --lib`
  - `cargo test --bin vldb-lancedb`
