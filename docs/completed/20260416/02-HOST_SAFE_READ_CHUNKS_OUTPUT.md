# 任务目标

将 `vulcan-codekit` 的 overflow pointer 输出进一步调整为面向宿主 `Read(offset, limit)` 的精确制导格式。在超限场景下，直接返回可用于宿主读取工具的 `offset` 与 `limit`，同时保留 `start_line` 与 `end_line` 作为人工可读的行号锚点，避免调用侧再次做 0-based / 1-based 换算。

# 执行步骤

1. 检查当前 `shared_overflow.lua` 中 chunk 计划的字段结构与渲染格式。
2. 将 chunk 元数据扩展为同时输出 `offset`、`limit`、`start_line`、`end_line`。
3. 调整 pointer 渲染文本，把 `Safe Read Chunks` 收敛为面向宿主 Read 的 `Host Safe Read Chunks`。
4. 视情况保留 `bytes` 作为附加调试信息，但不作为调用主字段。
5. 更新文档说明，使 overflow 协议明确以宿主 `Read(offset, limit)` 为主。
6. 做至少一轮真实超限冒烟验证，确认输出可直接用于宿主读取调用。
7. 追加执行变更总结并归档计划文件。

# 技术选型

- 继续复用 `shared_overflow.lua` 作为统一 overflow 协议入口，不在各工具中重复实现。
- 以宿主 `Read(offset, limit)` 参数语义为准：`offset` 为 0-based 起始行，`limit` 为读取行数。
- `start_line/end_line` 保留作为阅读锚点，避免影响 LLM 和人工理解。

# 验收标准

- 超限 pointer 中直接包含 `offset`、`limit`、`start_line`、`end_line`。
- 输出字段可被宿主 `Read` 原样使用，无需额外换算。
- 文档中明确说明 `offset` 为 0-based，`limit` 为行数。
- 至少一轮超限真实输出验证通过。

# 执行变更总结

## 1. 核心修复与调整概述

- 将共享 overflow 协议中的 chunk 输出升级为宿主 `Read(offset, limit)` 可直接使用的格式。
- 超限 pointer 现在以 `Host Safe Read Chunks` 形式输出 `offset`、`limit`、`start_line`、`end_line`，同时保留 `bytes` 作为调试参考。
- 同步更新文档说明，明确 `offset` 是 0-based 起始行，`limit` 是读取行数。

## 2. 📂文件变更清单

### 修改

- `runtime/lua_skills/vulcan-codekit/shared_overflow.lua`
- `docs/lua_skills.md`

## 3. 💻关键代码调整详情

- 在 `shared_overflow.lua` 的 chunk 规划阶段，为每个 chunk 新增：
  - `offset = start_line - 1`
  - `limit = line_count`
- 将 pointer 渲染区块从 `Safe Read Chunks` 改为 `Host Safe Read Chunks`。
- 调整读取策略提示，明确建议优先直接使用 `offset + limit` 调宿主 `Read`。
- 保留 `start_line/end_line` 作为可读锚点，避免 0-based / 1-based 混淆。

## 4. ⚠️遗留问题与注意事项

- 本次仅调整 overflow pointer 的字段与渲染格式，不改变各工具正常未超限时的正文返回逻辑。
- 已做真实超限冒烟验证：
  - `codekit-ast-tree` 输出已包含 `read_XX: offset=..., limit=..., start_line=..., end_line=...`
  - `codekit-rg` 输出已包含同样格式
- 工作区中仍有与本任务无关的既有修改与未跟踪目录，本次未处理。
