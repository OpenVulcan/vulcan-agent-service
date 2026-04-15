# 任务目标

将 `CLAUDE.md` 中高价值的行为原则与当前 `Vulcan CodeKit` 提示词体系进行融合，并同步更新本地 `vulcan-codekite` skill 与公共 `vulcan_codekite_skill` prompt。要求两者主体正文保持一致，公共 prompt 仅在末尾额外追加动态任务段落。

# 执行步骤

1. 以当前本地 `vulcan-codekite` skill 为正文基线，识别适合吸收的 `CLAUDE.md` 行为原则。
2. 将行为原则精炼为简短、可执行的英文段落，并放在 CodeKit 工具决策与工作流之前。
3. 同步更新本地 skill 与公共 prompt 主体，保持正文一致。
4. 保留公共 prompt 的动态 `task` 注入能力，使其仅作为末尾附加段落。
5. 校验本地 skill 与 `skill.json` 配置，补写执行总结并归档。

# 技术选型

- 本地 skill 与公共 prompt 继续以英文保存，便于跨客户端复用。
- 行为原则采用“Think before coding / Simplicity first / Keep changes surgical / Verify against a concrete outcome”的轻量结构。
- 公共 prompt 继续使用 `.lua` 生成器，以保留动态任务注入。

# 验收标准

- 本地 `vulcan-codekite` skill 已吸收精炼后的行为原则。
- 公共 prompt 主体与本地 skill 正文一致。
- 公共 prompt 相比本地 skill 仅多出末尾动态 `Current User Instruction` 段落。
- `skill.json` 解析通过。
- 本地 `vulcan-codekite` 继续通过 `quick_validate.py`。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `CLAUDE.md` 中适合当前场景的行为原则，以轻量形式融合进 `Vulcan CodeKit` 提示词体系。
- 这次没有替换掉现有的 CodeKit 决策树和工作流，而是在它们之前新增一层“行为原则”，补强思考、简化、外科手术式改动和验证闭环。
- 本地 `vulcan-codekite` skill 与公共 prompt 主体继续保持一致，公共 prompt 仍只在末尾多出动态 `Current User Instruction` 段落。

### 2. 📂文件变更清单

- 修改：`C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`

### 3. 💻关键代码调整详情

- 在本地 skill 正文最前部加入四组英文行为原则：
  - `Think before coding`
  - `Simplicity first`
  - `Keep changes surgical`
  - `Verify against a concrete outcome`
- 将相同内容同步插入公共 prompt 的 `base_prompt` 主体中，位置保持一致，确保两边的主体文本继续完全对齐。
- 保留现有的 `Quick Decision Tree`、`Main-Agent Rule`、`Tool Notes`、`Typical Workflows`、`Failure and Fallback` 与 `Boundaries` 结构，避免因全盘重写破坏已有工具使用心智。

### 4. ⚠️遗留问题与注意事项

- `skill.json` 解析通过，本地 `vulcan-codekite` 继续通过 `quick_validate.py`。
- 通过正文匹配检查确认公共 prompt 主体仍与本地 skill 正文一致。
- 后续如果还要继续演进提示词，建议仍以本地 skill 为基线更新，再同步公共 prompt 的主体，避免两边再次漂移。
