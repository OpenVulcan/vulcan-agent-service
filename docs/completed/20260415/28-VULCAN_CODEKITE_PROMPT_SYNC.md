# 任务目标

将新创建的 `vulcan-codekite` Codex skill 同步到仓库内的 `vulcan-codekit` Lua skill 元数据中，新增一个 prompt 入口，用于把调用方引导到本地 `[$vulcan-codekite]` skill，而不是继续依赖过期的内嵌说明。

# 执行步骤

1. 审阅当前 `runtime/lua_skills/vulcan-codekit/skill.json` 的分组结构，确认适合新增 prompt 的位置。
2. 在 `runtime/lua_skills/vulcan-codekit/prompts/` 下新增一个最小静态 prompt 文件，内容只负责指向新的 `[$vulcan-codekite]` skill。
3. 更新 `skill.json`，为 `analysis` 分组增加对应 prompt 元数据，确保名称、描述与文件路径一致。
4. 校验 `skill.json` JSON 结构，确认新增 prompt 后配置仍可正常解析。
5. 追加执行变更总结并将计划文件归档。

# 技术选型

- 采用最小 prompt 方式，不把 `vulcan-codekite` 的整套规则再次复制到 Lua skill 中，避免双份维护。
- prompt 内容直接引用本地 skill 绝对路径，确保 Codex 可以显式跳转到正确 skill。
- 保持 `vulcan-codekit` 的核心能力仍由 tools 承载，prompt 仅承担“引导进入正确 skill”的职责。

# 验收标准

- `vulcan-codekit` 新增一个有效 prompt 入口。
- prompt 内容明确指向 `[$vulcan-codekite](C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md)`。
- `skill.json` 能正常解析。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 为仓库内的 `vulcan-codekit` Lua skill 恢复了一个最小 prompt 入口，用于把调用方显式引导到本地 `[$vulcan-codekite]` Codex skill。
- 本次没有把 `vulcan-codekite` 的整套规则复制回 Lua skill，而是采用“单点指向”策略，避免后续形成双份维护。
- 新 prompt 只承担路由职责，`vulcan-codekit` 的核心能力仍由现有 `codekit-*` tools 承载。

## 2. 📂文件变更清单

### 新增文件

- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`

### 修改文件

- `runtime/lua_skills/vulcan-codekit/skill.json`
- `docs/plan/20260415-28-VULCAN_CODEKITE_PROMPT_SYNC.md`（后续已归档）

### 删除文件

- 无

## 3. 💻关键代码调整详情

- 在 `skill.json` 的 `analysis` 分组下新增了 `prompts` 数组，并注册：
  - `name = vulcan_codekite_skill`
  - `file = prompts/vulcan_codekite_skill.md`
  - `role = user`
- 新增的静态 prompt 内容直接引用：
  - `[$vulcan-codekite](C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md)`
- prompt 正文只保留触发说明与技能入口，不重复内嵌 `codekit-*` 工作流细节。

## 4. ⚠️遗留问题与注意事项

- 本轮完成了 `skill.json` 解析校验，但没有额外启动 MCP 运行时去做 `prompts/list` 端到端验证。
- 如果后续 `vulcan-codekite` skill 的路径发生变化，需要同步更新该 prompt 文件中的绝对路径。
