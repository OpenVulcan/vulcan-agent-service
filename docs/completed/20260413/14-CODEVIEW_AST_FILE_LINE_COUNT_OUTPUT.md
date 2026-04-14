# 任务目标

为 `codeview_ast` 的每个文件级返回结果增加“文件总行数”字段，让 AI 在读取结构摘要时，可以同时知道该文件整体长度，辅助判断是局部修改还是需要进一步阅读全文。

# 执行步骤

1. 审查当前 `codeview_ast` 文件结果构建位置，确认最稳妥的总行数来源。
2. 在文件结果生成阶段补充总行数字段，并尽量复用现有文件缓存，避免重复读取。
3. 更新 `skill.json` 的提示描述，明确返回结果包含文件总行数。
4. 构建并通过真实 MCP 调用验证返回结构中已包含总行数字段。

# 技术选型

- 复用现有 `read_file_state(...)` 与 `FILE_CACHE`，直接使用其 `line_count` 作为文件总行数来源。
- 字段保持在文件级结果对象内输出，不改动结构树与符号提取逻辑，尽量降低回归风险。
- 若文件状态读取失败，则保守返回 `0`，避免因单个文件元信息缺失而破坏整体结构输出。

# 验收标准

1. 每个 `files` 项都新增稳定的总行数字段。
2. 现有 `file/language/content` 结构保持兼容，不影响既有消费方式。
3. 构建通过，真实 MCP 调用中能看到新增字段。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 为 `codeview_ast` 的每个文件级返回结果新增 `line_count` 字段，用于表示该文件的总行数。
- 总行数读取直接复用现有 `read_file_state(...)` 与 `FILE_CACHE`，避免再引入额外文件读取链路。
- 同步更新 `skill.json` 描述，使外部调用方能够明确知道返回结构已包含 `line_count`。

## 2. 📂文件变更清单

- 新增：`docs/plan/20260413-14-CODEVIEW_AST_FILE_LINE_COUNT_OUTPUT.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`

## 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/codeview_ast/main.lua` 中新增 `get_file_line_count(file_path)`，通过缓存化文件状态返回总行数；若读取失败则回退为 `0`。
- 在文件结果组装阶段，为每个返回文件对象追加 `line_count = get_file_line_count(file_info.path)`。
- 在 `runtime/lua_skills/codeview_ast/skill.json` 中同步更新 `description` 与 `prompt`，将返回结构从 `file/language/content` 扩展为 `file/language/line_count/content`。

## 4. ⚠️遗留问题与注意事项

- `line_count` 是基于当前磁盘文件内容计算的总行数，不是结构节点覆盖的行数。
- 如果某个文件在极端情况下读取失败，当前实现会保守返回 `0`，以保证整体结构结果仍然可用。
- 实测验证结果：
  - `scripts/build.ps1` 构建成功。
  - 真实 MCP 调用 `codeview_ast(path = D:\\projects\\vulcan-mcp-client\\src\\server.rs)` 时，返回 `files_scanned = 1`、`files_with_symbols = 1`，且首个文件结果包含 `line_count = 1359`、`language = rust`。
