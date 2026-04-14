## 任务目标

将当前 `codeview_ast` skill 统一重命名为更符合实际用途的 `ast-grep`，并将对外常规工具名称调整为 `vmcp-ast`，同时同步修正目录、配置、Lua 模块名、文档、资源 URI、提示词名称与运行时副本，确保后续基于该 skill 继续扩展其他查询工具时命名体系清晰一致。

## 执行步骤

1. 梳理 `codeview_ast` 在源码、运行时目录、文档和计划记录中的关键引用位置，区分必须同步与可保留历史记录的内容。
2. 重命名 `runtime/lua_skills/codeview_ast` 与 `output/lua_skills/codeview_ast` 目录为 `ast-grep`。
3. 调整 skill 元数据：
   - skill 内部名称改为 `ast-grep`
   - tool 名改为 `vmcp-ast`
   - Lua 模块名改为稳定可注册形式
   - 资源 URI、提示词名称与描述同步更新
4. 调整 Lua 实现内的模块变量、缓存命名空间与相关提示文本。
5. 同步更新技能开发文档中的示例与名称引用。
6. 完成构建验证，并补写执行变更总结后归档。

## 技术选型

- 对外工具名使用 `vmcp-ast`，保持“Vulcan MCP AST 工具”语义直观。
- skill 目录名使用 `ast-grep`，与底层依赖及主要能力来源保持一致。
- Lua 模块名使用安全的下划线形式，避免目录名中的连字符影响全局变量注册。
- 历史计划文档中的既有记录保留原文，不追溯重写，仅修正当前生效实现与开发文档。

## 验收标准

1. `runtime/lua_skills/ast-grep` 与 `output/lua_skills/ast-grep` 成为新的运行目录。
2. Skill 对外工具名变为 `vmcp-ast`，并能被运行时识别。
3. 资源 URI、提示词名称、Lua 模块引用与缓存命名空间已同步更新。
4. `cargo build` 通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将原 `codeview_ast` skill 目录统一重命名为 `ast-grep`，使目录命名与底层依赖和实际用途保持一致。
- 已将对外常规工具名称从 `codeview_ast` 调整为 `vmcp-ast`，并同步修正资源 URI、提示词名称、Lua 模块名与缓存命名空间。
- 已同步更新开发文档中的示例调用与热重载日志样例，并刷新 `output/lua_skills/ast-grep` 运行时副本。

### 2. 📂文件变更清单

- 目录重命名：`runtime/lua_skills/codeview_ast` -> `runtime/lua_skills/ast-grep`
- 目录重命名：`output/lua_skills/codeview_ast` -> `output/lua_skills/ast-grep`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`runtime/lua_skills/ast-grep/main.lua`
- 修改：`runtime/lua_skills/ast-grep/resources/guide.md`
- 修改：`runtime/lua_skills/ast-grep/templates/example.md`
- 修改：`runtime/lua_skills/ast-grep/templates/example_generator.lua`
- 修改：`runtime/lua_skills/ast-grep/prompts/first_pass.md`
- 修改：`docs/lua_skills.md`
- 修改：`src/lua_skill.rs`
- 新增：`docs/plan/20260414-08-AST_GREP_SKILL_RENAME_AND_TOOL_ALIAS.md`

### 3. 💻关键代码调整详情

- 在 `skill.json` 中：
  - skill 内部名称改为 `ast-grep`
  - tool 名改为 `vmcp-ast`
  - Lua 模块名改为 `ast_grep`
  - 资源 URI 改为 `skill://ast-grep/...`
  - 提示词名改为 `vmcp_ast_first_pass`
- 在 `main.lua` 中：
  - `TOOL_CACHE_NAMESPACE` 改为 `vmcp-ast`
  - `__skill_dir_codeview_ast` 改为 `__skill_dir_ast_grep`
- 在文档中同步修正 `vulcan.call("vmcp-ast", ...)` 与目录样例。

### 4. ⚠️遗留问题与注意事项

- 历史计划与已归档文档仍会保留旧名称 `codeview_ast`，这些属于历史记录，不做追溯改写。
- 如果后续基于该 skill 再拆分新的查询工具，建议继续沿用 `ast-grep` 目录作为能力根目录，将具体查询能力按新的 tool 名扩展到同一个 skill 下。
