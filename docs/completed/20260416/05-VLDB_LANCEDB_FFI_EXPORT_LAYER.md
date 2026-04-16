# 任务目标

在保持 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 原始 gRPC 服务行为与配置方式不变的前提下，为现有纯 Rust `lib` 增加可被非 Rust 侧直接消费的 FFI 导出层，并形成稳定的头文件输出策略。该导出层需要面向 MCP/Lua/manager 等调用方，提供数据库运行时创建、引擎获取与基础生命周期控制能力，同时保证错误返回与资源释放边界清晰。

# 执行步骤

1. 梳理当前 `lib` 中适合导出的核心对象与操作边界。
   - 确定哪些类型应保留 Rust 内部使用。
   - 确定哪些能力应通过稳定 FFI 句柄暴露。

2. 设计 FFI 接口模型。
   - 设计 runtime / engine 句柄。
   - 设计错误码与错误消息获取方式。
   - 设计字符串与字节缓冲区的跨边界传递约定。

3. 实现导出层。
   - 新增 `ffi` 模块。
   - 提供 `extern "C"` 导出函数。
   - 保证资源释放函数、错误获取函数、基础实例创建函数齐全。

4. 设计并落地头文件策略。
   - 决定是手写头文件模板还是引入自动生成工具。
   - 确保当前仓库可稳定产出 `.h` 文件或至少提供确定的头文件源。

5. 完成验证与记录。
   - 执行编译与测试验证。
   - 补写执行变更总结并归档。

# 技术选型

- 保持现有单 crate 结构。
- `lib` 新增 `ffi` 模块，导出稳定 C ABI。
- 头文件优先采用仓库内稳定产物策略，避免把发布链路绑定到不必要的额外工具。
- 原始 gRPC 二进制继续使用 Rust API，不直接依赖 FFI。

# 验收标准

1. `lib` 提供可供非 Rust 侧直接调用的稳定 FFI 导出函数。
2. 至少支持运行时创建、默认/命名引擎获取、错误获取与资源释放。
3. 原始 gRPC 服务行为不变。
4. 仓库内存在明确可用的头文件输出方案。
5. 编译与测试通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次完成了 `vldb-lancedb` 的第一版 FFI 导出层落地，使当前纯 Rust `lib` 不再只是 Rust 内部可复用模块，而是已经具备：

- 可被非 Rust 侧加载的 `cdylib`
- 稳定的 C ABI 入口
- 明确的头文件
- 运行时/引擎句柄的创建与释放能力
- 最近一次错误消息获取与清理能力

同时保持了原始 gRPC 二进制入口与配置方式完全不变。

## 2. 📂 文件变更清单

### 新增文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\ffi.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\include\\vldb_lancedb.h`

### 修改文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\Cargo.toml`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\lib.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\README.md`

## 3. 💻 关键代码调整详情

### 动态库导出形态建立

- `Cargo.toml` 新增：
  - `[lib]`
  - `crate-type = ["rlib", "cdylib"]`
- 这样当前 crate 同时保留 Rust 内部复用能力，并能直接产出供外部宿主加载的动态库。
- 实际验证后，Windows 下已经生成：
  - `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\target\\debug\\deps\\vldb_lancedb.dll`

### 新增 FFI 模块

- `src/ffi.rs` 新增第一版稳定 C ABI 导出层。
- 当前已导出的能力包括：
  - `vldb_lancedb_runtime_options_default`
  - `vldb_lancedb_runtime_create`
  - `vldb_lancedb_runtime_destroy`
  - `vldb_lancedb_runtime_open_default_engine`
  - `vldb_lancedb_runtime_open_named_engine`
  - `vldb_lancedb_runtime_database_path_for_name`
  - `vldb_lancedb_engine_destroy`
  - `vldb_lancedb_string_free`
  - `vldb_lancedb_last_error_message`
  - `vldb_lancedb_clear_last_error`
  - 以及基础空句柄判断函数

### FFI 运行时与错误处理策略

- FFI 层新增了全局 Tokio runtime，用于在同步 C ABI 调用中安全驱动异步数据库打开逻辑。
- 使用线程局部错误缓冲区保存最近一次错误消息，避免把 Rust 异常模型直接泄漏给外部宿主。
- FFI 层不读取配置文件，也不依赖 gRPC，只消费已有的：
  - `LanceDbRuntime`
  - `DatabaseRuntimeConfig`
  - `LanceDbEngineOptions`

### 头文件策略落地

- 新增手写稳定头文件：
  - `include/vldb_lancedb.h`
- 当前采用“仓库内稳定头文件产物”策略，而不是立即引入额外自动生成工具。
- 这样可以先把 ABI 冻结在一份可审查、可控、便于 manager/MCP/Lua 直接消费的接口面上。

### 文档同步

- `README.md` 已增加 FFI 入口说明。
- 明确了：
  - 头文件位置
  - 动态库产物形态
  - 当前 FFI 已经覆盖的生命周期管理与错误处理能力

## 4. ⚠️ 遗留问题与注意事项

- 当前 FFI 只完成了“加载、创建 runtime、打开 engine、路径解析、错误获取、资源释放”这一层，还没有继续导出具体表操作（建表、写入、检索、删除、删表）。
- 这意味着当前版本已经足够让非 Rust 宿主打通动态库加载与生命周期管理，但还不够作为完整业务接口。
- 当前头文件采用手写维护策略，优点是稳定可控；如果后续导出函数面快速增长，可以再评估是否引入自动头文件生成工具。
- 本次验证已通过：
  - `cargo check`
  - `cargo test --lib`
  - `cargo test --bin vldb-lancedb`
  - `cargo build --lib`
