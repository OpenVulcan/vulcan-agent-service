# 任务目标

调整 `codekit-markdown-menu` 工具的输出形式，使其直接返回纯文本而非结构体，并将文件扫描数量等统计信息前置到输出头部，作为独立节点展示，提升模型阅读与二次检索效率。

# 执行步骤

1. 审查 `codekit-markdown-menu` 当前实现、工具定义与文档说明，确认现有返回结构和统计信息位置。
2. 修改 Lua 实现，使工具直接返回 Markdown 风格纯文本，而不是 `content` 包裹结构。
3. 重组输出格式，将文件扫描数量、命中数量等统计信息前置到头部独立节点。
4. 同步更新 `skill.json` 与 `docs/lua_skills.md` 的说明，确保输出约定与实际行为一致。
5. 通过实际调用验证新输出格式可读性，并确认不再返回 JSON 包裹内容。

# 技术选型

- 保持 `codekit-markdown-menu` 的轻量目录导航定位，不引入缓存、导出或结构化 JSON 包装。
- 采用紧凑的 Markdown 文本格式表达头部统计与正文目录，兼顾可读性与上下文体积控制。
- 仅调整输出协议与说明，不扩大工具职责范围。

# 验收标准

- `codekit-markdown-menu` 直接返回纯文本，不再返回 `content` 结构体包装。
- 输出头部包含独立的扫描统计节点，并能清晰表达文件扫描数量等信息。
- 工具说明与实际输出一致。
- 计划文件完成执行总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `codekit-markdown-menu` 从结构体返回调整为直接返回 Markdown 纯文本，避免 `content` 包装和 JSON 转义噪音。
- 在正文头部新增独立的 `# SCAN SUMMARY` 节点，前置展示扫描文件数、包含标题的文件数、标题总数和错误数量。
- 同步更新工具定义与文档说明，确保对外协议、实现和使用建议保持一致。

## 2. 📂文件变更清单

- 修改：`runtime/lua_skills/vulcan-codekit/main_markdown_menu.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`docs/lua_skills.md`
- 修改：`docs/plan/20260415-19-MARKDOWN_MENU_TEXT_OUTPUT_HEADER.md`

## 3. 💻关键代码调整详情

- 重写 `build_markdown_menu_content` 的输出组织方式，使其接收统计信息并在头部生成稳定的扫描摘要节点。
- 删除 `codekit-markdown-menu` 末尾的结构体结果包装，改为直接返回纯文本。
- 将 `skill.json` 中该工具的 `return_type` 调整为 `string`，并更新 prompt，明确输出为直接返回的 Markdown 文本。
- 在 `docs/lua_skills.md` 中补充“头部统计节点 + 纯文本返回”的规范说明。

## 4. ⚠️遗留问题与注意事项

- 默认 `output/bin/vulcan-mcp.exe --call-tools` 仍会优先读取 `output/lua_skills` 下的构建副本；本次验证通过临时配置将 `lua_skills_override` 指向 `runtime/lua_skills`，确认了源码行为正确。
- 若后续需要在默认 `output` 路径下直接体验新行为，仍需执行一次构建或同步步骤，让运行时副本更新到最新源码。
