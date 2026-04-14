## 任务目标

将 `codeview_ast` 中所有对外提示性质的信息统一调整为英文，确保技能元数据、报错提示与分页/缓存相关说明对外一致使用英文表达。

## 执行步骤

1. 扫描 `codeview_ast` 技能元数据与 Lua 实现中的对外可见文本。
2. 将中文提示、报错与说明文案统一改为英文。
3. 同步 `output/lua_skills/codeview_ast` 运行时副本。
4. 做最小核对并补充执行变更总结后归档。

## 技术选型

- 仅调整对外提示文本，不改动分页与缓存逻辑。
- 保留代码内部双语注释，不把内部注释误改为英文单语。

## 验收标准

1. `codeview_ast` 对外提示文本统一为英文。
2. `runtime` 与 `output` 副本保持一致。
3. 完成文档归档。

---

## 执行变更总结

### 1. 核心修复与调整概述

本次将 `codeview_ast` 中剩余的对外中文提示文本统一改为英文，确保技能元数据与结构化错误提示对外保持英文一致性。

### 2. 📂文件变更清单

修改：
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-04-CODEVIEW_AST_ENGLISH_PROMPTS.md`

新增：
- 无

删除：
- 无

### 3. 💻关键代码调整详情

1. 将 `too_many_matched_files` 的超限提示从中文改为英文：
   - `Matched files exceed 5000. Narrow the path scope or provide a more specific file list.`
2. 同步了 `output/lua_skills/codeview_ast/main.lua` 运行时副本。
3. 通过正则扫描确认 `message = "..."` 这类对外结构化错误文本中已无中文残留。

### 4. ⚠️遗留问题与注意事项

1. 本次只调整对外提示文本，没有改动分页、缓存和匹配逻辑。
2. 代码内部双语注释仍然保留，这是文档规范的一部分，不属于需要改成英文的“对外提示信息”。
