# 任务计划：调整 export_md_path 为仅导出不内联输出

## 任务目标

修改当前仓库中 `vmcp_ast` 与 `vmcp_rg` 的行为：当调用参数中显式设置 `export_md_path` 时，工具不再返回内联结构结果或预览内容，只执行 Markdown 导出，并返回简洁提示信息，告知导出文件已生成及其路径。

## 执行步骤

1. 定位 `vmcp_ast` 与 `vmcp_rg` 的实际实现文件、返回结构及 `export_md_path` 相关逻辑。
2. 分析当前导出逻辑与普通返回逻辑的分支关系，确定最小且稳定的修改点。
3. 修改 `vmcp_ast` 与 `vmcp_rg` 在设置 `export_md_path` 时的返回行为，使其仅返回导出成功提示和目标路径。
4. 如有必要，补充相关注释或提示文本，确保行为一致且易于理解。
5. 执行最小验证，确认两项工具在 `export_md_path` 场景下不再内联输出，并保留正常导出能力。
6. 对照计划补充执行变更总结，并归档到 `docs/completed/20260414/`。

## 技术选型

- 优先在工具结果组装层修改返回值，避免影响 AST/RG 的主体扫描逻辑。
- 保持未设置 `export_md_path` 时的现有行为不变，确保兼容现有调用方式。
- 只在显式传入 `export_md_path` 时启用“静默导出”模式。

## 验收标准

- `vmcp_ast` 设置 `export_md_path` 时仅返回“已生成 Markdown 文件及路径”的提示信息。
- `vmcp_rg` 设置 `export_md_path` 时仅返回“已生成 Markdown 文件及路径”的提示信息。
- 未设置 `export_md_path` 时，两个工具保持原有输出行为。
- 相关改动通过至少一次实际调用验证。

## 验证结果

1. 已定位实现位于 `runtime/lua_skills/ast-grep/main.lua` 与 `runtime/lua_skills/ast-grep/main_rg.lua` 的结果收尾函数 `finalize_ast_result`、`finalize_rg_result`。
2. 已修改两处逻辑：当显式传入 `export_md_path` 时，工具先写出 Markdown 文件，然后立即返回仅包含导出路径与成功消息的精简结果，不再返回内联结构内容。
3. 已同步更新 `runtime/lua_skills/ast-grep/skill.json` 与 `docs/lua_skills.md`，使参数说明与操作提示与新行为保持一致。
4. 由于当前对话内已加载的 MCP 工具实例未热重载到修改后的 Lua 文件，实际验证改用仓库内置 `--call-tools` 模式完成。
5. 实际验证结果：
   - `vmcp-ast` 在设置 `export_md_path` 后返回：
     - `exported_markdown_path`
     - `message = "Markdown file has been generated at ..."`
   - `vmcp-rg` 在设置 `export_md_path` 后返回相同风格的精简成功消息
6. 验证期间生成的临时配置、导出文件、联接目录和依赖产物已清理，未保留测试垃圾文件。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作将 `vmcp_ast` 与 `vmcp_rg` 的 `export_md_path` 行为从“额外导出 Markdown，但仍返回内联结果”调整为“静默导出模式”。现在只要调用方显式提供 `export_md_path`，工具就会只返回导出成功提示与目标路径，从而减少上下文占用和重复输出，更符合批量导出或会话外部消费场景。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main.lua`
- 修改：`runtime/lua_skills/ast-grep/main_rg.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`
- 新增：`docs/plan/20260414-25-EXPORT_MD_PATH_SILENT_EXPORT_BEHAVIOR.md`
- 后续归档目标：`docs/completed/20260414/25-EXPORT_MD_PATH_SILENT_EXPORT_BEHAVIOR.md`

### 3. 💻关键代码调整详情

- 在 `main.lua` 的 `finalize_ast_result` 中新增显式导出分支：
  - 先执行 `write_text_file(export_md_path, markdown_text)`
  - 成功后直接返回 `{ exported_markdown_path, message }`
  - 不再继续构造 `serializable_result`、不再参与内联大小判断
- 在 `main_rg.lua` 的 `finalize_rg_result` 中采用相同策略，保证 AST/RG 行为一致。
- 更新 `skill.json` 中两个工具的 `export_md_path` 参数描述与 prompt 文案，明确说明该参数会触发“只返回生成文件提示”的行为。
- 更新 `docs/lua_skills.md` 的大结果处理规则说明，避免文档仍描述旧语义。

### 4. ⚠️遗留问题与注意事项

- 当前会话中的 MCP 工具实例不会自动热重载仓库内 Lua 代码，因此交互式工具调用看到的仍是旧行为；重新加载工具实例后即可获得新逻辑。
- 未设置 `export_md_path` 的调用路径没有改动，仍保持原有“内联返回 / 大结果落盘预览”的行为。
