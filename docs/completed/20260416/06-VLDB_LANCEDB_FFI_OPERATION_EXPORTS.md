# 任务目标

在已完成 `vldb-lancedb` FFI 生命周期与句柄导出层的基础上，继续为非 Rust 调用方补齐核心业务操作的 FFI 导出能力。目标是在不改变原始 gRPC 服务行为与配置方式的前提下，让 Lua/MCP/manager 等外部调用方能够通过动态库直接执行建表、写入、检索、删除和删表等核心操作。

# 执行步骤

1. 盘点现有 Rust `engine` API 与当前 FFI 句柄层能力。
   - 确认哪些引擎操作需要优先导出。
   - 确认最稳的跨边界数据承载方式。

2. 设计 FFI 操作接口。
   - 尽量使用 `JSON / bytes / string` 作为跨边界载荷。
   - 保持头文件稳定，避免复杂结构体泄漏到 C ABI。
   - 设计成功返回、错误返回和资源释放规则。

3. 实现核心操作导出。
   - 导出 `create_table`
   - 导出 `vector_upsert`
   - 导出 `vector_search`
   - 导出 `delete`
   - 导出 `drop_table`

4. 同步更新头文件与文档。
   - 更新 `include/vldb_lancedb.h`
   - 更新 `README.md`

5. 完成验证与归档。
   - 执行编译与测试。
   - 补写执行变更总结。
   - 迁移到 `docs/completed/20260416/`。

# 技术选型

- 继续使用 `extern "C"` + `cdylib`。
- 输入输出优先采用 `UTF-8 JSON` 和 `bytes`，减少 ABI 复杂度。
- gRPC 继续复用 Rust 内部 API，不直接走 FFI。

# 验收标准

1. 非 Rust 调用方可以通过 FFI 直接执行核心 LanceDB 表操作。
2. 头文件与实现保持一致。
3. 原始 gRPC 服务行为不变。
4. 编译与测试通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次在已完成 FFI 生命周期与句柄导出的基础上，继续补齐了 `vldb-lancedb` 的核心业务操作 FFI 能力。现在非 Rust 调用方已经可以通过动态库直接完成：

- 建表
- 写入
- 向量检索
- 删除
- 删表

同时保持原始 gRPC 服务行为与配置方式不变。当前跨边界协议采用：

- `JSON`：承载元信息输入和大部分结果元信息
- `bytes`：承载写入 payload 与检索结果数据

这样既控制了 ABI 复杂度，也足够让 Lua/MCP/manager 直接开展实际业务调用。

## 2. 📂 文件变更清单

### 修改文件

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\src\\ffi.rs`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\include\\vldb_lancedb.h`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\README.md`

## 3. 💻 关键代码调整详情

### FFI 核心业务操作导出

- 在 `src/ffi.rs` 中新增了以下业务操作导出：
  - `vldb_lancedb_engine_create_table_json`
  - `vldb_lancedb_engine_vector_upsert`
  - `vldb_lancedb_engine_vector_search`
  - `vldb_lancedb_engine_delete_json`
  - `vldb_lancedb_engine_drop_table_json`

这些函数全部基于现有纯 Rust `engine` 能力实现，没有把业务逻辑重新复制一份，而是保持：

- Rust `engine` 负责真实 LanceDB 操作
- FFI 层负责参数解析、错误转换、结果封装

### JSON / bytes 混合边界模型

- 建表、删除、删表：使用 `JSON string -> JSON string`
- 写入：使用 `JSON metadata + raw bytes -> JSON string`
- 检索：使用 `JSON metadata -> JSON string + raw bytes buffer`

这样处理后：

- `Arrow IPC` 这类二进制 payload 不需要被 base64 包裹
- Lua/MCP 等上层也不必处理复杂 C 结构体树
- ABI 仍然保持在稳定、简洁的范围内

### 通用 FFI 辅助能力

- 新增 `VldbLancedbByteBuffer`
- 新增 `vldb_lancedb_bytes_free`
- 增加了一批 JSON 映射、格式解析和输入复制辅助函数
- 通过最近错误缓冲区继续统一承接错误信息

### 头文件同步

- `include/vldb_lancedb.h` 已同步增加上述业务操作声明
- 保证头文件与动态库导出保持一致

### 文档同步

- `README.md` 中的 FFI 部分已改为反映当前真实能力面
- 明确说明 FFI 现在已经支持核心表操作，而不只是生命周期管理

## 4. ⚠️ 遗留问题与注意事项

- 当前 FFI 已经能做核心 LanceDB 操作，但仍然是“底层可调用接口”，并不是最终面向 Lua/MCP 的最高层业务 SDK。
- 目前外部调用方仍然需要自己组织：
  - JSON 输入结构
  - 字节 payload
  - 搜索结果 bytes 的解释方式
- 所以下一阶段如果继续优化，最值得做的不是继续无节制地加新导出，而是：
  - 补一套更高层的调用示例
  - 明确 Lua/MCP 侧的推荐封装方式
  - 甚至补一个轻量 wrapper
- 本次验证已通过：
  - `cargo check`
  - `cargo test --lib`
  - `cargo test --bin vldb-lancedb`
