## 任务目标

调整 `vmcp-markdown-menu` 的行为，使其不再使用任何缓存、结果落盘或 Markdown 导出能力，只保留直接内联返回的轻量菜单输出。

## 详细执行步骤

1. 梳理 `vmcp-markdown-menu` 当前的参数、落盘逻辑和文档说明，确认需要移除的行为范围。
2. 删除 `workdir`、`export_md_path` 相关参数校验与结果处理逻辑。
3. 将工具行为改为始终直接返回内联结果，不再生成缓存文件、导出文件或相关提示消息。
4. 更新 `skill.json` 和文档说明，明确该工具只做内联菜单输出。
5. 通过 `--call-tools` 实际验证默认输出仍正确、且不再接受导出相关行为。

## 技术选型

- 保留 `vmcp-markdown-menu` 的 Markdown 文件收集与标题提取实现，不改其核心菜单模型。
- 仅移除大结果落盘、导出 Markdown 与相关参数处理，避免让工具职责扩张。
- 返回格式继续维持单个 `content` 文本块，确保与现有调用方式兼容。

## 验收标准

- `vmcp-markdown-menu` 不再声明 `workdir` 与 `export_md_path` 参数。
- 工具实现中不再写入缓存目录或导出 Markdown 文件。
- 调用结果始终为内联返回，不再附带落盘路径、导出路径或相关提示。
- 文档中明确说明该工具不做缓存、不做导出，只适合快速文档目录筛选。
- 计划文件最终补齐执行变更总结并归档到 `docs/completed/20260415/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vmcp-markdown-menu` 收敛为纯内联菜单工具，不再做缓存落盘、`workdir` 结果转存或 Markdown 导出。
- 同步移除了对外参数中的 `workdir` 与 `export_md_path`，并更新 prompt / 文档说明，明确这是一个“只返回内容、不写文件”的轻量筛选工具。
- 保留了原有 Markdown 文件收集、目录与文件混用、去重、标题提取与 fenced code block 过滤能力。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main_markdown_menu.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`

### 3. 💻关键代码调整详情

- 删除 `main_markdown_menu.lua` 中的 UTF-8 预览截断、大结果落盘、缓存路径解析、导出 Markdown 文件等整条结果后处理链路。
- 工具入口不再读取或校验 `workdir` / `export_md_path`，最终直接返回内联 `result`。
- `skill.json` 中删除了 `vmcp-markdown-menu` 的 `workdir` 和 `export_md_path` 参数定义，并将提示词改为显式声明 `NO CACHE / NO EXPORT`。
- `docs/lua_skills.md` 同步改为“不做缓存、不导出，只能通过缩小路径范围重新调用”的说明。

### 4. ⚠️遗留问题与注意事项

- 当前实现不会因为旧调用方仍传入 `workdir` 或 `export_md_path` 而报错，但这些字段已经被忽略，不会触发任何导出行为。
- 由于工具不再落盘，若客户端侧发生截断，只能根据 `# FILE MENU` 缩小路径范围再次调用，不能依赖文件导出结果做后续读取。
