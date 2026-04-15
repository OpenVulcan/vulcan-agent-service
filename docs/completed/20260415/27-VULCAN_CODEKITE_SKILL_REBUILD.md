# 任务目标

基于当前 `vulcan-codekit` 的最新工具协议，删除旧的 `vmcp-ast-rg-patch` skill，并重新创建一个面向 Codex 的新 skill，用于指导模型正确使用 `codekit-ast-tree`、`codekit-ast-detail`、`codekit-rg`、`codekit-markdown-menu` 与 `codekit-patch`。

# 执行步骤

1. 审阅旧 `vmcp-ast-rg-patch` skill 与 `skill-creator` 规范，提炼可复用经验与已失效内容。
2. 结合当前 `vulcan-codekit` 工具真实协议，重新设计新 skill 的触发描述、工作流、边界规则与输出期待。
3. 在 `C:\Users\20000\.codex\skills` 下创建新 skill 目录与 `SKILL.md`，生成面向当前工具的新技能内容。
4. 删除旧的 `vmcp-ast-rg-patch` skill，避免 Codex 后续继续触发过期规则。
5. 运行 `skill-creator` 提供的快速校验脚本，确认新 skill 结构和 frontmatter 合法。
6. 记录执行变更总结并将计划文件归档。

# 技术选型

- 新 skill 以当前 `codekit-*` 工具协议为唯一事实来源，不保留旧 `vmcp-*` 工作流兼容描述。
- 重点强调“先目录建图，再精确文件细读，再按需 regex 缩窄，最后才做函数级 patch”的真实使用路径。
- 针对 Codex 的使用习惯，优先把触发条件写进 frontmatter description，把具体规则写进正文，避免依赖 MCP prompt。

# 验收标准

- 新 skill 已在 `C:\Users\20000\.codex\skills` 下创建完成。
- 旧 `vmcp-ast-rg-patch` 目录已删除。
- 新 skill 内容与当前 `codekit-*` 工具能力一致，不再包含旧 `vmcp-*` 协议。
- 快速校验脚本通过。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 基于当前 `vulcan-codekit` 的真实工具协议，重建了新的 Codex skill：`vulcan-codekite`。
- 新 skill 不再沿用旧 `vmcp-*` 工作流，而是收敛为当前 `codekit-ast-tree`、`codekit-ast-detail`、`codekit-rg`、`codekit-markdown-menu`、`codekit-patch` 的实际使用路径。
- 删除了旧的 `vmcp-ast-rg-patch` 技能目录，避免后续继续触发失效经验。
- 使用 `skill-creator` 的 `quick_validate.py` 完成了新 skill 的结构校验。

## 2. 📂文件变更清单

### 新增文件

- `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
- `C:\Users\20000\.codex\skills\vulcan-codekite\agents\openai.yaml`

### 删除文件

- `C:\Users\20000\.codex\skills\vmcp-ast-rg-patch\`（旧 skill 整体目录删除）

### 修改文件

- `docs/plan/20260415-27-VULCAN_CODEKITE_SKILL_REBUILD.md`（后续已归档）

## 3. 💻关键代码调整详情

- 新 `vulcan-codekite` skill 的 frontmatter description 重点强化了触发条件，让 Codex 在“未知仓库接手、文件挑选、文本回映结构、文档导航、函数级替换”场景下更容易主动命中该 skill。
- `SKILL.md` 主体采用任务驱动结构，明确规定：
  - 未知目录先走 `codekit-ast-tree`
  - 已知精确文件再走 `codekit-ast-detail`
  - 已有文本线索时走 `codekit-rg`
  - 文档筛选走 `codekit-markdown-menu`
  - 完整函数替换才使用 `codekit-patch`
- 修正了新 skill `agents/openai.yaml` 中默认提示词里 `$vulcan-codekite` 被 shell 吃掉的问题，确保 UI 默认调用文案正确。

## 4. ⚠️遗留问题与注意事项

- 新 skill 已基于当前 `codekit-*` 工具协议编写；若后续工具名、参数或工作流继续调整，需要同步维护 `SKILL.md`。
- 本轮只做了结构合法性校验，没有额外做前向压测；若后续想继续提高触发准确率，可以再基于真实任务样本做一轮使用反馈迭代。
