# 任务计划：修复 `codeview_ast` 注释 token 的字面量替换错误

## 任务目标

修复 `codeview_ast` 在扫描 Lua 文件时，针对块注释 token（如 `--[[`、`]]`）做注释提取时误把字面量 token 当作 Lua pattern 使用，导致：

```text
malformed pattern (missing ']')
```

本次目标如下：

1. 修复 `extract_leading_comment` 中对块注释起止 token 的替换方式；
2. 顺手检查同文件内其他基于注释 token 的 `gsub` 调用，避免同类 pattern 注入问题；
3. 保持现有结构提取行为不退化；
4. 通过真实 MCP 调用验证 `path = D:\projects\vulcan-mcp-client`、`ext = lua` 场景恢复正常。

## 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 中所有与注释 token 相关的 `gsub` / pattern 处理位置。
2. 引入安全的“按字面量替换”辅助逻辑，避免 `--[[`、`]]`、`"""` 等 token 被当作 Lua pattern 解释。
3. 修复 `extract_leading_comment` 与 `extract_docstring` 等相关路径。
4. 执行构建同步。
5. 使用真实 MCP 调用验证：
   - `path = D:\projects\vulcan-mcp-client`
   - `recursive = false`
   - `ext = lua`
   场景下不再报 `malformed pattern`。
6. 对照计划补充执行总结并归档到 `docs/completed/20260413/`。

## 技术选型

- 优先使用 Lua pattern 转义或字面量替换辅助函数，而不是继续直接把 token 喂给 `string.gsub`。
- 修复范围控制在注释 token 处理路径，不扩大到无关的结构归一化逻辑，避免引入额外噪音。

## 验收标准

- `codeview_ast` 扫描 Lua 文件或 Lua 目录时不再出现 `malformed pattern (missing ']')`。
- 注释提取逻辑仍能正常返回可读文本。
- 构建通过，真实 MCP 回归通过。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 修复了 `codeview_ast` 在提取 Lua 块注释时，将 `--[[`、`]]` 这类注释 token 直接传给 `string.gsub` 作为 Lua pattern 的问题。
- 引入按字面量替换的辅助函数，避免注释 token 被 Lua pattern 引擎误解释，从而触发：
  - `malformed pattern (missing ']')`
- 同时修复了同类路径上的 docstring token 替换，避免后续在三引号等 token 上再次踩到同样的问题。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`docs/plan/20260413-11-CODEVIEW_AST_LITERAL_COMMENT_TOKEN_FIX.md`

### 3. 💻关键代码调整详情

- `main.lua`
  - 新增：
    - `escape_lua_pattern`
    - `replace_literal`
  - `extract_leading_comment` 中对块注释 `start_token / end_token` 的移除逻辑改为使用字面量替换；
  - `extract_docstring` 中对 `token` 的移除逻辑改为使用字面量替换；
  - 额外修正了新增 helper 的双语注释文本，避免在 Lua 块注释正文中再次出现会截断注释的危险字面量。

### 4. ⚠️遗留问题与注意事项

- 当前问题已经修复，但仓库根路径递归扫描 `ext = lua` 时会覆盖 `output/`、`lua_packages/` 等生成目录与第三方目录，因此结果量会比较大；这不是本次报错的原因，只是当前扫描范围的自然结果。
- 真实 MCP 回归已验证：
  - 调用参数：
    - `path = D:\\projects\\vulcan-mcp-client`
    - `recursive = true`
    - `ext = lua`
  - 返回结果：
    - `files_scanned = 511`
    - `files_with_symbols = 222`
    - `items_found = 2518`
    - `errors = 0`
  - 未再出现 `malformed pattern` 相关错误。
