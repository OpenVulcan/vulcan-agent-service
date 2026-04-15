# 任务目标

将 `codekit-rg` 的默认返回协议调整为 Markdown 纯文本输出，减少 JSON 包装和转义噪音，提升模型直接阅读与继续分析的效率；同时保留 `export_md_path` 的显式导出能力与大结果超限处理能力。

# 执行步骤

1. 审查 `codekit-rg` 当前返回流程，包括 `main_rg.lua`、`skill.json`、`docs/lua_skills.md` 与宿主输出链路。
2. 将 `codekit-rg` 默认返回从结构化 table 调整为 Markdown 纯文本，确保普通调用直接得到可读内容。
3. 保留 `export_md_path` 的静默导出行为，并保证大结果超限时仍能写入缓存并返回带路径提示的正文。
4. 同步更新 `skill.json` 的 `return_type`、工具说明与开发文档中的使用描述。
5. 使用本地 `--call-tools` 对普通命中、无命中、导出模式进行回归验证。
6. 补充执行变更总结并在完成后归档计划文件。

# 技术选型

- 沿用当前已经在 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-markdown-menu` 上使用的“纯文本优先”返回策略。
- 保留 `codekit-rg` 内部的 Markdown 渲染逻辑作为主输出，不再默认走 JSON table 内联。
- 显式导出场景继续以“仅返回生成提示与绝对路径”的静默模式处理，避免重复内联正文。

# 验收标准

- `codekit-rg` 默认返回 Markdown 纯文本，而不是 JSON 包装结构。
- `codekit-rg` 不再支持 `export_md_path`，并会对该参数返回明确错误。
- 超限缓存逻辑仍可用，并以纯文本提示形式呈现。
- `skill.json` 与 `docs/lua_skills.md` 已同步更新。
- 计划文件完成执行总结并归档。

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `codekit-rg` 的默认返回从 JSON/table 收敛为 Markdown 纯文本，输出头部改为摘要区，文件正文改为 `## FILE ...` 区块，便于模型直接阅读与继续分析。
- 取消 `codekit-rg` 的 `export_md_path` 能力，并增加显式拒绝逻辑，避免调用方误以为仍可导出到指定目录。
- 新增共享长度规则模块 `shared_length.lua`，让 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-rg` 统一复用同一套客户端字符预算映射与超限口径。
- 同步更新 `skill.json` 与开发文档中的返回类型、使用说明和大结果规则描述，消除旧的 JSON 输出心智模型。

## 2. 📂文件变更清单

- 新增：`runtime/lua_skills/vulcan-codekit/shared_length.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/main_rg.lua`
- 修改：`runtime/lua_skills/vulcan-codekit/skill.json`
- 修改：`docs/lua_skills.md`

## 3. 💻关键代码调整详情

- 在 `shared_length.lua` 中抽离客户端长度预算规则，统一维护 `qwen/codex/opencode/claude-code/默认` 的字符限制映射。
- 在 `main.lua` 与 `main_ast_tree.lua` 中接入共享长度模块，替换原本各自维护的字符预算初始化逻辑。
- 在 `main_rg.lua` 中取消 JSON 编码大小判断，改为直接按 Markdown 纯文本长度与共享字符预算决定是否超限落盘。
- 在 `main_rg.lua` 中新增 `validate_export_md_absence(...)`，对 `export_md_path` 进行明确拒绝，而不是静默忽略。
- 在 `skill.json` 中把 `codekit-rg` 的 `return_type` 改为 `string`，并移除 `export_md_path` 参数定义。
- 在 `docs/lua_skills.md` 中更新 `codekit-rg` 与统一长度规则的对外说明，强调纯文本返回和公共长度模块策略。

## 4. ⚠️遗留问题与注意事项

- 本次没有处理工作区里与本任务无关的其他既有变更，它们仍保留在当前工作区状态中。
- `codekit-rg` 取消 `export_md_path` 后，超限结果只能依赖自动缓存路径提示；如果后续需要人工导出文件，应通过缓存文件或另行设计专门导出工具处理。
- 共享长度模块当前只负责客户端字符预算，不负责缓存目录创建、写盘策略等其余行为；这些逻辑仍分别保留在各个工具入口内部。
