# 任务目标

分析当前提出的 Codekit 公共提示词草案，判断其优缺点，并据此同时优化两份规则载体：
1. 本地 Codex skill：`C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
2. Lua skill 公共 prompt：`runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`

目标是在降低认知负担的同时，提高模型在未显式指定工具时的正确选用率、工作流稳定性与边界判断能力。

# 执行步骤

1. 审阅当前 `vulcan-codekite` skill 与 `vulcan_codekite_skill.md` 的现状，结合用户给出的新草案分析优劣。
2. 提炼更优结构，优先采用“按当前状态决策”的快速决策树，再保留精简后的工具说明、典型工作流、异常回退与边界说明。
3. 同步改写本地 Codex skill，使其更适合 Codex 自动触发与长期复用。
4. 同步改写 Lua skill 公共 prompt，使其更适合支持 MCP prompts 的客户端直接消费。
5. 做必要的结构校验，确认文件内容落地无误；若涉及 `skill.json` 则确认其仍可正常解析。
6. 追加执行变更总结并将计划文件归档。

# 技术选型

- 以“用户当前状态 -> 选择工具”的决策树作为主结构，减少按工具堆规则带来的认知负担。
- 保留少量高价值工作流示例与失败回退策略，但避免变成长篇说明书。
- 对 Codex 本地 skill 与 Lua 公共 prompt 采取同一思想、不同粒度：
  - Codex skill 更完整，适合自动触发与长期复用。
  - Lua 公共 prompt 更短、更通用，适合跨客户端消费。

# 验收标准

- 已完成对新草案的优缺点分析。
- `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md` 已按新结构优化。
- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md` 已同步优化。
- 若改动 `skill.json`，则 JSON 解析通过；若未改动，也需说明原因。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 结合用户提出的新版草案，对现有 CodeKit 提示词做了结构性重构，核心思路从“按工具说明限制”调整为“按当前状态决策”。
- 同步优化了两份载体：
  - `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
  - `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`
- 新版本统一采用“快速决策树 + 精简工具摘要 + 典型工作流 + 失败回退 + 边界说明”的组织方式，降低模型认知负担。

## 2. 📂文件变更清单

### 修改文件

- `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`
- `docs/plan/20260415-32-CODEKIT_SKILL_PROMPT_DECISION_TREE_OPTIMIZATION.md`（后续已归档）

### 新增文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

- 本地 `vulcan-codekite` skill
  - 优化了 frontmatter description，使其更容易按“未知文件、已知文件、已知线索、Markdown 导航、整函数替换”这些状态触发。
  - 主体结构改为：
    - Quick Decision Tree
    - Tool Notes
    - Typical Workflows
    - Failure and Fallback
    - Boundaries
  - 明确补充了“什么时候不用 CodeKit，而该回退到更轻工具”的边界。
- Lua 公共 prompt
  - 改为更短的公共决策树版本，适合支持 MCP prompts 的客户端直接消费。
  - 保留高价值部分：
    - 当前状态 -> 选工具
    - 常见工作流
    - 失败回退
  - 删除了不必要的长篇工具说明。

## 4. ⚠️遗留问题与注意事项

- 本轮没有改动 `skill.json`，因为现有 prompt 注册结构已经足够，问题主要在提示词正文组织方式而非配置本身。
- 已运行 `quick_validate.py` 校验本地 `vulcan-codekite` skill，结果通过。
- 如果后续还要进一步压缩 token，可继续缩减 `Tool Notes` 段落，或者把失败回退再压成更短的表格式提示。
