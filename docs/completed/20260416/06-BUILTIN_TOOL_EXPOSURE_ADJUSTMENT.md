# 任务目标

本次任务需要调整 MCP 默认暴露的内建工具范围，不再保持“仅 Lua skill”这一刀切策略，而是改为仅保留两类具备明确产品价值的内建能力：

1. `vmcp_scratchpad_*`
2. `runlua`

除此之外，其他历史内建工具（如 `add`、`greet`、`current_time`、`lancedb_*`、`sqlite_*`）继续保持不注册、不默认暴露。

# 执行步骤

1. 审阅当前 `McpServer::register_defaults()` 与 `handle_tools_call()` 的实际调用路径，确认默认注册面与真实可调用能力的关系。
2. 在 `src/server.rs` 中恢复仅 `vmcp_scratchpad_*` 与 `runlua` 的默认注册逻辑，确保它们能出现在 `tools/list` 中并通过 `tools/call` 正常访问。
3. 保持其他历史内建工具不注册、不默认暴露，避免对外协议面重新膨胀。
4. 同步修正文案说明，避免继续写成“默认仅 Lua skills”，而应准确描述为“默认暴露 Lua skills，以及少量保留内建能力”。
5. 运行编译校验，确认注册调整未破坏现有协议与服务端构建。

# 技术选型

1. 继续沿用现有 `Tool::with_annotations(...)` 的注册方式，不引入新的工具注册抽象。
2. `vmcp_scratchpad_*` 与 `runlua` 作为保留内建工具，在 `register_defaults()` 内显式注册。
3. 不删除 `handle_tools_call()` 里现存的其他内建分支，先只通过“是否注册”控制默认暴露面，降低本轮改动风险。

# 验收标准

1. `tools/list` 默认可见 `vmcp_scratchpad_*` 与 `runlua`。
2. `tools/list` 默认不可见 `add`、`greet`、`current_time`、`lancedb_*`、`sqlite_*`。
3. 服务端初始化说明文案与实际默认暴露能力一致。
4. `cargo check` 通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次调整把 MCP 默认内建工具的暴露面从“全部关闭”改成“仅保留有明确产品价值的两类能力”：`vmcp_scratchpad_*` 与 `runlua`。其余历史内建工具继续不注册、不默认暴露，从而满足“对外协议面收缩，但保留特色能力”的目标。

同时，这次顺手把保留工具的 JSON Schema 做了补齐，避免数组参数继续沿用缺少 `items` 的旧写法；并把初始化说明文案修正为真实状态，防止客户端继续被“默认仅 Lua skills”误导。

## 2. 📂 文件变更清单

- 新增：`docs/plan/20260416-06-BUILTIN_TOOL_EXPOSURE_ADJUSTMENT.md`
- 修改：`src/server.rs`

## 3. 💻 关键代码调整详情

- 在 `McpServer::register_defaults()` 中恢复注册以下默认内建工具：
  - `vmcp_scratchpad_upsert`
  - `vmcp_scratchpad_delete`
  - `vmcp_scratchpad_get`
  - `vmcp_scratchpad_list_keys`
  - `vmcp_scratchpad_clean`
  - `runlua`
- 保持 `add`、`greet`、`current_time`、`lancedb_*`、`sqlite_*` 不注册，因此它们不会再出现在默认的 `tools/list` 中。
- 为 scratchpad 相关 schema 的数组参数补充了 `items` 定义，避免继续暴露不完整的数组 schema。
- 修正初始化阶段的能力日志和 `instructions` 文案，使其与当前真实默认暴露面一致：Lua skills + `vmcp_scratchpad_*` + `runlua`。
- 保留 `handle_tools_call()` 中现存的其他历史分支不动，本轮仅通过“是否注册”来控制默认暴露面，以降低改动风险。

## 4. ⚠️ 遗留问题与注意事项

- 目前 `src/server.rs` 中仍保留其他历史内建工具的调用分支实现，只是默认不注册；如果后续确认这些能力完全不再需要，可以再做一次代码清理。
- `runlua` 依赖 Lua 引擎可用；如果服务未配置 Lua skills 目录，工具会在调用时返回“Lua engine not configured”错误，这属于当前设计预期。
- 本次验证仅执行了 `cargo check`，未额外做端到端 MCP 调用回归。
