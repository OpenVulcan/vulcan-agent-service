# 任务目标

为 `codeview_ast` 增加一个用于控制备注信息输出的参数，并默认启用该参数；同时优化结构摘要输出，使函数、方法、类等节点能够精准展示各自的备注信息，且备注文本本身不附带行号。

# 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 中现有备注提取、结构节点归一化和最终文本渲染流程，确认备注目前在哪个阶段被提取、在哪个阶段被丢弃。
2. 设计并实现新的备注开关参数，要求默认开启、参数校验清晰，并同步更新 `skill.json` 的参数定义、描述与 prompt。
3. 调整文件内容渲染逻辑，将备注信息按节点精准附着到函数、方法、类等结构项下，备注文本不显示行号，也不错误归属到相邻节点。
4. 完成后对照典型源码场景做本地验证，确认关闭参数时不输出备注、开启参数时输出稳定且结构可读。
5. 在计划文件末尾追加执行变更总结，并将计划迁移到完成目录。

# 技术选型

- 延续现有 `extract_leading_comment` / `extract_docstring` 的提取链路，避免重写 AST 扫描逻辑。
- 在结构渲染阶段按节点输出备注，以最小改动保留现有 `file/language/line_count/content` 返回格式。
- 继续使用 Lua 侧参数校验函数，保持错误返回风格与现有参数一致。

# 验收标准

- `codeview_ast` 新增备注控制参数，且默认值为启用。
- 开启时，函数、方法、类等节点下能看到对应备注文本，备注不携带行号。
- 关闭时，输出恢复为仅结构签名与行号，不包含备注文本。
- `skill.json` 的参数定义和提示词已同步更新。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次完成了 `codeview_ast` 的备注输出增强，实现了一个默认启用的 `comment` 布尔参数，用于控制是否在结构摘要中展示函数、方法、类等节点的备注信息。具体结果如下：

1. 新增 `comment` 参数校验逻辑，默认值为 `true`，仅在显式传入 `false` 时关闭备注输出。
2. 保持现有注释提取链路不变，继续复用节点级 `comment` 字段，避免影响 AST 扫描和结构归一化逻辑。
3. 调整结构文本渲染逻辑，在每个结构节点下追加无行号的 `note: ...` 备注行，使备注精准附着到对应类/函数节点下。
4. 同步更新 `skill.json` 参数定义与 prompt 文案，补充 `comment` 参数语义说明。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-16-CODEVIEW_AST_COMMENT_PARAMETER_AND_OUTPUT_ENHANCEMENT.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`

## 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/codeview_ast/main.lua` 中新增：
  - `format_symbol_comment_line(symbol, depth)`，专门生成无行号备注展示行；
  - `validate_comment_argument(value)`，校验新的备注开关参数。
- 调整 `append_symbol_outline(...)` 与 `build_file_content(...)`，使结构渲染阶段可以按参数决定是否输出备注。
- 在技能入口中追加 `comment` 参数解析，并将开关状态回传到最终结果对象中。
- 在 `runtime/lua_skills/codeview_ast/skill.json` 中增加 `comment` 参数定义，并同步更新 prompt 描述。
- 同步上述改动到 `output/lua_skills/codeview_ast/`，保证当前运行目录与源码目录保持一致。

## 4. ⚠️遗留问题与注意事项

1. 动态调用验证已确认：
   - 默认调用时，`content` 中会出现 `note:` 备注行；
   - 显式传入 `comment=false` 时，备注行会消失。
2. 当前运行中的 MCP 服务已经对 `main.lua` 热更新生效，但 `tools/list` 中的参数元数据仍是旧缓存，尚未显示新加的 `comment` 参数。
3. 若需要让 Inspector / Codex `/mcp` / `tools/list` 立即看到新的 `comment` 参数定义，需要重启当前 `vulcan-mcp` 服务，使其重新加载最新的 `skill.json` 元数据。
