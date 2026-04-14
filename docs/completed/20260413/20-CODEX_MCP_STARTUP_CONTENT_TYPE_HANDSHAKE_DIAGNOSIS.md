# 任务目标

排查 Codex 启动 `VulcanMcp` 时出现的 `missing-content-type` 握手失败报错，确认这是服务端实现问题、客户端配置问题，还是协议端点错配导致的问题，并给出可执行修复建议。

# 执行步骤

1. 检查仓库 HTTP 传输实现，确认 `streamable-http` 与 legacy `sse` 分别对应的端点和允许的方法。
2. 检查当前本机 Codex 配置中 `VulcanMcp` 的 MCP server 条目，确认 URL 与传输类型配置。
3. 复现当前 Codex 配置对应的请求行为，验证 `/sse` 是否会对初始化 POST 返回空体/无 JSON 内容类型响应。
4. 输出根因判断、证据链和推荐修复方式。
5. 补充执行变更总结并归档计划文件。

# 技术选型

- 以本仓库 `src/http_server.rs` 的路由实现为主线判断协议端点语义。
- 以 `C:\Users\20000\.codex\config.toml` 作为实际 Codex 本地配置事实来源。
- 使用 `curl.exe` 对当前配置端点发起原始 HTTP 请求，直接对照报错行为。

# 验收标准

- 能明确指出 `missing-content-type` 的直接触发原因。
- 能给出至少一条本地可执行的修复建议。
- 诊断结论与仓库代码、Codex 配置、本地复现结果三者一致。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次未修改运行时代码，重点完成了对 Codex 启动 `VulcanMcp` 时握手失败报错的根因诊断。最终结论如下：

1. 当前问题不是 `initialize` 逻辑本身返回错误 JSON，也不是 `Content-Type` 响应头在 `/mcp` 初始化路径上缺失。
2. 问题根因是 Codex 当前配置把 `VulcanMcp` 指向了 legacy SSE 端点：
   - `[mcp_servers.VulcanMcp]`
   - `type = "sse"`
   - `url = "http://127.0.0.1:19201/sse"`
3. 但实际启动报错里的客户端栈是 `StreamableHttpClientWorker`，说明 Codex 这次使用的是 streamable HTTP 客户端而不是 legacy SSE 客户端。
4. 在这种情况下，Codex 会把初始化 POST 发到 `/sse`，而仓库服务端 `src/http_server.rs` 中 `/sse` 只注册了 `GET`，没有 `POST`，因此返回：
   - `HTTP/1.1 405 Method Not Allowed`
   - `content-length: 0`
   - 无 `Content-Type`
5. 这与 Codex 报错 `Unexpected content type: Some("missing-content-type; body: ")` 完全吻合，因此根因就是“客户端配置端点/传输类型错配”。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-20-CODEX_MCP_STARTUP_CONTENT_TYPE_HANDSHAKE_DIAGNOSIS.md`

## 3. 💻关键代码调整详情

- 本次未修改业务代码。
- 关键证据链如下：
  - `src/http_server.rs`
    - `POST /mcp`、`DELETE /mcp` 对应 streamable HTTP
    - `GET /sse`、`POST /message` 对应 legacy SSE
  - `C:\Users\20000\.codex\config.toml`
    - `VulcanMcp` 当前配置为 `type = "sse"` 且 `url = "http://127.0.0.1:19201/sse"`
  - 本地复现：
    - 对 `http://127.0.0.1:19201/sse` 发送初始化 `POST`
    - 返回 `405 Method Not Allowed`，`content-length: 0`，无 `Content-Type`
    - 与 Codex 启动报错一致

## 4. ⚠️遗留问题与注意事项

1. 若你希望 Codex 正常连接当前服务，应优先改为 streamable HTTP 配置，建议使用：
   - `url = "http://127.0.0.1:19201/mcp"`
   - 移除 `type = "sse"`，或确保 Codex 使用与该 URL 匹配的 streamable HTTP 方式
2. 如果你确实想继续使用 legacy SSE，则客户端不能把 `initialize` POST 直接打到 `/sse`，而必须：
   - 先 `GET /sse`
   - 再按返回的 endpoint 通过 `/message?sessionId=...` 发送消息
   这与当前 Codex 报错栈中表现出的 streamable HTTP 客户端模式不一致。
3. 因此从兼容性与维护成本看，最推荐的修复不是改服务端，而是把 Codex 的 `VulcanMcp` 配置改回 `/mcp`。
