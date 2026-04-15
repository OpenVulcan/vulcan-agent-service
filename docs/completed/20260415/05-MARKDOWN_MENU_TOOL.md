## 任务目标

在 `ast-grep` Lua skill 工具组中新增一个面向 Markdown 文档目录浏览的 `markdown_menu` 工具，用于按目录/文件集合提取 `.md` 文件的标题目录树，并以适合大规模文档筛选的单段 `content` 输出返回。

## 详细执行步骤

1. 梳理 `ast-grep` 现有工具组的 `skill.json` 组织方式、参数风格与大结果处理规则。
2. 设计 `markdown_menu` 的输入参数与输出格式，支持目录、文件、多路径组合、去重与是否递归。
3. 在 `runtime/lua_skills/ast-grep` 中新增 Lua 入口，实现 Markdown 文件收集、标题解析、目录输出与大结果处理。
4. 更新 `skill.json` 与文档说明，明确工具适用场景、截断后的再次调用建议与推荐使用方式。
5. 通过 `--call-tools` 实际验证多目录/多文件组合、递归开关、标题行号与输出内容格式。

## 技术选型

- 复用 `ast-grep` 工具组现有的路径解析、忽略规则与大结果落盘策略，保持参数体验一致。
- Markdown 解析仅做轻量标题提取，严格限制在 `#`、`##`、`###` 目录节点，不尝试做正文摘要或复杂 Markdown 语义分析。
- 输出统一为适合快速筛选的单个 `content` 文本块，便于模型在截断后根据文件目录重新精确调用。

## 验收标准

- 支持目录、文件、多路径组合输入，并对重复 Markdown 文件去重。
- 支持递归开关，仅扫描 `.md` 文件。
- 输出中包含文件菜单与每个 Markdown 文件对应的标题目录树，并带行号。
- 当结果过大时，遵循现有大结果导出/提示规则，不破坏工具组一致性。
- 文档中明确说明该工具适合文档筛选与导航，不适合正文分析。
- 计划文件最终补齐执行变更总结并归档到 `docs/completed/20260415/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 在 `ast-grep` 工具组中新增 `vmcp-markdown-menu`，用于按目录、文件或混合路径集合提取 Markdown 文档的 `#`、`##`、`###` 标题目录。
- 输出格式固定为单个 `content` 文本块，前半部分是 `# FILE MENU`，后半部分是逐文件的标题目录详情，方便结果截断后按文件菜单做二次精确调用。
- 新工具支持多路径输入、目录与文件混用、Markdown 文件去重、递归开关，以及和现有工具一致的 `workdir` / `export_md_path` 大结果处理行为。
- 目录扫描改为复用 skill 自带 `rg` 依赖做 `.md` 文件收集，从而继承高性能目录遍历和忽略规则体验；Markdown 内容解析仅提取标题，不读取正文语义。

### 2. 📂文件变更清单

- 新增：`runtime/lua_skills/ast-grep/main_markdown_menu.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`
- 新增：`testdata/markdown_menu/guide_root.md`
- 新增：`testdata/markdown_menu/notes.md`
- 新增：`testdata/markdown_menu/nested/guide_nested.md`

### 3. 💻关键代码调整详情

- 新增 Markdown 专用入口，复用 `vmcp-ast` 的参数校验函数，并在新文件中补齐路径解析、UTF-8 安全截断、导出落盘和结果预览逻辑。
- 标题提取仅识别 `#`、`##`、`###`，并显式跳过 fenced code block，避免把代码样例里的井号误判成文档目录。
- 目录模式通过 `rg --files <dir> -g *.md` 收集 Markdown 文件；`recursive=false` 时附加 `--max-depth 1`，`ignore=false` 时附加 `--no-ignore --hidden`。
- 输出目录详情时为每个文件保留稳定编号和绝对路径，使 `# FILE MENU` 与 `# Markdown Details` 可以直接一一对应。

### 4. ⚠️遗留问题与注意事项

- `vmcp-markdown-menu` 只处理 `.md` 扩展名，不覆盖 `.markdown` 或其他文档格式，这是有意收紧范围后的设计选择。
- 当前工具只提取 ATX 标题（`#` 风格），不处理 Setext 标题，这样更利于保持输出稳定和实现简单。
- 验证时使用了 `output/bin/vulcan-mcp` 配合临时配置将 `lua_skills_override` 指向 `runtime/lua_skills`，以确保测试的是当前源码而不是旧输出副本。
