# 任务目标

为 `vulcan-codekit` 的公共 prompt 增加 `task` 参数，并基于新的最佳实践草案重构 prompt 内容：既保留主代理先建立全局观的核心原则，也把 `task` 作为“用户当前指令”附加到提示词末尾，提升支持 MCP prompts 的客户端在实际调用时的上下文对齐能力。

# 执行步骤

1. 审阅当前 `skill.json` 中 prompt 注册方式，以及静态 prompt 文件在宿主中的读取/参数处理机制。
2. 判断静态 prompt 是否支持参数注入；若不支持，则按宿主实际机制选择最稳妥的实现方式（例如改为动态 `.lua` prompt 生成器）。
3. 为 `vulcan_codekite_skill` 增加 `task` 参数，并重写 prompt 内容，融合新的最佳实践草案。
4. 确保 prompt 末尾能附加“用户当前指令”内容，并保持公共 prompt 仍适合跨客户端消费。
5. 做必要校验，确认 `skill.json` 可解析、prompt 参数定义正确、文件内容落地无误。
6. 追加执行变更总结并归档计划文件。

# 技术选型

- 优先遵守宿主当前对 prompts 的真实支持方式，不假设静态 Markdown 会自动模板替换。
- 若需要参数注入，优先改为 `.lua` prompt 生成器，以稳定输出最终 prompt 文本。
- 保留公共 prompt 的跨客户端通用性，同时将 `task` 作为上下文收口点，显式告诉模型“这是用户当前指令”。

# 验收标准

- `vulcan_codekite_skill` 已新增 `task` 参数。
- prompt 内容已融合新的最佳实践草案，并显式强调主代理先用 `codekit-ast-tree` 建图。
- prompt 末尾能附加 `task` 作为“用户当前指令”。
- `skill.json` 解析通过，且 prompt 配置与宿主机制匹配。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `vulcan_codekite_skill` 从静态 Markdown prompt 改为动态 `.lua` prompt 生成器，解决了静态文件无法注入参数的问题。
- 为该 prompt 增加了 `task` 参数，并在末尾追加为“用户当前指令”段落。
- 当 `task` 未传、为空或只有空白字符时，默认使用：
  - `分析当前项目，快速了解项目结构，为用户下一步指令做准备`
- 同步把新的最佳实践草案融入公共 prompt，并补充到本地 `vulcan-codekite` skill 中关于主代理先建图、再决定是否派子代理的规则。

## 2. 📂文件变更清单

### 修改文件

- `runtime/lua_skills/vulcan-codekit/skill.json`
- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`
- `C:\Users\20000\.codex\skills\vulcan-codekite\SKILL.md`
- `docs/plan/20260415-33-CODEKIT_PROMPT_TASK_ARGUMENT_AND_WORKFLOW_REWRITE.md`（后续已归档）

### 删除文件

- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.md`

### 新增文件

- `runtime/lua_skills/vulcan-codekit/prompts/vulcan_codekite_skill.lua`

## 3. 💻关键代码调整详情

- `skill.json`
  - 将 `vulcan_codekite_skill` 的 `file` 从静态 `.md` 改为 `.lua`
  - 新增 `task` 参数定义，并说明该参数会作为用户当前指令附加到提示词末尾
- `vulcan_codekite_skill.lua`
  - 读取 `args.arguments.task`
  - 自动裁剪前后空白
  - 若为空，则回退到默认任务文案
  - 输出包含：
    - 核心原则
    - 快速决策树
    - 工具调用顺序
    - 主代理与子代理
    - 常见循环模式
    - 安全护栏
    - 失败回退
    - 用户当前指令
- 本地 `vulcan-codekite` skill
  - 补强了主代理先建立认知地图，再决定是否需要子代理的工作原则
  - 顺手修复了导致 `quick_validate.py` 在 Windows 默认编码下失败的弯引号字符

## 4. ⚠️遗留问题与注意事项

- 这次只完成了配置解析与本地 skill 校验，没有额外跑 MCP `prompts/get` 端到端调用。
- 由于公共 prompt 已经改成 `.lua` 生成器，后续如果再扩参数，应继续沿用生成器方式，而不要退回静态 Markdown。
- 当前默认任务文案适合“未提供 task 时的兜底探索”；如果后续你希望按客户端类型给不同默认任务，也可以在这个生成器里继续细化。
