# 任务目标

为 `runtime/lua_skills/vulcan-codekit` 增加第三方版权声明文件，明确记录该 skill 包依赖的 `ripgrep (rg)` 与 `ast-grep` 的官方协议信息、来源地址与使用说明，补齐对外分发时的合规材料。

# 执行步骤

1. 确认 `runtime/lua_skills/vulcan-codekit` 当前目录结构与现有说明文件，判断第三方声明文件的最佳落点与命名方式。
2. 查验 `ripgrep (rg)` 与 `ast-grep` 的官方许可证信息、版权归属与上游仓库地址，确保声明内容准确。
3. 在 `runtime/lua_skills/vulcan-codekit` 下新增版权声明文件，结构化写入两项第三方依赖的协议信息与说明。
4. 如有必要，补充 skill 包内的引用说明，确保后续维护者能够快速找到该声明文件。
5. 完成后进行自检，确认文件命名、内容准确性与路径位置符合当前仓库组织方式。

# 技术选型

- 声明文件采用 Markdown 格式，便于 skill 包分发与人工审阅。
- 协议信息以官方仓库或官方许可证文本为准，不使用未经核实的二手描述。
- 在不引入多余结构变更的前提下，将声明文件直接放置于 `runtime/lua_skills/vulcan-codekit` 根目录，降低查找成本。

# 验收标准

1. `runtime/lua_skills/vulcan-codekit` 下存在新的第三方版权声明文件。
2. 文件中明确包含 `ripgrep (rg)` 与 `ast-grep` 的名称、用途、上游来源与许可证信息。
3. 协议信息表述准确，能够支撑该 skill 包的分发合规说明。
4. 计划文件在任务完成后补齐执行变更总结，并按规范归档到 `docs/completed/20260415/`。

---

# 执行变更总结

## 1. 核心修复与调整概述

已在 `runtime/lua_skills/vulcan-codekit` 根目录新增第三方版权声明文件，集中整理 `ast-grep` 与 `ripgrep (rg)` 的上游来源、许可证类型、版权声明与随包分发注意事项，用于补齐当前 skill 包的合规归档材料。

## 2. 📂文件变更清单

新增：

- `runtime/lua_skills/vulcan-codekit/THIRD_PARTY_NOTICES.md`

修改：

- `docs/plan/20260415-08-THIRD_PARTY_NOTICE_FILE.md`

删除：

- 无

## 3. 💻关键代码调整详情

- 本次未修改运行时代码逻辑。
- 新增的声明文件以 Markdown 形式记录两项第三方工具的用途与许可证边界，便于 skill 包后续打包、审查与维护。
- `ast-grep` 条目中写明其采用 MIT License，并记录官方仓库与版权声明。
- `ripgrep` 条目中写明其采用 `Unlicense OR MIT License` 双许可证发布，并补充随包分发时建议保留的上游许可证材料。

## 4. ⚠️遗留问题与注意事项

- 当前已补齐声明文件，但若未来实际对外分发二进制工具，仍建议在发布物中同时附带上游原始许可证文件。
- 后续若 `dependencies.yaml` 新增新的第三方可执行工具，应继续在本声明文件中追加对应许可证信息。
