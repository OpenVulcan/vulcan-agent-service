# 任务目标

按新的工具分工重新收敛 `vulcan-codekit`：将主 AST 工具改名为 `codekit-ast-detail` 并收敛为文件级纯文本详情工具；同时让 `codekit-ast-tree` 保持单目录递归扫描，但恢复 `ext` 与 `noignore` 能力，避免大型仓库把 `dist`、`target` 等目录内容错误卷入结果。

# 执行步骤

1. 审查 `codekit-ast`、`codekit-ast-tree` 当前实现与 `skill.json` 对外协议，确认需要调整的入口、参数和文档引用。
2. 将 `codekit-ast` 改名为 `codekit-ast-detail`，并收敛为：
   - 仅支持显式文件列表输入
   - 纯文本输出
   - 移除 `recursive`、`ext`、`export_md_path` 等无意义参数
3. 调整 `codekit-ast-tree`：
   - 只接受单个目录
   - 固定递归扫描子目录
   - 恢复 `ext` 过滤
   - 恢复 `noignore`，且语义保持“默认启用忽略，只有 `noignore=true` 才关闭忽略”
4. 同步更新 `skill.json`、`docs/lua_skills.md` 及代码中的命名/提示词引用，保证新旧名称不混杂。
5. 通过本地调用验证 `codekit-ast-detail` 与 `codekit-ast-tree` 的新协议，包括文件输入、目录输入、`ext` 过滤与 `noignore` 行为。
6. 补充执行变更总结并归档计划文件。

# 技术选型

- 保持“tree 负责目录导航，detail 负责文件详情”的双工具分工，避免同一工具在目录导航与文件详情之间来回摇摆。
- 对 `codekit-ast-tree` 保留默认忽略规则与扩展名过滤，确保大型仓库场景下的结果可控。
- 对 `codekit-ast-detail` 采用纯文本输出，减少 JSON 包装噪音，贴近文件级详情阅读场景。

# 验收标准

- `codekit-ast` 已改名为 `codekit-ast-detail`。
- `codekit-ast-detail` 只接受显式文件列表并返回纯文本。
- `codekit-ast-tree` 只接受单目录，固定递归扫描，并支持 `ext` 与 `noignore`。
- 文档、提示词和命名引用完成同步更新。
- 计划文件完成执行总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 将主 AST 工具正式收敛为 `codekit-ast-detail`，只接受显式文件列表，输出改为 Markdown 纯文本，并移除了目录扫描、递归、扩展名过滤、忽略切换和导出 Markdown 等不再适用的入口参数。
- 调整 `codekit-ast-tree` 为“单目录、固定递归”的仓库级导航工具，同时恢复 `ext` 与 `noignore`，确保默认情况下仍遵守 `.gitignore`、`.ignore` 与内建黑名单，不会把 `dist`、`target` 之类目录卷入默认结果。
- 修复了主 AST 入口收窄后导致共享 helper upvalue 丢失的问题，确保 `codekit-rg`、`codekit-markdown-menu`、`codekit-ast-tree` 继续可以复用参数校验与扫描辅助能力。
- 同步更新了 `skill.json`、开发文档与第三方声明中的工具名、参数提示和使用示例，避免模型继续学习旧的 `codekit-ast` 协议。

## 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/main.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_rg.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_patch.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_markdown_menu.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`runtime/lua_skills/vulcan-codekit/THIRD_PARTY_NOTICES.md`
- 修改：`docs/lua_skills.md`

## 3. 💻关键代码调整详情

- 在 `main.lua` 中新增 `codekit-ast-detail` 的文件列表参数校验逻辑，限制最多 20 个显式文件，并在目录或混合路径输入时返回明确错误。
- 在 `main.lua` 中新增纯文本详情渲染与超限缓存提示逻辑，返回结构改为 `# AST DETAIL SUMMARY` 加逐文件详情区块。
- 在 `main.lua` 中增加共享 helper 保活分支，保留 `validate_path_argument`、`validate_recursive_argument`、`validate_noignore_argument`、`validate_extension_argument` 这些被其他工具通过闭包 upvalue 抽取的函数。
- 在 `main_ast_tree.lua` 中恢复 `ext` 与 `noignore` 参数解析，并把 `collect_files(...)` 的目录收集切回“固定递归 + 可选扩展名过滤 + 默认启用忽略规则”。
- 在 `skill.json` 中把主 AST tool 对外名称调整为 `codekit-ast-detail`，同步改为 `return_type: "string"`，并重写了 `codekit-ast-tree` 与 `codekit-rg` 的说明文字。
- 在 `docs/lua_skills.md` 中更新 `codekit-ast-detail` / `codekit-ast-tree` 的分工说明，并修正 Lua 调用示例，避免继续以 JSON 结构读取纯文本结果。

## 4. ⚠️遗留问题与注意事项

- 本次只修改了当前任务涉及的技能文件，没有处理工作区中其他与本任务无关的既有删除项与未提交变更。
- `codekit-ast-tree` 在 `noignore=true` 且扫描范围非常大时，仍可能触发超限缓存写盘；本次验证前额外创建了 `output/temp/mcp/cache` 目录用于保证该路径可写。
- `docs/lua_skills.md` 中仍保留了与历史 `vmcp-*` 调试文档相关的内容，这些属于旧记录说明，本次未做大范围重写。
