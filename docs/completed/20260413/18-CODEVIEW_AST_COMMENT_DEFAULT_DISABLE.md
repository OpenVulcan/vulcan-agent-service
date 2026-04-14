# 任务目标

将 `codeview_ast` 的备注输出参数 `comment` 从“默认启用”调整为“默认关闭”，避免在未显式请求时把备注信息自动混入结构摘要。

# 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 中 `comment` 参数的校验默认值与最终返回字段。
2. 调整 `validate_comment_argument(...)` 的默认返回值，使备注默认关闭，仅在显式传入 `true` 时启用。
3. 同步更新 `runtime/lua_skills/codeview_ast/skill.json` 中的参数描述与 prompt 文案，确保对外说明一致。
4. 同步修改 `output/lua_skills/codeview_ast/` 下的运行时副本。
5. 完成后补充执行变更总结，并迁移计划文件到完成目录。

# 技术选型

- 仅调整参数默认值与文案，不改动现有备注提取和渲染逻辑。
- 保持 `comment` 参数的显式开关能力不变，兼容用户后续按需启用备注输出。

# 验收标准

- `comment` 参数默认值变为关闭。
- 未显式传参时，不再输出 `note:` 备注行。
- 显式传入 `comment=true` 时，备注输出仍能正常工作。
- `skill.json` 参数说明与 prompt 文案已同步反映“默认关闭”。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次将 `codeview_ast` 的备注输出参数 `comment` 从“默认启用”调整为“默认关闭”。调整后：

1. 不传 `comment` 时，结构摘要默认不再附带 `note:` 备注行。
2. 显式传入 `comment=true` 时，备注提取与渲染能力仍然保留并正常工作。
3. 参数说明与 prompt 文案已同步更新为“默认关闭”语义。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-18-CODEVIEW_AST_COMMENT_DEFAULT_DISABLE.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`

## 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/codeview_ast/main.lua` 中，将 `validate_comment_argument(value)` 的默认返回值从 `true` 调整为 `false`。
- 在 `runtime/lua_skills/codeview_ast/skill.json` 中：
  - 将 `comment` 参数描述从 `default: true` 改为 `default: false`；
  - 将 prompt 中“默认展示备注”的说明改为“默认关闭，可通过 `comment=true` 启用”。
- 同步更新 `output/lua_skills/codeview_ast/` 下对应文件，确保运行目录与源码目录一致。

## 4. ⚠️遗留问题与注意事项

1. 本次仅调整默认值，不影响备注提取实现本身，因此现有 `comment=true` 使用方式保持兼容。
2. 本地动态验证结果如下：
   - 默认调用时 `defaultHasNote = false`；
   - 显式 `comment=true` 调用时 `commentTrueHasNote = true`。
3. 如果外部 MCP 工具说明缓存了旧的 `skill.json` 元数据，可能仍需要重启服务后，参数说明界面才会刷新为最新描述。
