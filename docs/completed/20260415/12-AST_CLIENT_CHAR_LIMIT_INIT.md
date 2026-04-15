# 任务目标

为 `vulcan-codekit` 的 AST 处理增加基于 MCP 客户端名称的字符上限初始化判断逻辑，仅完成“客户端识别与默认限制映射”，暂不实现超限后的具体处理策略。

# 执行步骤

1. 定位 AST 工具当前长度相关配置与 Lua 侧可见客户端上下文入口。
2. 按客户端名称补充统一的字符限制判定逻辑，并在 AST 运行时初始化阶段完成映射。
3. 保持当前行为兼容：未命中指定客户端时使用统一默认值。
4. 进行最小必要验证，确认映射逻辑可运行且不影响现有 AST 主流程。
5. 补充执行变更总结，并按规范归档到 `docs/completed/20260415/`。

# 技术选型

- 复用现有 `vulcan.client_info` / `vulcan.context` 上下文入口，不额外引入新的协议字段。
- 仅新增“客户端名称归一化 + 默认字符上限选择”逻辑，不提前绑定后续的超限处理策略。
- 优先在 Lua AST 运行时侧完成判断，便于后续直接衔接不同客户端的截断/导出策略。

# 验收标准

1. AST 工具可根据客户端名称得到对应字符上限初始化值。
2. 映射规则满足以下要求：
   - 包含 `qwen` 或名称以 `qwen-code-mcp-client` 开头时为 `25000`
   - `codex-mcp-client` 为 `10000`
   - `opencode` 为 `50000`
   - `claude-code` 为 `50000`
   - 其它客户端统一为 `10000`
3. 当前改动仅完成判断与初始化，不引入额外超限处理分支。
4. 计划文件在任务完成后补齐执行变更总结，并归档到 `docs/completed/20260415/`。

---

# 执行变更总结

## 1. 核心修复与调整概述

已为 `codekit-ast` 增加“客户端名称 -> AST 字符上限”的初始化映射逻辑。工具在每次调用开始时会先读取 Lua 可见的客户端信息，并初始化当前请求的 AST 字符预算，但暂不对超限结果做截断、导出或分支处理。

## 2. 📂文件变更清单

修改：

- `runtime/lua_skills/vulcan-codekit/main.lua`
- `docs/plan/20260415-12-AST_CLIENT_CHAR_LIMIT_INIT.md`

新增：

- 无

删除：

- 无

## 3. 💻关键代码调整详情

- 在 `main.lua` 中新增 `DEFAULT_AST_CLIENT_CHAR_LIMIT` 与 `CURRENT_AST_CLIENT_CHAR_LIMIT`，作为当前请求的 AST 字符预算初始化值。
- 新增 `resolve_current_client_name`，从 `vulcan.client_info` 或 `vulcan.context.client_info` 中提取客户端名称，并统一做小写归一化。
- 新增 `resolve_ast_client_char_limit`，按如下规则映射：
  - 名称以 `qwen-code-mcp-client` 开头，或包含 `qwen`：`25000`
  - 名称为 `codex-mcp-client`：`10000`
  - 名称包含 `opencode`：`50000`
  - 名称包含 `claude-code`：`50000`
  - 其它客户端：`10000`
- 在工具入口处新增 `initialize_ast_client_char_limit()` 调用，确保每次请求开始时先完成判定初始化。

## 4. ⚠️遗留问题与注意事项

- 本次仅完成“判定初始化”，尚未把该字符上限真正用于 AST 结果的截断、导出或回退策略。
- 当前逻辑依赖 Lua 侧可见的客户端上下文；若当前调用没有客户端信息，则会回退到默认值 `10000`。
- 本次验证使用 `python scripts/verify_vmcp_ast_comment_notes.py`，确认初始化新增逻辑未破坏现有 AST 备注回归。
