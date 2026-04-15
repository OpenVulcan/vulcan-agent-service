# 任务目标

统一调整 `vulcan-codekit` 相关工具的忽略参数语义，并收敛 `codekit-ast-tree` 的递归开关：

- 将 `ignore` 参数改为 `noignore`
- 默认空值时仍启用忽略规则
- 仅当显式传入 `true` 时关闭忽略规则
- 对 `codekit-ast-tree` 移除无意义的递归开关，统一按目录树工具的固定行为执行

# 执行步骤

1. 梳理 `codekit-ast`、`codekit-rg`、`codekit-markdown-menu`、`codekit-ast-tree` 当前对 `ignore` 与 `recursive` 的使用路径。
2. 在共享参数校验逻辑中将 `ignore` 改为 `noignore`，并将语义调整为“默认忽略，显式 true 才不忽略”。
3. 同步修改相关工具的 `skill.json` 参数定义与提示说明。
4. 收敛 `codekit-ast-tree` 的递归行为，移除对外暴露的 `recursive` 参数并调整实现。
5. 进行真实调用验证，确认：
   - 默认仍会忽略 `.git`、`target`、`node_modules` 等目录
   - 显式 `noignore=true` 时可关闭忽略规则
   - `codekit-ast-tree` 不再依赖递归开关
6. 完成执行总结并归档计划文件。

# 技术选型

- 复用 `main.lua` 的共享参数校验函数，避免多个工具各自保留不同语义。
- `codekit-ast-tree` 保持固定递归行为，减少调用方判断成本。
- 文档、工具说明、运行时行为统一同步，避免模型按旧语义继续调用。

# 验收标准

- 相关工具对外参数统一为 `noignore`
- 默认行为仍启用忽略规则，`noignore=true` 时关闭忽略
- `codekit-ast-tree` 不再暴露 `recursive` 参数
- 实际调用验证通过

# 执行变更总结

## 1. 核心修复与调整概述

- 统一把 `ignore` 外部参数语义调整为 `noignore`：默认空值时仍启用忽略规则，仅当显式传入 `true` 时才关闭忽略。
- `codekit-ast-tree` 已移除对外 `recursive` 参数，并改为固定递归扫描子目录。
- 同步补齐了 `codekit-rg` 的忽略开关，使整组 `vulcan-codekit` 分析工具在忽略行为上保持一致。

## 2. 📂文件变更清单

### 新增

- `docs/plan/20260415-17-NOIGNORE_AND_AST_TREE_RECURSIVE_ADJUSTMENT.md`

### 修改

- `runtime/lua_skills/vulcan-codekit/main.lua`
- `runtime/lua_skills/vulcan-codekit/main_markdown_menu.lua`
- `runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- `runtime/lua_skills/vulcan-codekit/main_rg.lua`
- `runtime/lua_skills/vulcan-codekit/skill.json`
- `runtime/lua_skills/vulcan-codekit/resources/guide.md`
- `runtime/lua_skills/vulcan-codekit/templates/example_generator.lua`
- `runtime/lua_skills/vulcan-codekit/templates/example_params.lua`
- `runtime/lua_skills/vulcan-codekit/templates/example.md`
- `docs/lua_skills.md`

## 3. 💻关键代码调整详情

- 在共享入口 [main.lua](/D:/projects/vulcan-mcp-client/runtime/lua_skills/vulcan-codekit/main.lua) 中：
  - `validate_ignore_argument` 调整为 `validate_noignore_argument`
  - 运行时语义从“传 `ignore=false` 才关闭忽略”改成“传 `noignore=true` 才关闭忽略”
- 在 [main_markdown_menu.lua](/D:/projects/vulcan-mcp-client/runtime/lua_skills/vulcan-codekit/main_markdown_menu.lua) 与 [main_ast_tree.lua](/D:/projects/vulcan-mcp-client/runtime/lua_skills/vulcan-codekit/main_ast_tree.lua) 中同步切换到新的共享校验函数。
- 在 [main_rg.lua](/D:/projects/vulcan-mcp-client/runtime/lua_skills/vulcan-codekit/main_rg.lua) 中新增 `noignore` 支持：
  - `rg` 搜索阶段在 `noignore=true` 时附加 `--no-ignore --hidden`
  - 匹配结果回流到 AST 细化阶段时，也沿用同一忽略开关
- `codekit-ast-tree` 现在固定以递归方式收集目录树，不再读取外部 `recursive` 参数。
- `skill.json`、开发文档与示例模板已全部切到 `noignore` 语义，避免模型继续按旧参数调用。

## 4. ⚠️遗留问题与注意事项

- 本次只调整了当前 `vulcan-codekit` 组内工具与相关说明，历史归档文档里仍会保留旧的 `ignore` 描述，这是历史记录，不影响当前行为。
- `codekit-markdown-menu` 仍然保留 `recursive` 参数，因为它的“浅扫文档根目录”场景仍然有意义；只有 `codekit-ast-tree` 被收敛成固定递归。
- 真实验证使用了临时 `.ignore` 夹具目录，测试结束后已清理，不留仓库垃圾文件。
