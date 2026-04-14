## 任务目标

将 `codeview_ast` 的匹配文件数量上限从 `500` 提升到 `5000`，以适配当前已经具备分页能力后的大结果场景，同时继续保留防止模型误扫超大目录的保护机制。

## 执行步骤

1. 定位 `codeview_ast` 当前匹配文件上限常量与报错文案。
2. 将上限由 `500` 调整为 `5000`。
3. 同步更新 `skill.json` 中的参数描述与 prompt 说明。
4. 同步运行时 `output/lua_skills/codeview_ast` 副本。
5. 做最小验证并补充执行变更总结后归档。

## 技术选型

- 仅调整 Lua 技能常量与技能元数据，不改分页核心逻辑。
- 保持现有分页方案不变，只扩大预扫描/匹配保护阈值。

## 验收标准

1. `codeview_ast` 的匹配文件硬上限变为 `5000`。
2. 超限报错文案与工具说明同步更新。
3. `runtime` 与 `output` 下副本保持一致。
4. 完成文档归档。

---

## 执行变更总结

### 1. 核心修复与调整概述

本次将 `codeview_ast` 的匹配文件保护上限从 `500` 提升到 `5000`，以适配当前已经具备分页返回能力后的大结果场景，同时继续保留对误扫超大目录的硬保护。

### 2. 📂文件变更清单

修改：
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\codeview_ast\skill.json`
- `D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ast\skill.json`
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-03-CODEVIEW_AST_MATCH_LIMIT_EXPANSION.md`

新增：
- 无

删除：
- 无

### 3. 💻关键代码调整详情

1. 在 `runtime/lua_skills/codeview_ast/main.lua` 中：
   - 将 `MAX_MATCHED_FILES` 从 `500` 调整为 `5000`
   - 将对应中文超限报错文案同步改为“超过5000个”

2. 在 `runtime/lua_skills/codeview_ast/skill.json` 中：
   - 将 `ext` 参数说明中的 `500-file` 调整为 `5000-file`
   - 将 prompt 中的 `500-file circuit breaker limit` 调整为 `5000-file circuit breaker limit`

3. 已同步 `output/lua_skills/codeview_ast` 运行时副本，保持与 `runtime` 下实现一致。

### 4. ⚠️遗留问题与注意事项

1. 本次只扩大了匹配文件保护阈值，没有修改分页核心逻辑。
2. 由于当前工具已经支持分页，因此 `5000` 更适合作为“防止误扫失控”的保护上限，而不是最终单次返回量。
3. 若当前运行中的 `vulcan-mcp` 进程已经加载过旧技能副本，仍建议重启服务以确保外部客户端立即看到最新元数据说明。
