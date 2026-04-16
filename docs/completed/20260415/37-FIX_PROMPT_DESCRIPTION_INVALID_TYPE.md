# 任务目标

排查并修复当前 `vulcan-codekit` 提示词相关配置触发的 `description` 字段类型校验错误，确保运行时或客户端不再报出 `expected: "string"` 的无效类型异常。

# 执行步骤

1. 检查当前 `vulcan-codekit` 的 `skill.json`、prompt 定义及相关 Rust/Lua 解析路径，确认是哪一层把 `description` 解析成了非字符串。
2. 根据实际协议要求修复配置或宿主转换逻辑，确保 `description` 始终以字符串形式输出。
3. 做最小范围校验，验证错误消失且相关配置仍可正常解析。
4. 追加执行变更总结并归档计划文件。

# 技术选型

- 优先修复配置层问题，只有在确认配置无误时再调整宿主转换逻辑。
- 保持变更最小化，不扩大到与本次错误无关的提示词结构调整。

# 验收标准

- 能定位 `description invalid_type` 的真实来源。
- 修复后相关配置校验通过，不再出现该错误。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 定位到错误并非来自 `skill.json` 配置内容，而是来自 MCP `prompts/get` 结果的序列化行为。
- `PromptGetResult.description` 在为空时原本会被序列化成 `null`，部分客户端 schema 只接受“字符串”或“字段缺失”，因此触发了 `invalid_type`。
- 已将该字段改为在 `None` 时跳过序列化，从根源上避免 `description: null` 输出。

### 2. 📂文件变更清单

- 修改：`src/protocol.rs`

### 3. 💻关键代码调整详情

- 在 `PromptGetResult.description` 上增加 `#[serde(skip_serializing_if = "Option::is_none")]`。
- 这样当 Lua prompt 生成器直接返回纯字符串、最终形成 `PromptGetResult { description: None, ... }` 时，返回 JSON 将省略 `description` 字段，而不是输出 `null`。

### 4. ⚠️遗留问题与注意事项

- 本次做的是最小修复，没有扩大到与当前报错无关的字段清理。
- 已执行 `cargo check`，当前编译通过。
- 额外用于排查的临时文件已删除，没有保留在工作区。
