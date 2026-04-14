## 任务目标

将 Codex 源代码下载到 `D:\projects\codex` 目录，并分析其 MCP 工具调用与返回内容处理逻辑，重点确认：

1. MCP 工具返回内容是如何注入模型上下文的；
2. 长返回内容的截断/裁剪逻辑位于哪里；
3. 是否存在可配置参数、环境变量或启动选项可以解除或调高该限制；
4. 若没有显式配置项，是否能从源码上判断限制属于固定常量、模型上下文保护、UI 渲染层裁剪还是其他链路。

## 执行步骤

1. 确认 `D:\projects\codex` 是否已存在，若不存在则拉取官方仓库源码。
2. 快速梳理仓库结构，定位与 MCP、tool calling、context packing、response truncation、token budgeting 相关的目录与入口文件。
3. 重点搜索并阅读以下关键实现：
   - MCP transport / tool invocation
   - tool result serialization / normalization
   - 长文本截断、token 限流、context budget 相关逻辑
   - 与 UI 或客户端显示层相关的二次裁剪逻辑
4. 根据源码证据判断：
   - 截断发生在哪一层
   - 是否有可调参数
   - 如有参数，给出具体配置方式
   - 如无参数，给出准确结论与证据
5. 整理分析结果并输出建议；任务完成后补充执行变更总结并归档。

## 技术选型

- 优先使用本地源码分析，不依赖二手资料。
- 搜索工具优先使用 `rg`，按模块逐层收敛，避免盲目全仓库展开。
- 判断“是否可调”时，以实际代码路径、配置定义、CLI 参数定义和环境变量读取点为准。

## 验收标准

1. `D:\projects\codex` 成功准备好可分析源码。
2. 明确定位到 MCP 工具结果处理主链路。
3. 明确指出截断逻辑所在层级与关键文件。
4. 明确回答“是否有可配置方式解除或调整限制”。
5. 给出带源码证据的中文结论，并完成计划归档。

---

## 执行变更总结

### 1. 核心修复与调整概述

本次任务未修改 `vulcan-mcp-client` 业务代码，主要完成了对 `D:\projects\codex` 源码的拉取与定向分析，并基于源码确认了以下结论：

1. MCP 工具调用主链路位于 `codex-rs/codex-mcp/src/mcp_connection_manager.rs`，该层负责把工具调用委托给 RMCP 客户端，本身不做长度截断。
2. 长输出截断发生在 `codex-rs/core/src/context_manager/history.rs` 的历史入库阶段，也就是“工具结果写入模型上下文”之前。
3. Codex 存在可调参数 `tool_output_token_limit`，该参数定义在配置结构与 JSON Schema 中，语义是“写入上下文管理器时，工具/函数输出允许占用的 token 预算”。
4. `tool_output_token_limit` 会覆盖模型默认的 `truncation_policy`，因此它不仅影响 shell/unified exec，也影响 MCP 工具输出。
5. 源码测试明确覆盖了“MCP 输出默认会被截断”和“配置更大预算后 MCP 输出不再被截断”两类场景，因此可以确认这是产品设计而非偶发 bug。

### 2. 📂文件变更清单

新增：
- `D:\projects\codex`（拉取的 Codex 源码仓库，用于本次分析）

修改：
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-01-CODEX_MCP_TRUNCATION_SOURCE_ANALYSIS.md`

删除：
- 无

### 3. 💻关键代码调整详情

本次未对业务代码进行修改，重点沉淀了源码结论与关键证据：

1. MCP 调用层不截断  
   `codex-rs/codex-mcp/src/mcp_connection_manager.rs` 中，`call_tool()` 直接执行：
   - `client.client.call_tool(...)`
   - 然后把 `result.content` 转成 `CallToolResult`
   这一层没有长度裁剪逻辑。

2. 截断发生在上下文历史层  
   `codex-rs/core/src/context_manager/history.rs` 中：
   - `process_item()` 对 `ResponseItem::FunctionCallOutput` 和 `ResponseItem::CustomToolCallOutput` 调用 `truncate_function_output_payload(...)`
   - `truncate_function_output_payload(...)` 再调用 `truncate_text(...)` 或 `truncate_function_output_items_with_policy(...)`
   说明截断发生在“写入上下文历史”时，而不是 MCP 传输层。

3. 可调参数来源  
   `codex-rs/config/src/config_toml.rs` 与 `codex-rs/core/config.schema.json` 中定义了：
   - `tool_output_token_limit`
   描述为：
   - `Token budget applied when storing tool/function outputs in the context manager.`

4. 参数覆盖默认策略  
   `codex-rs/models-manager/src/model_info.rs` 中：
   - 如果配置了 `tool_output_token_limit`
   - 就会覆盖 `model.truncation_policy`
   - 字节型模型会把 token 预算换算成 byte budget
   - token 型模型则直接替换为 `TruncationPolicyConfig::tokens(limit)`

5. MCP 专项测试证据  
   `codex-rs/core/tests/suite/truncation.rs` 中同时存在：
   - `mcp_tool_call_output_exceeds_limit_truncated_for_model`
   - `mcp_tool_call_output_not_truncated_with_custom_limit`
   前者证明 MCP 长输出会被截断，后者证明把 `tool_output_token_limit` 调大后，MCP 输出可完整保留。

### 4. ⚠️遗留问题与注意事项

1. `tool_output_token_limit` 是“调高预算”的入口，不等于绝对无限制；设置极大值后，仍可能受更上层模型上下文窗口与客户端展示策略影响。
2. 不同模型默认 `truncation_policy` 不完全一致，默认阈值依赖模型元数据；本地源码里至少能确认有 `10_000 bytes` 的默认策略示例。
3. 如果用户想逼近当前桌面端真实阈值，最可靠的方法仍然是结合当前模型，通过逐步调高 `tool_output_token_limit` 做二分测试，而不是只凭单次截断现象反推。
