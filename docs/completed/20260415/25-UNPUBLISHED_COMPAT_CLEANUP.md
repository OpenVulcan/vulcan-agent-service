# 任务目标

在 `vulcan-codekit` 尚未正式发布的前提下，清理当前仓库中为兼容旧协议、旧工具名、旧目录注入变量或历史描述而保留的过渡逻辑，收敛为单一、明确、可破坏式演进的实现形态。

# 执行步骤

1. 先提交当前已经完成并验证通过的 `codekit-rg` 纯文本化与共享长度规则改动，固定干净基线。
2. 梳理 `vulcan-codekit` 中仍保留的兼容层，包括旧 skill 目录变量名回退、旧工具名表述、旧协议注释与历史文档提示。
3. 移除无需保留的兼容逻辑，统一对外只保留当前正式命名与当前协议。
4. 同步更新 `skill.json`、运行时代码注释与开发文档，避免继续向模型暴露旧用法。
5. 做本地回归验证，确认清理后 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-rg`、`codekit-markdown-menu`、`codekit-patch` 仍可正常工作。
6. 补充执行变更总结并在完成后归档计划文件。

# 技术选型

- 采用“未发布即不兼容”的清理原则，不为旧名字、旧参数或旧变量保留兜底逻辑。
- 优先删除无意义兼容分支，而不是继续叠加判断。
- 保持工具行为单一明确，减少模型和维护者面对多套协议时的歧义。

# 验收标准

- 旧工具名、旧变量名和无必要兼容分支已被移除或统一。
- 文档和运行时代码不再继续宣传旧协议。
- 关键工具回归验证通过。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 清理 `vulcan-codekit` 中未发布阶段不再需要保留的兼容层，统一只保留当前 `codekit-*` 协议与当前宿主注入变量。
- 移除多个入口对旧 skill 目录变量名的回退逻辑，避免继续维持 `ast-grep` / 旧 AST 名称的双轨实现。
- 统一修正文档、注释与错误码中的旧命名描述，确保模型和维护者只看到当前正式协议。
- 补充回归验证，确认 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-rg`、`codekit-markdown-menu` 仍可正常工作。

## 2. 📂文件变更清单

### 修改文件

- `docs/lua_skills.md`
- `runtime/lua_skills/vulcan-codekit/main.lua`
- `runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- `runtime/lua_skills/vulcan-codekit/main_markdown_menu.lua`
- `runtime/lua_skills/vulcan-codekit/main_patch.lua`
- `runtime/lua_skills/vulcan-codekit/main_rg.lua`
- `runtime/lua_skills/vulcan-codekit/skill.json`

### 新增文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

- `main.lua`：移除主 AST 入口对旧 `__skill_dir_codekit_ast` 与 `__skill_dir_ast_grep` 的目录变量回退，只保留 `codekit-ast-detail` 当前宿主注入变量。
- `main_ast_tree.lua`：移除旧目录变量回退，并保持 `codekit-ast-tree` 只接受单目录的当前协议；同时清理 prompt 中“plural for compatibility”一类历史表述。
- `main_markdown_menu.lua`、`main_patch.lua`、`main_rg.lua`：统一切换为当前 `codekit-ast-detail` 命名，删除旧错误码前缀 `vmcp_ast_*`，并移除旧 skill 目录变量回退。
- `skill.json`：更新 `codekit-ast-tree` 的工具说明，去除未发布阶段不再需要的兼容措辞，确保对外提示只描述当前正式协议。
- `docs/lua_skills.md`：同步更新热重载示例与 `ast-tree` 使用说明，避免继续向模型暴露旧命名或“沿用兼容”式表述。

## 4. ⚠️遗留问题与注意事项

- 当前工作区仍存在一批与本任务无关的既有删除项、未跟踪归档文档和 `tmp/` 目录，本次未做处理也未纳入提交。
- 调试验证依赖 `tmp/config_codekit_runtime.yaml` 将 `lua_skills_override` 指向运行时目录；若后续要在默认构建输出目录直接观察新行为，仍需要重新构建或同步运行时副本。
