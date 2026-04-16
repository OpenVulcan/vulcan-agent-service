# 任务目标

将当前 MCP 服务默认暴露的非 Lua skill 功能全部关闭，只保留由 Lua skills 提供的 MCP tools/resources/resource templates/prompts 能力，为后续按需逐项恢复和调整打基础。同时明确 LanceDB 相关能力不再作为默认 MCP 暴露项，而仅作为内部能力保留。

# 执行步骤

1. 检查当前 MCP 默认注册与对外暴露路径，确认哪些能力属于非 Lua skill 默认项。
2. 调整默认注册逻辑，停止暴露非 Lua skill 的 tools/resources/resource templates/prompts 等入口。
3. 处理与默认暴露项相关的 completion/初始化说明等行为，避免继续暴露无效能力。
4. 进行最小必要校验，确认编译通过且服务端行为与“仅暴露 Lua skills”一致。
5. 追加执行变更总结并归档计划文件。

# 技术选型

- 优先从“默认注册层”关闭非 Lua skill 能力，而不是逐个删除底层实现。
- 保留底层实现代码供后续逐项恢复，但不在当前对外默认暴露。
- 以 Lua skill 元数据作为唯一默认 MCP 能力来源。

# 验收标准

- 默认 MCP 列表中不再暴露非 Lua skill 的 tools/resources/resource templates/prompts。
- LanceDB 等内部能力不再作为默认 MCP 工具暴露。
- `cargo check` 通过。
- 计划文件完成执行变更总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 收缩 `McpServer` 的默认暴露面，取消内建默认注册逻辑，默认只保留由 Lua skills 注入的 MCP tools/resources/resource templates/prompts/completions。
- 移除内建资源读取、内建 prompt/demo prompt、语言补全、roots/sampling/elicitation/logging 等非 Lua skill 默认协议入口，避免“列表里不显示但仍可直接调用”的半关闭状态。
- 调整 `initialize` 返回能力与说明文本，使其与当前“仅暴露 Lua skills” 的行为保持一致。

## 2. 📂文件变更清单

### 修改

- `src/server.rs`

## 3. 💻关键代码调整详情

- 将 `register_defaults()` 收敛为空实现，不再默认注册 `add/greet/current_time`、LanceDB、SQLite、Scratchpad、`runlua`、内建资源、模板与测试 prompts。
- 调整 `handle_initialize()`：
  - 根据 Lua skills 实际注入结果动态计算 `tools/resources/prompts/completions` 能力。
  - 关闭默认 `logging` 能力。
  - 更新初始化说明文本，明确默认仅暴露 Lua skill 提供的能力。
- 调整 `handle_resources_read()`，移除内建 `resource_data` 与 `echo://` 读取分支，仅保留 Lua skill 资源读取路径。
- 调整 `handle_request()`，去掉 `roots/list`、`sampling/createMessage`、`elicitation/create`、`logging/setLevel` 默认入口。
- 调整 `handle_completion()`，只保留 Lua skill prompt 参数补全路径，不再提供内建语言候选和默认资源候选。
- 清理已失效的 `resource_data`、`roots`、`log_level` 字段，以及对应的未使用处理函数。

## 4. ⚠️遗留问题与注意事项

- 本次只关闭了默认对外暴露，不删除底层数据库/内部执行实现，后续仍可按需逐项恢复。
- 工作区中还存在与本任务无关的既有修改与未跟踪文件，本次未处理。
- 已执行 `cargo check`，当前编译通过。
