# 任务目标

优化当前 `vulcan_codekite_skill` 提示词内容，不做全盘推翻式重写，而是结合旧版中仍有价值的结构与新补充的“主代理先建图、再决定策略/子代理”的最佳实践，重新收敛为一版更平衡、更易执行的英文提示词。同时视需要同步微调本地 `vulcan-codekite` skill，保持两边规则一致。

# 执行步骤

1. 审阅当前公共 prompt 生成器与本地 `vulcan-codekite` skill，识别哪些内容来自旧版且仍有价值，哪些内容来自新版且应保留。
2. 以“保留高价值旧结构 + 吸收新最佳实践 + 全英文输出”为原则，重新优化 prompt 内容，而不是完全重写思路。
3. 视一致性需要，微调本地 `vulcan-codekite` skill 的正文，使其与公共 prompt 的核心规则保持一致。
4. 校验 `skill.json` 解析、公共 prompt 文件落地，以及本地 `vulcan-codekite` skill 继续通过 `quick_validate.py`。
5. 追加执行变更总结并将计划文件归档。

# 技术选型

- 公共 prompt 继续使用 `.lua` 生成器，以保留 `task` 参数注入能力。
- 文本内容改为全英文输出，便于跨客户端复用。
- 结构上采用折中方案：保留决策树/工作流/回退边界，同时吸收“主代理先建立全局观再决定子代理”的核心原则。

# 验收标准

- 公共 prompt 已改为英文。
- 内容不是全盘推翻，而是体现对旧版与新版规则的融合优化。
- 若本地 `vulcan-codekite` skill 被同步调整，则仍能通过 `quick_validate.py`。
- `skill.json` 解析通过。
- 计划文件完成执行总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vulcan_codekite_skill` 公共 prompt 从中文内容收敛为英文内容，并保留动态 `task` 注入能力。
- 本次没有全盘推翻原结构，而是在保留“快速决策树、工具顺序、典型工作流、失败回退、边界”这些高价值旧结构的基础上，吸收了“主代理先建图再决定策略/子代理”的新规则。
- 同步微调本地 `vulcan-codekite` skill，使 Codex 本地 skill 与公共 prompt 的核心判断逻辑保持一致，但不重复做大幅改写。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`
- 修改：`C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`

### 3. 💻关键代码调整详情

- 将公共 prompt 生成器中的默认 `task` 文案改为英文，并统一将末尾上下文段落改为 `Current User Instruction`。
- 将公共 prompt 的正文改为英文决策式结构，保留旧版中有执行价值的“Quick Decision Tree / Tool Order / Typical Workflows / Failure and Fallback / When Not to Use CodeKit”，同时融合新版中强调的“主代理必须先通过 `codekit-ast-tree` 建立全局地图”的原则。
- 对本地 `vulcan-codekite` skill 做小幅对齐，补强 `codekit-ast-tree` 的“读完后应能解释模块职责”这一点，并让“Unknown codebase”工作流与公共 prompt 的表述更一致。

### 4. ⚠️遗留问题与注意事项

- `skill.json` 解析校验已通过，本地 `vulcan-codekite` 也已通过 `quick_validate.py`。
- 由于当前环境没有独立 `lua` 命令行解释器，未直接用系统 Lua 单独执行该 prompt 生成器；本次以文件落地、JSON 解析及本地 skill 校验作为验证闭环。
