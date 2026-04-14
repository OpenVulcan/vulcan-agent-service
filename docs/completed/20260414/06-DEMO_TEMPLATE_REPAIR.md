## 任务目标

修复 `runtime/lua_skills/__demo` 模板 skill，使其在新的多 group、多入口结构下真正具备清晰、可复制的演示价值。重点是让不同 tool 入口对应不同示例脚本，并同步更新说明文档与运行时副本。

## 执行步骤

1. 检查当前 `__demo` 的目录结构与入口绑定方式，定位“虽然多入口但演示不充分”的问题。
2. 调整 `__demo` 的 tool 示例，使不同 tool 入口各自绑定独立 Lua 文件，输出行为清晰区分。
3. 如有必要，补充缺失的模板文件或说明文字，确保复制出去后可直接修改使用。
4. 同步更新 `docs/lua_skills.md` 中关于 `__demo` 的说明。
5. 同步 `output/lua_skills/__demo` 副本，并进行构建验证。

## 技术选型

- 保持 `__demo` 继续作为不自动加载的内部模板目录。
- 每个 tool 示例使用独立 `lua_entry` 文件，强化“多入口技能”的目录组织示范。
- 继续保留静态与动态 resource/prompt/template 示例，避免模板能力缩水。

## 验收标准

1. `__demo` 至少包含两个清晰区分的 tool 入口示例，并分别绑定不同 Lua 文件。
2. `skill.json`、目录文件与文档说明保持一致。
3. `output/lua_skills/__demo` 已同步更新。
4. `cargo build` 通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已修复 `__demo` 模板在多入口结构下“两个 tool 共用同一份脚本、演示不够直观”的问题。
- 现在主 tool 与第二个 tool 分别绑定 `main.lua` 与 `main_summary.lua`，更符合实际项目中“多工具拆分实现文件”的推荐写法。
- 已同步更新文档说明与 `output` 目录副本，确保模板复制体验一致。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/__demo/main.lua`
- 新增：`runtime/lua_skills/__demo/main_summary.lua`
- 修改：`runtime/lua_skills/__demo/skill.json`
- 修改：`docs/lua_skills.md`
- 同步：`output/lua_skills/__demo/**`
- 新增：`docs/plan/20260414-06-DEMO_TEMPLATE_REPAIR.md`

### 3. 💻关键代码调整详情

- 将 `demo_template_summary` 的 `lua_entry` 从 `main.lua` 改为 `main_summary.lua`，使第二个 tool 具备独立实现文件。
- 强化 `main.lua` 的返回内容和说明文本，明确其“主工具入口模板”的定位。
- 新增 `main_summary.lua`，演示第二个 tool 如何使用独立参数契约与不同返回结构。
- 在 `docs/lua_skills.md` 中补充 `main_summary.lua` 的模板目录说明，帮助开发者理解多 tool skill 的推荐目录组织方式。

### 4. ⚠️遗留问题与注意事项

- `__demo` 目录仍然以 `__` 前缀命名，运行时不会自动加载，这是模板目录的预期行为。
- 如果后续要继续增强模板，可再增加 `init_scripts` 示例或更多 group 之间的拆分示例，但本次已满足“多入口清晰可复制”的目标。
