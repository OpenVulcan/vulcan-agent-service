## 任务目标

将 `ast-grep` Lua skill 的对外库名统一调整为 `vulcan-codekit`，并将对外暴露的 `vmcp-*` 工具名统一调整为 `codekit-*`，使整体命名更贴近代码分析工具包定位。

## 详细执行步骤

1. 梳理 `skill.json`、Lua 入口、文档与测试脚本中所有对外暴露的 `ast-grep` / `vmcp-*` 命名。
2. 调整 `skill.json` 中的 skill 名、tool 名、资源 URI、资源展示名称与相关 prompt 文案。
3. 同步更新 Lua 入口中的导出标题、提示文案与其他对外显示内容，确保新旧命名不混杂。
4. 更新仓库文档和本地验证脚本中的调用示例与工具名引用。
5. 通过 `--call-tools` 实际验证新的 `codekit-*` 工具名可正常调用，旧命名不再作为主命名暴露。

## 技术选型

- 优先修改对外接口名称和用户可见文案，尽量不扩大到无必要的内部变量和目录结构变更。
- 保持技能目录仍位于 `runtime/lua_skills/ast-grep/`，仅调整 `skill.json` 暴露名与外部调用名，避免引入额外加载路径风险。
- 对文档和脚本采用一致口径：skill 名统一为 `vulcan-codekit`，tool 名统一为 `codekit-*`。

## 验收标准

- `skill.json` 中的 skill 名更新为 `vulcan-codekit`。
- 对外 tool 名从 `vmcp-*` 全部调整为 `codekit-*`。
- 文档、导出标题、调用示例等用户可见文本不再以旧命名作为主名称。
- 实际 `--call-tools` 调用可通过 `codekit-*` 名称正常运行。
- 计划文件最终补齐执行变更总结并归档到 `docs/completed/20260415/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 skill 对外库名从 `ast-grep` 调整为 `vulcan-codekit`。
- 将对外工具名统一从 `vmcp-*` 调整为 `codekit-*`，覆盖 AST、RG、Markdown 菜单与 Patch 四个入口。
- 同步更新了资源 URI、prompt 名称、导出标题、帮助文案和本地验证脚本中的调用名，保证外部口径一致。
- 为避免 `lua_module` 改名后丢失宿主注入的 skill 目录变量，补上了新旧变量名兼容逻辑，确保 `codekit-*` 工具可以实际运行。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`runtime/lua_skills/ast-grep/main.lua`
- 修改：`runtime/lua_skills/ast-grep/main_rg.lua`
- 修改：`runtime/lua_skills/ast-grep/main_patch.lua`
- 修改：`runtime/lua_skills/ast-grep/main_markdown_menu.lua`
- 修改：`runtime/lua_skills/ast-grep/resources/guide.md`
- 修改：`runtime/lua_skills/ast-grep/prompts/first_pass.md`
- 修改：`runtime/lua_skills/ast-grep/templates/example.md`
- 修改：`docs/lua_skills.md`
- 修改：`scripts/verify_vmcp_ast_comment_notes.py`

### 3. 💻关键代码调整详情

- `skill.json` 中将 skill 名改为 `vulcan-codekit`，tool 名分别改为 `codekit-ast`、`codekit-rg`、`codekit-markdown-menu`、`codekit-patch`。
- 资源 URI 从 `skill://ast-grep/...` 调整为 `skill://vulcan-codekit/...`，prompt 名从 `vmcp_ast_first_pass` 调整为 `codekit_ast_first_pass`。
- Lua 入口中所有对外导出标题、关键报错和帮助描述统一替换为 `codekit-*` 命名。
- `get_skill_dir()` 新增了对 `__skill_dir_codekit_*` 与旧变量名的兼容回退，修复 `lua_module` 改名后运行时找不到 `main.lua` 和依赖目录的问题。
- 本地回归脚本改为通过 `codekit-ast` 发起调用，保证新命名可持续验证。

### 4. ⚠️遗留问题与注意事项

- 技能目录仍然保留在 `runtime/lua_skills/ast-grep/`，这是刻意保守的实现方式，用于避免引入目录迁移和宿主加载路径风险。
- 底层依赖二进制仍然是 `ast-grep` 与 `rg`，因此相关依赖示例和下载日志仍会保留 `ast-grep` 名称，这不影响对外 skill / tool 名称已经切换到 `vulcan-codekit` / `codekit-*`。
