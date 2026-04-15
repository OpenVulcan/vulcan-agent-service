# 任务目标

将 `vulcan-codekit` Lua skill 中新增的 `vulcan_codekite_skill` prompt 从“跳转到本地 Codex skill”改为“直接内嵌公共工作流内容”，使支持 MCP prompt 的客户端可以直接消费这份通用规则，而 Codex 继续单独使用本地 `vulcan-codekite` skill。

# 执行步骤

1. 审阅当前 `vulcan_codekite_skill` prompt 文件与本地 `vulcan-codekite` skill 内容，提炼应同步到公共 prompt 的核心规则。
2. 将 prompt 文件改写为独立可用的通用工作流说明，不再依赖本地 skill 链接。
3. 保持 `skill.json` 中现有 prompt 注册项不变，仅同步 prompt 正文内容。
4. 校验 `skill.json` 仍可正常解析，并确认新 prompt 文件内容已落地。
5. 追加执行变更总结并将计划文件归档。

# 技术选型

- 公共 prompt 只保留对 `codekit-*` 工具的实际工作流规则，不再引用本地 `C:\Users\20000\.codex\skills` 路径。
- Codex 继续使用独立 `vulcan-codekite` skill；支持 MCP prompt 的客户端直接消费这份内联公共版本。
- 尽量保持 prompt 简洁，但保留关键路径：目录建图、精确文件细读、文本回映结构、文档筛选、函数级 patch。

# 验收标准

- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md` 已改为独立可读的公共工作流内容。
- prompt 不再依赖本地 skill 绝对路径。
- `skill.json` 能正常解析。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `vulcan_codekite_skill` prompt 从“跳转到本地 Codex skill”改为“直接内嵌公共版 `codekit-*` 工作流”。
- 这样支持 MCP prompt 的客户端可以直接消费这份规则，不需要额外安装 Codex 本地 skill。
- Codex 仍可继续使用单独的 `vulcan-codekite` skill，不与公共 prompt 冲突。

## 2. 📂文件变更清单

### 修改文件

- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`
- `docs/plan/20260415-29-VULCAN_CODEKITE_PROMPT_INLINE_SYNC.md`（后续已归档）

### 新增文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

- prompt 正文改为直接描述 `codekit-ast-tree`、`codekit-ast-detail`、`codekit-rg`、`codekit-markdown-menu`、`codekit-patch` 的推荐使用顺序。
- 保留了工具选择规则、误用保护规则与常见工作环路，确保 prompt 在支持方可直接独立使用。
- 删除了对 `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md` 的本地路径依赖。

## 4. ⚠️遗留问题与注意事项

- 本轮没有改动 `skill.json` 的 prompt 注册结构，只替换了 prompt 文件正文。
- 目前仅做了 `skill.json` 解析校验，没有额外做 MCP `prompts/get` 端到端验证。
- 如果后续公共 prompt 和 Codex 独立 skill 的工作流继续演进，建议定期同步两边内容，避免经验漂移。
