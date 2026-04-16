# 任务目标

将当前写死在服务端中的 Prompt 参数候选项补全改造成 `skill.json` 配置驱动，并保证 skill 在脱离服务端特判代码后仍能独立运行。

# 执行步骤

1. 扩展 `skill.json` / `lua_skill.rs` 的 Prompt 参数元数据结构，支持声明候选项补全。
2. 修改服务端 completion 逻辑，从已加载 skill 元数据中动态读取 Prompt 参数候选项，不再写死 `vulcan_codekite_skill` 的特判。
3. 在 `vulcan-codekit` 的 `skill.json` 中为 `task` 参数补充常规候选项配置。
4. 进行最小必要校验，确认编译通过、配置解析正常，且本地 skill 仍可用。
5. 追加执行变更总结并归档计划文件。

# 技术选型

- 采用配置驱动方案，让 Prompt completion 能作为 skill 自身能力存在。
- 优先扩展通用 Prompt 参数元数据，而不是继续在 `server.rs` 中增加更多特判。
- 维持现有客户端兼容行为，不在本次引入额外协议层变更。

# 验收标准

- Prompt 参数候选项补全不再写死在服务端逻辑中。
- `vulcan-codekit` 的 `task` completion 改为来自 `skill.json` 配置。
- `cargo check` 通过。
- 本地 `vulcan-codekite` skill 继续通过 `quick_validate.py`。
- 计划文件完成执行变更总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vulcan_codekite_skill.task` 的候选项补全从服务端硬编码改造成 `skill.json` 配置驱动。
- 现在补全候选项成为 skill 自身的一部分，服务端只负责通用读取与过滤，不再认识某个特定 prompt 名称。
- 这样 `vulcan-codekit` 作为一个独立 skill 就能自带 prompt completion 能力，后续增删候选项只需改配置。

### 2. 📂文件变更清单

- 修改：`src/lua_skill.rs`
- 修改：`src/lua_engine.rs`
- 修改：`src/server.rs`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`

### 3. 💻关键代码调整详情

- 在 `SkillPromptArgumentMeta` 中新增 `completions: Vec<String>` 字段，用于承载 Prompt 参数候选项配置。
- 在 `LuaEngine` 中新增 `prompt_argument_completions(prompt_name, argument_name)`，统一从已加载 skill 元数据中读取某个 Prompt 参数的候选项。
- 在 `server.rs` 的 `handle_completion()` 中移除对 `vulcan_codekite_skill.task` 的写死特判，改为：
  - 对 `ref/prompt` 请求优先尝试读取 skill 元数据中的参数候选项
  - 若存在配置，则按用户输入做大小写无关过滤后返回
- 在 `runtime/lua_skills/vulcan-codekit/skill.json` 中为 `task` 参数补上常规候选项列表，覆盖全盘分析、信号定位、精确文件检查、Markdown 定位、整函数替换准备、建图后委派子代理等高频场景。

### 4. ⚠️遗留问题与注意事项

- 目前 prompt 参数候选项是 skill 层自定义能力，不是标准 MCP prompt 参数 schema 的通用字段；但在当前宿主实现中已经生效。
- 这次保留了 `("ref/prompt", "language")` 这个旧 completion 分支，它属于历史演示逻辑，与本次配置化改造并存。
- 已执行 `skill.json` 解析校验、`cargo check` 与本地 `quick_validate.py`，当前均通过。
