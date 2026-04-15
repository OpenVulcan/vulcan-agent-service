# 任务目标

调整 AST 工具的对外协议，使其收敛为面向显式文件列表的纯文本输出模式，不再依赖 JSON 结构返回，也不再承担目录扫描、递归遍历、扩展名过滤与 Markdown 导出等职责。

# 执行步骤

1. 审查 `codekit-ast`、`codekit-ast-tree` 当前的工具定义、Lua 入口与文档说明，确认现有职责边界和返回协议。
2. 按新约束调整目标 AST 工具的参数与实现：
   - 仅支持显式文件路径输入
   - 返回纯文本字符串
   - 移除目录扫描能力
   - 移除 `recursive`、`ext`、`export_md_path` 等无关参数
3. 同步更新 `skill.json`、`docs/lua_skills.md` 及必要提示文案，确保工具描述与实际行为一致。
4. 通过本地工具调用验证新工具的输入限制与纯文本输出效果。
5. 补充执行变更总结并归档计划文件。

# 技术选型

- 以“单次输入明确文件列表、单次返回纯文本结构结果”为核心收敛方向，减少隐式分支和 JSON 包装噪音。
- 优先复用现有 AST 提取能力，只调整工具协议与结果组织方式，不无谓扩散实现复杂度。
- 严格限制输入形态，避免目录扫描继续带来大盘输出与参数歧义。

# 验收标准

- 目标 AST 工具只接受显式文件路径，不再接受目录。
- 工具返回纯文本字符串，不再返回 JSON/table 结构。
- `recursive`、`ext`、`export_md_path` 等参数已从对外协议中移除。
- 文档与提示说明完成同步更新。
- 计划文件完成执行总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `codekit-ast-tree` 收敛为“显式文件列表输入 + 纯文本 AST 结构树输出”的单一职责工具。
- 参数入口从 `path` 调整为 `paths`，并取消目录扫描、递归、扩展名过滤、忽略规则与 Markdown 导出等外围能力。
- 输出改为稳定的 Markdown 文本：头部先给出 AST 摘要，再逐文件输出结构树正文，避免 JSON 包装噪音。

## 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`docs/lua_skills.md`
- 修改：`docs/plan/20260415-20-AST_TOOL_FILE_ONLY_STRING_OUTPUT.md`

## 3. 💻关键代码调整详情

- 重写 `main_ast_tree.lua`，仅保留文件列表校验、AST 扫描、纯文本渲染与超限落盘能力。
- 新增 `paths` 参数校验逻辑，支持多行文件路径列表，并限制单次最多 20 个显式文件。
- 复用主 `codekit-ast` 的建树与文本渲染 helper，但不再暴露目录扫描相关参数。
- 同步调整 `skill.json` 的参数定义、工具描述与 prompt，使其明确为文件型纯文本工具。
- 更新 `docs/lua_skills.md`，将 `codekit-ast-tree` 的使用说明改为文件级结构树模式。

## 4. ⚠️遗留问题与注意事项

- 当前 `codekit-ast-tree` 仍保留与主 AST 工具一致的超限落盘逻辑；若后续希望连缓存备注也进一步收紧，可在此基础上继续裁剪。
- 默认 `output/bin/vulcan-mcp.exe --call-tools` 会优先读取构建副本，本次验证通过临时配置把 `lua_skills_override` 指向 `runtime/lua_skills`，确认了源码行为正确。
