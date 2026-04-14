# 任务目标

提升 `codeview_ast` 目录扫描时的最大匹配文件上限，并在每个文件输出内容的第一行固定增加 MCP 长内容截断提示，帮助调用方在遇到返回内容被截断时主动继续获取完整信息。

# 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 中的文件数量上限常量、超限错误文案，以及文件内容渲染函数入口。
2. 将最大匹配文件上限从 200 提升到 500，并同步调整相关错误提示和对外 prompt 描述。
3. 在最终 `content` 文本生成函数中，把用户指定的提示语固定放在第一行，确保每个文件结果都以该提示开头。
4. 同步更新 `output/lua_skills/codeview_ast/` 下的对应文件，保持运行目录与源码目录一致。
5. 完成后补充执行变更总结，并将计划迁移到完成目录。

# 技术选型

- 直接复用现有 `MAX_MATCHED_FILES` 常量和 `build_file_content(...)` 渲染入口，以最小改动实现能力增强。
- 继续保持当前 `file/language/line_count/content` 返回格式，不改动外层 JSON 结构，只增强 `content` 首行说明。

# 验收标准

- 目录扫描超限阈值从 200 提升到 500。
- 超限报错文案与 `skill.json` prompt 中的数量说明同步更新为 500。
- 每个文件的 `content` 第一行固定为用户要求的截断提示语。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次完成了两个直接影响 `codeview_ast` 可用性的调整：

1. 将目录扫描最大匹配文件数上限从 200 提升到 500，缓解中大型仓库扫描时过早触发上限的问题。
2. 在每个文件结果的 `content` 第一行固定增加截断提醒，提示调用方在遇到长 MCP 返回内容被开发软件截断时，需要主动继续获取完整信息。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-17-CODEVIEW_AST_FILE_LIMIT_AND_TRUNCATION_NOTICE.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`

## 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/codeview_ast/main.lua` 中：
  - 将 `MAX_MATCHED_FILES` 从 `200` 提升为 `500`；
  - 将超限错误文案同步改为“超过500个”；
  - 新增 `TRUNCATION_NOTICE` 常量；
  - 调整 `build_file_content(...)`，使最终 `content` 的第一行固定为该提示语。
- 在 `runtime/lua_skills/codeview_ast/skill.json` 中，将 prompt 内的文件数量上限说明从 200 更新为 500。
- 同步上述改动到 `output/lua_skills/codeview_ast/`，保证当前运行目录与源码目录一致。

## 4. ⚠️遗留问题与注意事项

1. 当前提示语是按“每个文件结果的 `content` 第一行”注入的，因此同一次调用返回多个文件时，每个文件都会各自带一遍提示语。
2. 已通过本地动态调用验证：
   - `content` 第一行确实为指定提示语；
   - 第二行开始才是实际的结构摘要内容。
3. `skill.json` 的 prompt 文案已更新；若外部工具缓存了旧的 MCP tool 元数据，可能仍需要重启服务后才能在工具说明里看到最新文案。
