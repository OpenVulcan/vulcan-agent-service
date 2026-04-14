# 任务目标

修复 `vulcan-mcp-client` 的 streamable HTTP `/mcp` 握手行为，确保 `initialize` 请求始终返回标准 JSON 响应而不是 SSE 长连接，从而兼容 Codex 的 MCP 启动握手流程。

# 执行步骤

1. 审查 `src/http_server.rs` 中 `handle_streamable_post(...)` 的 `Accept: text/event-stream` 分支逻辑，确认 `initialize` 请求当前会被错误升级为 SSE。
2. 调整 `/mcp` 响应策略：`initialize` 请求无论 `Accept` 如何，都强制返回 `application/json` 的单次响应，并携带 `Mcp-Session-Id`。
3. 保留非 `initialize` 请求的现有流式分支，避免扩大改动范围。
4. 使用本地 HTTP 请求验证：
   - `initialize + Accept: text/event-stream` 时返回 JSON；
   - 返回头包含 `Content-Type: application/json` 与 `Mcp-Session-Id`；
   - 响应不再悬挂到超时。
5. 完成后补充执行变更总结，并迁移计划文件到完成目录。

# 技术选型

- 以最小改动修复当前不兼容行为，不重构整个 streamable HTTP/SSE 分支。
- 通过在请求入口处识别 `initialize` 方法，显式绕过 SSE 分支，确保握手行为稳定。

# 验收标准

- `initialize` 请求不再因为 `Accept: text/event-stream` 被错误返回为 SSE。
- Codex 可获得带 `application/json` 内容类型的初始化响应。
- 本次修复不影响已有的 `/sse` legacy 端点。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次修复了 `vulcan-mcp-client` 在 streamable HTTP `/mcp` 握手阶段对 `initialize` 请求的错误处理。此前，只要请求头里带有 `Accept: text/event-stream`，服务端就会把 `initialize` 也错误地升级成 SSE 长连接响应，导致 Codex 在启动 MCP 时长时间等待并最终超时。修复后：

1. `initialize` 请求会被显式识别为“必须返回 JSON 的握手请求”。
2. 即便客户端声明可接受 `text/event-stream`，`initialize` 也不会再进入 SSE 分支。
3. 其他非 `initialize` 请求仍保留现有 SSE 分支逻辑，避免扩大影响面。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-21-STREAMABLE_HTTP_INITIALIZE_JSON_RESPONSE_FIX.md`
- 修改：`src/http_server.rs`

## 3. 💻关键代码调整详情

- 在 [http_server.rs](D:\projects\vulcan-mcp-client\src\http_server.rs:114) 新增 `should_force_json_response` 判断，用于识别 `initialize` 请求。
- 在 [http_server.rs](D:\projects\vulcan-mcp-client\src\http_server.rs:159) 调整 SSE 分支条件，从原先“只要 `wants_sse` 就返回 SSE”改为“`wants_sse && !should_force_json_response` 才返回 SSE”。
- 这样可以保证 `initialize` 始终落到 JSON 响应分支，带着 `Mcp-Session-Id` 正常完成握手。

## 4. ⚠️遗留问题与注意事项

1. 代码已通过本地 `cargo build` 构建验证。
2. 当前 `127.0.0.1:19201` 上运行的服务仍是旧进程，因此若未重启服务，实际对外行为不会立刻变化。
3. 要让 Codex 启动错误消失，需要在部署或本地运行中重启当前 `vulcan-mcp` 进程，使新的 [http_server.rs](D:\projects\vulcan-mcp-client\src\http_server.rs) 逻辑生效。
