# 任务目标

排查当前仓库对应的自定义 MCP 服务为什么已经在 Codex 侧完成连接或重启，但当前对话会话仍无法枚举到预期的 MCP 工具（例如 `codeview_ast`），并给出可复现的原因定位与修复建议。

# 执行步骤

1. 盘点仓库内与 MCP 服务注册、启动、暴露工具相关的配置文件、脚本与源码入口，确认 `codeview_ast` 等工具的实际定义位置。
2. 检查服务端能力声明，区分当前实现究竟暴露的是 tools、resources 还是 templates，确认是否存在“客户端已连接但资源枚举为空”的预期差异。
3. 检查与 Codex/Inspector 连接相关的启动参数、传输方式、日志输出和重连逻辑，定位服务重启后能力未注入当前会话的可能原因。
4. 如存在仓库内可直接修复的问题，则在不引入高风险架构调整的前提下完成修复；如问题位于外部客户端注入链路，则整理证据与后续排查建议。
5. 完成后逐项对照计划进行验证，并在文末补充执行变更总结，再将计划迁移至完成目录。

# 技术选型

- 使用 PowerShell 与 `rg` 检索仓库内配置、脚本与源码，优先基于本地事实定位问题。
- 使用现有 Codex 会话可访问的 MCP 枚举接口交叉验证“资源可见性”和“工具可见性”的差别。
- 如需修改仓库文件，优先保持最小改动，确保排查结论能够被后续复现。

# 验收标准

- 能明确说明当前 `codeview_ast` 不可见的原因属于服务端未暴露、客户端未注入，还是当前会话未刷新。
- 给出至少一条可操作的验证路径，帮助用户复测修复结果。
- 若产生代码或文档改动，改动内容与排查结论保持一致，且计划文件已完整记录执行结果。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次未修改运行时代码，重点完成了对“Codex 已配置 `vulcanmcp`，但当前会话仍看不到 `codeview_ast`”问题的本地证据排查与归因确认。结论如下：

1. `vulcanmcp` 已经写入当前用户的 Codex 配置文件 `C:\Users\20000\.codex\config.toml`，配置项为：
   - `[mcp_servers.vulcanmcp]`
   - `enabled = true`
   - `url = "http://127.0.0.1:19201/mcp"`
2. 仓库服务端本身具备注册 `codeview_ast` 的能力，且 `runtime/lua_skills/codeview_ast/skill.json` 中 `tool_name` 明确为 `codeview_ast`；结合用户说明“Inspector 可正常连接和调用”，可以排除“服务端未暴露工具”的主因。
3. 当前对话会话中，模型侧只能看到 `notion` 与 `playwright`，并且 `2026-04-13` 的本地 session 记录也只体现了这两个 MCP 的可见性，没有出现 `vulcanmcp` 被注入当前线程上下文的证据。
4. 旧日志 `C:\Users\20000\.codex\log\codex-tui.log` 中多次出现 `session_init.enabled_mcp_server_count=2`，与当前 `config.toml` 里应为 3 个 MCP（`notion`、`playwright`、`vulcanmcp`）不一致，说明至少存在一次“客户端会话启动时未加载到最新 MCP 配置”的情况。
5. 结合 `codex-tui.log` 的最后更新时间仍停留在 `2026-04-10`，而 `2026-04-13` 的 session 文件已存在，可以进一步判断：当前桌面端活跃会话与我能直接读取到的历史日志/配置加载路径之间存在断层，问题更偏向 Codex 客户端的配置重载、会话注入或运行实例分流，而不是 `vulcan-mcp-client` 服务端实现本身。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-15-CODEX_MCP_INJECTION_DIAGNOSIS.md`

## 3. 💻关键代码调整详情

- 本次未修改任何业务代码、配置代码或启动脚本。
- 本次排查主要核验了以下事实来源：
  - 仓库内 `src/main.rs`、`src/server.rs`、`src/lua_engine.rs` 的 Lua skill 加载与 MCP tool 注册链路；
  - `runtime/lua_skills/codeview_ast/skill.json` 的工具声明；
  - `C:\Users\20000\.codex\config.toml` 中的 MCP 配置；
  - `C:\Users\20000\.codex\sessions\2026\04\13\...` 与 `C:\Users\20000\.codex\log\codex-tui.log` 中的会话可见性证据。

## 4. ⚠️遗留问题与注意事项

1. 当前最可能的问题不在服务端，而在 Codex 客户端侧：
   - 配置修改后未被当前桌面实例完整重载；
   - 当前对话线程创建时未注入新增的 `vulcanmcp`；
   - 你正在使用的桌面实例与 `C:\Users\20000\.codex\config.toml` 不是同一套实际生效配置。
2. 若需要进一步确认，应优先在“新开一个全新线程”后观察当前会话是否出现 `vulcanmcp`；如果仍没有，再检查桌面端是否存在另一份运行时配置来源。
3. 由于当前代理无法直接读取桌面端内部的“已注入 MCP tool 清单”调试界面，本次结论基于本地配置文件、历史日志以及当前 session 记录综合推断。
