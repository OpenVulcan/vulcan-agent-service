# 任务目标

调整 `vulcan_codekite_skill` 公共 prompt 的 `task` 参数行为：当 `task` 未传递或仅为空白时，只返回固定提示词主体；只有在 `task` 明确有内容时，才在末尾追加 `Current User Instruction` 段落。

# 执行步骤

1. 检查当前 `vulcan_codekite_skill.lua` 的 `task` 处理逻辑，确认默认兜底文本与始终追加段落的实现位置。
2. 修改逻辑为：`task` 为空时直接返回基础提示词；`task` 有内容时才拼接末尾动态指令段落。
3. 做最小范围校验，确认配置未受影响且返回逻辑符合预期。
4. 追加执行变更总结并归档计划文件。

# 技术选型

- 仅调整公共 prompt 生成器，不扩大到本地 skill 正文。
- 保持 `skill.json` 现有参数定义不变，只修正运行时行为。

# 验收标准

- `task` 未传递时，只返回固定提示词主体。
- `task` 有内容时，才追加 `Current User Instruction` 段落。
- `skill.json` 解析通过。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vulcan_codekite_skill` 的 `task` 行为改为真正“可选”。
- 现在当 `task` 未传递或仅为空白时，prompt 直接返回固定主体，不再自动拼接默认任务文案。
- 只有当 `task` 明确有内容时，才会在末尾附加 `Current User Instruction` 段落。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`

### 3. 💻关键代码调整详情

- 删除了原先在 `task` 为空时自动注入默认任务文本的逻辑。
- 保留 `base_prompt` 作为固定主体，并增加条件分支：
  - `task == ""` 时直接返回 `base_prompt`
  - `task` 有内容时再拼接动态指令区

### 4. ⚠️遗留问题与注意事项

- 本次只调整了运行时返回逻辑，没有修改 `skill.json` 中 `task` 参数的声明。
- 因此部分客户端界面仍可能把该 prompt 展示成“带参数”的形式；但在运行时，空参数已不会再触发默认用户指令注入。
- `skill.json` 解析校验已通过。
