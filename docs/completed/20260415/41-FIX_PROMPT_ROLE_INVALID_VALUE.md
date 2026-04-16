# 任务目标

修复 `vulcan_codekite_skill` Prompt 返回 `system` 角色导致客户端校验失败的问题，确保 Prompt 消息角色与当前客户端实际可接受的取值保持兼容。

# 执行步骤

1. 检查当前 Prompt 角色生成逻辑与客户端报错信息，确认允许的角色范围。
2. 调整 `vulcan_codekite_skill` 的返回结构与元数据，使其不再输出不被接受的 `system` 角色。
3. 做最小范围校验，确认配置解析正常且编译通过。
4. 追加执行变更总结并归档计划文件。

# 技术选型

- 以客户端实际接受的角色集合为准，不再继续输出 `system`。
- 保持 Prompt 结构尽量稳定，只修复角色兼容性问题。

# 验收标准

- `vulcan_codekite_skill` 不再输出 `system` 角色。
- 不再触发 `messages[0].role` 的 `invalid_value` 错误。
- `skill.json` 解析通过。
- `cargo check` 通过。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 根据客户端返回的错误，确认当前客户端只接受 `user` 与 `assistant` 两类 Prompt 消息角色，不接受 `system`。
- 将 `vulcan_codekite_skill` 从此前的 `system + user` 组合回退到完全兼容的 `user` 角色输出。
- 保留此前已经接好的 `task` completion 能力，不扩大回退范围。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`

### 3. 💻关键代码调整详情

- 将 `skill.json` 中 `vulcan_codekite_skill.role` 从 `system` 改回 `user`。
- 同步调整 `task` 参数描述，避免继续暗示会以 `system` 身份承载共享提示词。
- 在 `vulcan_codekite_skill.lua` 中，将基础提示词主体消息的 `role` 从 `system` 改回 `user`，保持后续可选 `task` 仍以 `user` 追加。

### 4. ⚠️遗留问题与注意事项

- 当前宿主实现虽然允许 Prompt 消息透传任意字符串角色，但具体客户端会自行做更严格的枚举校验。
- 因此在实际对外行为上，应以客户端当前明确接受的角色集合为准。
- 已执行 `skill.json` 解析校验与 `cargo check`，当前均通过。
