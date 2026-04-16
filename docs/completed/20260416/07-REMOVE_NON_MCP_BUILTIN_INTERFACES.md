# 任务目标

本次任务需要进一步收缩 MCP 服务端的内建接口面：将不再用于 MCP 的旧内建工具从服务端接口层彻底移除，只保留以下两类能力继续对外暴露：

1. `vmcp_scratchpad_*`
2. `runlua`

需要明确区分“移除 MCP 接口”与“移除底层 gRPC 对接”：

- `lancedb`、`sqlite` 等底层 gRPC client 保持可连接能力，不在本轮删除。
- 本轮只清理 `server.rs` 中面向 MCP 的默认注册与 `tools/call` 分发接口。

# 执行步骤

1. 创建计划文件并梳理 `src/server.rs` 中所有仍属于旧内建 MCP 接口的注册与调用分支。
2. 删除 `add`、`greet`、`current_time`、`lancedb_*`、`sqlite_*` 等非 MCP 用旧内建工具的默认注册与 `tools/call` 分支。
3. 删除仅服务于这些旧内建工具的辅助函数，保留 `vmcp_scratchpad_*`、`runlua` 及其所需逻辑。
4. 保留底层 `LanceDbClient`、`SqliteClient`、`with_lancedb()`、`with_sqlite()` 等连接能力，不动底层 gRPC 集成。
5. 运行编译校验，确认服务端仍可正常构建，并补充执行总结后归档。

# 技术选型

1. 直接在 `src/server.rs` 做收口，避免增加新的“禁用开关”或兼容层。
2. 用“删除接口分支”而不是“继续保留但不注册”的方式，彻底移除旧 MCP 入口，降低后续维护歧义。
3. 保持 `vmcp_scratchpad_*` 与 `runlua` 的 schema、调用路径和初始化说明不变。

# 验收标准

1. `tools/list` 默认仅包含 Lua skills、`vmcp_scratchpad_*` 与 `runlua`。
2. `tools/call` 不再存在 `add`、`greet`、`current_time`、`lancedb_*`、`sqlite_*` 的分发分支。
3. `src/server.rs` 中与上述已删除接口仅相关的辅助函数被同步清理。
4. 底层 `with_lancedb()`、`with_sqlite()` 等 gRPC 对接能力仍保留。
5. `cargo check` 通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

执行过程中，用户将范围临时收窄为“先只彻底销毁 `add`、`greet`、`current_time`”。因此本轮没有继续扩大到 `lancedb_*`、`sqlite_*` 等其他历史内建接口，而是仅对这三个旧 MCP 内建工具做了彻底移除：

- 删除对应工具实现函数；
- 删除 `tools/call` 中的分发分支；
- 保持其他保留能力（如 `vmcp_scratchpad_*`、`runlua`）不受影响。

## 2. 📂 文件变更清单

- 修改：`src/server.rs`
- 修改：`docs/plan/20260416-07-REMOVE_NON_MCP_BUILTIN_INTERFACES.md`

## 3. 💻 关键代码调整详情

- 删除了 `tool_add()`、`tool_greet()`、`tool_time()` 以及仅供 `current_time` 使用的 `utc_now()`。
- 删除了 `handle_tools_call()` 中以下匹配分支：
  - `add`
  - `greet`
  - `current_time`
- 保留了 `vmcp_scratchpad_*`、`runlua`、以及其他尚未继续处理的旧分支，避免在用户尚未确认前扩大修改范围。

## 4. ⚠️ 遗留问题与注意事项

- `lancedb_*`、`sqlite_*` 等旧内建接口目前仍保留在 `handle_tools_call()` 中，本轮未继续处理；如果下一步确认要继续收口，可以按同样方式继续删除。
- 本次验证已执行 `cargo check`，未做端到端 MCP 调用验证。
