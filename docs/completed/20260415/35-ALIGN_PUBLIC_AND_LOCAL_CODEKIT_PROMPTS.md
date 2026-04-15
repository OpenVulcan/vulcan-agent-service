# 任务目标

将公共 `vulcan_codekite_skill` prompt 与本地 `vulcan-codekite` skill 的正文内容完全对齐，确保两者唯一差异只剩公共 prompt 末尾追加的动态 `task` 指令段落。

# 执行步骤

1. 审阅当前公共 prompt 生成器与本地 `vulcan-codekite` skill，确认正文差异点。
2. 以本地 skill 的正文为基准，调整公共 prompt，使其正文与本地 skill 保持一致。
3. 保留公共 prompt 的动态 `task` 注入能力，并确保它仅作为末尾附加段落存在。
4. 进行 `skill.json` 解析与本地 skill 校验，确认未引入配置问题。
5. 追加执行变更总结并归档计划文件。

# 技术选型

- 公共 prompt 继续使用 `.lua` 生成器，以保留 `task` 参数注入能力。
- 正文内容以本地 `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md` 为对齐基准。
- 公共 prompt 与本地 skill 的共享正文保持英文，便于跨客户端复用。

# 验收标准

- 公共 prompt 与本地 skill 正文一致。
- 公共 prompt 相比本地 skill 仅多出末尾动态 `Current User Instruction` 段落。
- `skill.json` 解析通过。
- 本地 `vulcan-codekite` skill 继续通过 `quick_validate.py`。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 将公共 `vulcan_codekite_skill` prompt 的主体内容改为与本地 `vulcan-codekite` skill 完全一致。
- 保留公共 prompt 的动态 `task` 注入能力，并将其限制为末尾唯一附加差异段落 `Current User Instruction`。
- 通过正文匹配检查确认公共 prompt 中嵌入的主体文本与本地 skill 正文一致，避免后续两边语义漂移。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`

### 3. 💻关键代码调整详情

- 将原本以 `lines` 数组逐段维护的公共 prompt 内容，替换为一份完整的 `base_prompt` 英文正文。
- 该 `base_prompt` 的内容与本地 `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md` 去除 frontmatter 后的正文保持一致。
- 返回值结构调整为：`base_prompt` + 空行 + `## Current User Instruction` + 动态 `task` 内容，从而保证公共 prompt 与本地 skill 的差异只出现在最后的动态指令区。

### 4. ⚠️遗留问题与注意事项

- `skill.json` 解析已通过，本地 `vulcan-codekite` 继续通过 `quick_validate.py`。
- 这次未修改本地 skill 正文本身，而是让公共 prompt 向本地 skill 对齐；如果后续要继续演进两者内容，建议优先修改本地 skill，再同步更新公共 prompt 的 `base_prompt`。
