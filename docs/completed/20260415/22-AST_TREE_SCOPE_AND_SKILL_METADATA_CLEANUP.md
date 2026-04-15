# 任务目标

进一步收敛 `codekit-ast-tree` 的职责边界，并清理 `vulcan-codekit` 中尚未准备好的附属能力配置。具体包括：让 `codekit-ast-tree` 只接受单个目录输入、关闭 `comment` 能力、修复前一轮审查发现的问题，并移除当前 skill 中未正式启用的 `resources`、`resource_templates`、`prompts` 配置。

# 执行步骤

1. 审查 `codekit-ast-tree` 当前参数协议、返回格式与错误处理路径，确认需要收敛的行为边界。
2. 修改 `codekit-ast-tree`：
   - 仅支持传入单个目录
   - 明确拒绝多目录与文件输入
   - 移除 `comment` 支持
   - 修复审查中发现的静默跳过与统计口径问题
3. 同步更新 `skill.json`、`docs/lua_skills.md` 等说明，使对外协议与实现保持一致。
4. 清理当前 skill 中未正式启用的 `resources`、`resource_templates`、`prompts` 配置，并视情况删除对应静态文件。
5. 通过本地调用验证目录输入、多目录拒绝、文件拒绝与纯文本输出行为。
6. 补充执行变更总结并归档计划文件。

# 技术选型

- 保持 `codekit-ast-tree` 为轻量级目录导航工具，但进一步收窄输入形态，避免多目录和文件混用造成歧义。
- 对已识别缺陷直接修复，不保留“日志里有、正文不报”的隐性失败语义。
- 对 skill 元数据做最小必要清理，移除未准备好的附属能力暴露面，降低误用风险。

# 验收标准

- `codekit-ast-tree` 只接受单个目录输入，且错误提示明确。
- `comment` 参数已移除且实现中不再生效。
- 已修复静默跳过与统计口径问题。
- skill 中未启用的 `resources`、`resource_templates`、`prompts` 配置已移除。
- 文档与实际行为一致。
- 计划文件完成执行总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `codekit-ast-tree` 收敛为“单目录输入 + 纯文本目录树摘要”的稳定工具形态。
- 明确关闭 `comment` 支持，并移除 `noignore`、`ext`、`export_md_path` 等不再需要的外围协议。
- 清理了当前 `vulcan-codekit` skill 中尚未准备好的 `resources`、`resource_templates`、`prompts` 配置与对应文件。

## 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`docs/lua_skills.md`
- 删除：`runtime/lua_skills/vulcan-codekit/resources/guide.md`
- 删除：`runtime/lua_skills/vulcan-codekit/templates/example.md`
- 删除：`runtime/lua_skills/vulcan-codekit/templates/example_generator.lua`
- 删除：`runtime/lua_skills/vulcan-codekit/templates/example_params.lua`
- 删除：`runtime/lua_skills/vulcan-codekit/prompts/first_pass.md`
- 修改：`docs/plan/20260415-22-AST_TREE_SCOPE_AND_SKILL_METADATA_CLEANUP.md`

## 3. 💻关键代码调整详情

- 重写 `main_ast_tree.lua`，恢复目录级 AST 导航定位，并限定 `paths` 只能承载一个目录路径。
- 在运行时显式拒绝多目录、文件路径和 `comment` 参数，避免旧调用方式被静默接受。
- 将 `items_found` 统计调整为基于去重后的符号数量，避免与最终输出口径不一致。
- 移除 `skill.json` 中 `resources`、`resource_templates`、`prompts` 的对外暴露，并同步清理对应静态文件。

## 4. ⚠️遗留问题与注意事项

- 参数名仍沿用 `paths`，但当前协议只允许一个目录值；这是为了兼容上一轮外部命名调整，后续若希望彻底收口为 `path`，可以再做一次命名统一。
- 当前 `codekit-ast-tree` 仍保留超限落盘到 `vulcan.temp_dir/mcp/cache/` 的能力；如果后续您希望进一步取消这一层，也可以继续裁剪。
