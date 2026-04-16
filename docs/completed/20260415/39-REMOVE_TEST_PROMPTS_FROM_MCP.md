# 任务目标

从当前 MCP 服务默认注册项中移除无实际业务价值的测试 Prompt，避免客户端继续看到 `code_review` 与 `explain_code` 这两个演示型入口。

# 执行步骤

1. 检查 `server.rs` 中默认 Prompt 的注册与读取逻辑，确认测试 Prompt 的注册点和处理分支。
2. 移除 `code_review` 与 `explain_code` 的默认注册及对应处理逻辑，保留其它 Prompt/Skill 机制不变。
3. 进行最小必要校验，确认编译通过且相关默认 Prompt 不再暴露。
4. 追加执行变更总结并归档计划文件。

# 技术选型

- 仅删除默认测试 Prompt，不改动 Lua skill prompt 体系。
- 保持 MCP Prompt 机制仍可供真正有用的 prompt 使用，不做额外架构调整。

# 验收标准

- `code_review` 与 `explain_code` 不再出现在默认 MCP prompts 列表中。
- 对应服务端处理分支已移除。
- `cargo check` 通过。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 从 MCP 服务默认注册项中移除了 `code_review` 与 `explain_code` 这两个演示型 Prompt。
- 同时删除了 `prompts/get` 中对应的硬编码处理分支，让 Prompt 解析只保留真正有用的 Lua skill prompt 通路。
- 这样客户端后续看到的 Prompt 列表将不再混入这两个无业务价值的测试项。

### 2. 📂文件变更清单

- 修改：`src/server.rs`

### 3. 💻关键代码调整详情

- 删除 `register_defaults()` 中对 `code_review` 和 `explain_code` 的默认 `Prompt` 注册。
- 删除 `handle_prompts_get()` 中针对这两个 Prompt 的专门 `match` 分支。
- 保留 Lua skill prompt 的获取逻辑不变，未改动 `vulcan-codekite` 等真实 Prompt 的加载机制。

### 4. ⚠️遗留问题与注意事项

- 已执行 `cargo check`，当前编译通过。
- 额外用源码搜索确认 `src/server.rs` 中已不再存在 `code_review` / `explain_code` 文本。
- 本次只删除默认测试 Prompt，没有调整 MCP Prompt 机制本身。
