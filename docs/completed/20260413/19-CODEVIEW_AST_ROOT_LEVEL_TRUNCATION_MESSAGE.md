# 任务目标

将 `codeview_ast` 当前写入每个文件 `content` 第一行的截断提示，改为英文版根节点提示字段 `!msg`，并且只有在成功命中到文件结果时才返回该提示；若没有搜索出文件，则不返回该提示。

# 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 中当前截断提示常量、文件内容渲染函数和最终返回对象构造逻辑。
2. 移除文件级 `content` 中的提示语注入，恢复文件内容为纯结构摘要。
3. 在最终返回根对象中增加英文 `!msg` 提示字段，并确保仅当 `file_results` 非空时才注入。
4. 同步更新 `output/lua_skills/codeview_ast/` 下的运行时副本，保持当前运行目录与源码目录一致。
5. 完成后补充执行变更总结，并将计划迁移到完成目录。

# 技术选型

- 继续保持现有 `file/language/line_count/content` 文件结果结构不变，只调整提示语承载位置。
- 使用根节点字符串字段 `!msg` 作为提示承载，避免在每个文件内容里重复注入相同文本。

# 验收标准

- 提示语改为英文。
- 提示语不再出现在每个文件 `content` 第一行。
- 返回结果根节点在有文件结果时新增 `!msg` 字段。
- 无文件结果时不返回 `!msg` 字段。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次将 `codeview_ast` 的长返回截断提示从“每个文件内容的第一行”调整为“根节点 `!msg` 提示字段”，并将提示语改为英文。调整后：

1. 文件级 `content` 恢复为纯结构摘要，不再重复携带相同提示语。
2. 只有在实际命中到文件结果时，根节点才会新增 `!msg` 字段。
3. 如果本次扫描没有搜索出任何文件结构结果，则不返回 `!msg`。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-19-CODEVIEW_AST_ROOT_LEVEL_TRUNCATION_MESSAGE.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/main.lua`

## 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/codeview_ast/main.lua` 中：
  - 将 `TRUNCATION_NOTICE` 改为英文提示语；
  - 调整 `build_file_content(...)`，去除文件级首行提示注入；
  - 调整最终返回对象构造逻辑，改为在 `#file_results > 0` 时注入 `result["!msg"] = TRUNCATION_NOTICE`。
- 同步更新 `output/lua_skills/codeview_ast/main.lua`，保证当前运行目录与源码目录一致。

## 4. ⚠️遗留问题与注意事项

1. 当前 `!msg` 是根节点字符串字段，便于调用方在顶层快速感知截断风险，但 JSON 对象字段顺序是否在所有消费端都严格保持“最前”仍取决于消费端实现。
2. 本地动态验证结果如下：
   - 命中结果时：`hitHasRootMsg = true`，且文件内容第一行已恢复为结构签名；
   - 无结果时：`missHasRootMsg = false`，`files_with_symbols = 0`。
