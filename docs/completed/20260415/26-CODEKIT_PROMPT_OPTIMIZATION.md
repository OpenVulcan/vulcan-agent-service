# 任务目标

优化 `runtime/lua_skills/vulcan-codekit/skill.json` 中五个核心工具的 `description` 与 `prompt`，在降低提示词体积的同时，提高模型在未显式指定工具时的主动选用能力与参数遵循正确率。

# 执行步骤

1. 审阅当前五个工具的 `description` 与 `prompt`，识别影响模型自动选用和正确调用的冗余描述与关键缺失点。
2. 以“更容易触发正确工具选择 + 更容易约束参数使用”为目标，统一收敛为短规则风格的提示词。
3. 对高频误用点保留必要硬约束，例如 `codekit-ast-tree` 的单目录限制、`codekit-ast-detail` 的显式文件限制、`codekit-patch` 的完整函数替换要求。
4. 同步优化 `description`，让工具在 `tools/list` 视图里更容易被模型主动选择，而不仅仅依赖详细 prompt。
5. 校验 `skill.json` JSON 结构合法，确认没有破坏加载格式。
6. 追加执行变更总结，并将计划文件归档到 `docs/completed/20260415/`。

# 技术选型

- 采用“短 description + 强约束 prompt”的组合方式。
- `description` 负责告诉模型“什么时候该选这个工具”。
- `prompt` 负责告诉模型“选了以后必须怎么用、不要怎么用”。
- 尽量保留对真实能力边界有决定性影响的信息，删除说明书式冗长描述。

# 验收标准

- 五个工具的 `prompt` 均明显短于当前版本。
- `description` 更清晰地区分“首轮筛选 / 文本回映 / 文件细读 / 文档导航 / 函数补丁”职责。
- 关键限制条件仍完整保留，避免因过度压缩导致误用。
- `skill.json` 可被正常解析。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `vulcan-codekit` 五个核心工具的 `prompt` 统一收敛为短规则风格，减少说明书式冗长描述。
- 同步优化 `description`，让模型在未显式指定工具时更容易依据职责边界主动选择正确工具。
- 保留对真实协议边界有决定性影响的约束信息，例如单目录、显式文件、默认 ignore、完整函数替换与大结果缓存提示。
- 完成 `skill.json` 结构校验，确认本轮提示词重构未破坏技能配置格式。

## 2. 📂文件变更清单

### 修改文件

- `runtime/lua_skills/vulcan-codekit/skill.json`

### 新增文件

- `docs/plan/20260415-26-CODEKIT_PROMPT_OPTIMIZATION.md`（后续已归档）

### 删除文件

- 无

## 3. 💻关键代码调整详情

- `codekit-ast-detail`：强化“仅在已知精确文件路径时使用”的前置约束，压缩为显式文件、注释开关与缓存提示三类核心规则。
- `codekit-rg`：突出“第二步工具”定位，明确禁止首轮探索，并保留 `ext` 为扩展名过滤、`show_full_function` 为精确审阅开关的关键语义。
- `codekit-markdown-menu`：强化“仅做文档导航，不做正文摘要”的边界，并明确截断后的正确重试策略。
- `codekit-ast-tree`：突出“仅一个目录、固定递归、默认 ignore”的使用模型，引导结果自然流向 `codekit-ast-detail` 或 `codekit-rg`。
- `codekit-patch`：将“只 patch 函数/方法”和“replacement 必须是完整函数源码”提升为最高优先级规则。

## 4. ⚠️遗留问题与注意事项

- 本轮仅调整 `skill.json` 中的工具描述与提示词，没有修改运行时代码逻辑。
- 当前未额外更新外部文档，因为工具真实行为没有变化；若后续需要统一对外文档语气，可再单独做一轮文案收敛。
- 本次只做了 JSON 解析层验证；若后续还要进一步验证模型实际调用效果，建议在真实 `tools/list -> tools/call` 场景下继续观察一轮。
