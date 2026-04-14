# 任务目标

修复 `vulcan-mcp-client` 的 streamable HTTP 初始化流行为，确保当客户端以 `Accept: text/event-stream` 对 `/mcp` 发起 `initialize` 时，服务端会使用同一个 `Mcp-Session-Id` 建立并保持 SSE 通道，而不是返回一次性 JSON 导致后续 `initialized notification` 发送时传输通道已关闭。

# 执行步骤

1. 审查 `src/http_server.rs` 中 `handle_streamable_post(...)` 对 `initialize`、session 创建和 SSE 分支的处理顺序。
2. 调整 `initialize + wants_sse` 路径：
   - 先创建真正要对外返回的 session；
   - 保留该 session 对应的 receiver；
   - 使用同一个 session id 返回 SSE 响应；
   - 将 `initialize` 的 JSON-RPC 响应推入该 session 流。
3. 移除之前对 `initialize` 强制 JSON 返回的特殊处理，恢复其在流式模式下的持续通道能力。
4. 构建验证，确认改动编译通过。
5. 补充执行变更总结，并迁移计划文件到完成目录。

# 技术选型

- 以最小改动修复现有 `SessionManager` 使用方式，不重构 legacy `/sse` 与 `/message` 通道。
- 通过在 `initialize` 分支提前保留 receiver，避免“创建了 session，但实际返回给客户端的是另一个临时 stream session”的错位问题。

# 验收标准

- `initialize + Accept: text/event-stream` 时，返回的 `Mcp-Session-Id` 与实际保持的 SSE 通道一致。
- 后续 `notifications/initialized` 不会因为通道提前关闭而直接报 transport closed。
- `cargo build` 通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次修复了 `/mcp` 在 streamable HTTP 初始化阶段的 session 对齐问题。此前的实现存在两个直接问题：

1. `initialize + wants_sse` 时，先创建了一个逻辑 session，但没有保留对应 receiver；
2. 真正返回给客户端的 SSE 响应又额外创建了另一个 `stream_id`，导致“对外返回的 `Mcp-Session-Id`”与“初始化阶段真实创建的会话”不一致。

这会使客户端在 `initialize` 之后继续发送 `notifications/initialized` 时，发现它绑定的传输通道已经关闭或错位，从而触发 `Transport channel closed`。

修复后：

1. `initialize + Accept: text/event-stream` 会先创建最终要返回给客户端的 session；
2. 该 session 的 receiver 会被保留下来并直接用于 SSE 响应；
3. `initialize` 的 JSON-RPC 响应会被推送到同一个 session stream；
4. 返回给客户端的 `Mcp-Session-Id` 与实际保持的 SSE 通道一致。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-23-STREAMABLE_HTTP_INITIALIZE_SSE_SESSION_ALIGNMENT_FIX.md`
- 修改：`src/http_server.rs`

## 3. 💻关键代码调整详情

- 在 [http_server.rs](D:\projects\vulcan-mcp-client\src\http_server.rs) 中新增 `initialize_stream_rx`，用于在 `initialize + wants_sse` 路径下保留真正的 stream receiver。
- 调整 `session_id == None && method == initialize` 分支：
  - 若 `wants_sse == true`，则先创建 session 并保留 `rx`；
  - 若 `wants_sse == false`，则维持原来的普通 session 创建方式。
- 调整 SSE 分支：
  - 优先复用 `initialize_stream_rx`；
  - 只有在非初始化 SSE 请求时，才创建新的临时 stream session。
- 这样可以保证 `final_session_id` 与实际向外暴露的 SSE 通道保持一致，不再错绑到另一个 `stream_id`。

## 4. ⚠️遗留问题与注意事项

1. 本次修改已通过 `cargo build` 构建验证。
2. 由于当前 `127.0.0.1:19201` 上运行的仍可能是旧进程，且临时端口回归命令在当前桌面终端环境中未稳定产出可读输出，因此最终行为仍需要在你重启 `vulcan-mcp` 服务后结合 Codex 实际连接结果复测。
3. 如果重启后仍报新的 streamable HTTP 兼容性问题，下一步应重点检查：
   - SSE 事件格式是否完全符合 Codex 当前 `rmcp` 客户端预期；
   - `notifications/initialized` 是否应该走同一流还是独立 POST；
   - 服务端是否还需要为 `/mcp` 的流式模式补充更明确的 keep-alive / 事件命名语义。
