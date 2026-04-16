# 任务目标

为 `vulcan_codekite_skill` 的 `task` 参数实现常规快捷候选项补全，并评估及处理当前 Prompt 消息角色是否支持以 `system` 或 `user` 身份输出的问题，保证该 Prompt 在支持 completions 的客户端中更易用。

# 执行步骤

1. 检查当前 Prompt 参数、completion 处理逻辑与 Prompt 消息角色生成路径，确认扩展点。
2. 为 `vulcan_codekite_skill.task` 增加一组常规候选项补全，覆盖常见分析/定位/替换场景。
3. 检查当前 Prompt role 是否允许 `system`，并根据实际协议与宿主实现决定是否支持或限制。
4. 进行最小必要校验，确认编译通过且行为符合预期。
5. 追加执行变更总结并归档计划文件。

# 技术选型

- 优先复用现有 `completion/complete` 能力，不额外设计新的 prompt 参数协议。
- 对 Prompt role 的支持以当前 MCP 协议结构和本仓库实现为准，避免做无效扩展。
- 常规候选项保持少而精，优先覆盖高频任务场景。

# 验收标准

- `vulcan_codekite_skill.task` 拥有可用的快捷候选项补全。
- 明确当前 Prompt 消息角色是否支持 `system` 与 `user`，并在实现中正确处理。
- `cargo check` 通过。
- 计划文件完成执行变更总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 为 `vulcan_codekite_skill.task` 接入了一组常见任务候选项补全，方便支持 completions 的客户端直接选择高频分析与修改意图。
- 将公共 Prompt 从单条文本返回改为显式 `messages` 结构，使共享规则以 `system` 身份输出，而可选的 `task` 以 `user` 身份追加。
- 同时确认当前宿主实现本身已经支持 `system` / `user` 角色透传，不需要再额外扩展协议模型。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`
- 修改：`src/server.rs`

### 3. 💻关键代码调整详情

- 在 `skill.json` 中将 `vulcan_codekite_skill` 的默认 `role` 调整为 `system`，并更新 `task` 参数描述，说明其为可选任务意图并支持候选项补全。
- 在 `vulcan_codekite_skill.lua` 中将返回值改为 `messages`：
  - 第一条固定为 `system` 消息，承载共享 CodeKit 规则
  - 当 `task` 有内容时，第二条为 `user` 消息，承载当前具体指令
- 在 `server.rs` 的 `handle_completion()` 中为 `ref.name == "vulcan_codekite_skill"` 且参数名为 `task` 的场景补充了常用候选项，包括：
  - 全盘分析项目结构
  - 根据信号定位函数/类归属
  - 精确文件 AST 细查
  - Markdown 文档章节定位
  - 安全整函数/整方法替换准备
  - 建图后再委派子代理

### 4. ⚠️遗留问题与注意事项

- 当前实现已经支持 `system` / `user` 角色透传，但客户端是否严格按这两个角色的语义消费，仍取决于具体客户端实现。
- 这次只给 `vulcan_codekite_skill.task` 增加了 completions，没有扩展通用 Prompt 参数枚举协议。
- 已执行 `skill.json` 解析校验、`cargo check` 与本地 `quick_validate.py`，当前均通过。
