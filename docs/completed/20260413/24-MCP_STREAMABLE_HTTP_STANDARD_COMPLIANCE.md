## 任务目标

完整实现 MCP 新版 Streamable HTTP 标准流程，修复当前 `/mcp` 端点与官方协议不一致的问题，确保其能够与 Codex 等严格遵循 2025-11-25 传输规范的客户端正常握手、初始化、发送通知、管理会话并执行后续请求，同时继续保留旧版 `/sse` + `/message` 兼容链路供 legacy 客户端使用。

## 执行步骤

1. 梳理当前 HTTP 传输实现与 MCP 官方规范的差异，重点核查以下部分：
   - `/mcp` 是否同时支持 `GET` 与 `POST`
   - request / notification / response 的 HTTP 返回码是否符合规范
   - `MCP-Session-Id`、`MCP-Protocol-Version`、`Accept`、`Origin` 等关键请求头是否正确处理
   - POST 返回 SSE 时的事件组织、会话绑定与生命周期是否符合标准
2. 重构 `src/http_server.rs` 的新版 HTTP 传输处理逻辑：
   - 为 `/mcp` 增加 `GET` 处理器
   - 对 JSON-RPC request / notification / response 做明确分流
   - 对初始化阶段与后续阶段分别执行正确的版本、会话与协议校验
   - 将 notification/response 调整为 `202 Accepted` 无 body
   - 将无效 session 调整为 `404 Not Found`
3. 必要时补充 session 管理能力：
   - 支持 GET 建立独立 SSE 流
   - 支持将服务端消息投递到单一选定流，避免跨流广播
   - 为 POST 返回的 SSE 请求维护更清晰的流生命周期
4. 复核 `src/server.rs` 与 `src/protocol.rs` 的初始化、能力协商与版本协商逻辑，确保与新版 HTTP 头语义一致。
5. 完成构建与本地协议验证：
   - `cargo build`
   - 用本地 HTTP 请求模拟 initialize、initialized、tools/list、DELETE 等关键路径
   - 检查 `/sse` 兼容链路未被破坏

## 技术选型

- 以 MCP 官方 2025-11-25 `Transports` 与 `Lifecycle` 规范为唯一判定依据。
- 在不破坏现有业务层 `McpServer::handle_message` 的前提下，优先重构 HTTP 适配层，使协议约束尽量收敛在 transport handler 中。
- 保留 `/sse` + `/message` 作为 legacy transport，不将新旧协议逻辑继续混杂到同一条旧链路中。
- 对于新版 `/mcp` 的 SSE 能力，优先实现“符合标准的最小正确行为”，先保证 Codex 等严格客户端稳定接入，再考虑高级特性如 resumability。

## 验收标准

1. `/mcp` 同时支持 `GET` 与 `POST`，并满足新版 Streamable HTTP 基本规范。
2. `initialize` 能成功返回 `InitializeResult`，并正确附带 `MCP-Session-Id`。
3. `notifications/initialized` 等 notification 请求返回 `202 Accepted`，不会再触发 Codex 的 transport closed 错误。
4. 无效 session 返回 `404`，缺失或非法协议版本返回 `400`，非法 `Origin` 返回 `403`。
5. `cargo build` 通过，关键请求链路经本地验证可正常工作。
6. 旧版 `/sse` + `/message` 仍保留可用，不因新版修复而回归失效。

## 执行变更总结

### 1. 核心修复与调整概述

- 重构了 `src/http_server.rs` 的新版 `/mcp` 传输层，实现了更贴近 MCP 2025-11-25 Streamable HTTP 规范的标准流程。
- 新增 `GET /mcp` 作为会话绑定的 SSE 监听入口，并将 `POST /mcp` 收敛为标准 JSON-RPC 请求入口，不再把初始化和后续通知错误地混入同一条 SSE 返回链路。
- 对 `initialize`、普通 request、notification、response 分别执行不同 HTTP 语义：
  - `initialize` 返回 `200 + JSON` 并创建新 session
  - 普通 request 返回 `200 + JSON`
  - notification/response 返回 `202 Accepted`
  - 无效 session 返回 `404 Not Found`
  - 协议版本错误返回 `400 Bad Request`
  - 非法 Origin 返回 `403 Forbidden`
- 扩展 `SessionManager`，使其能够记录协商后的协议版本，并支持给单个 session 绑定新版 SSE 流。
- 同步修正服务端对支持协议版本的说明文案，纳入 `2025-06-18`。

### 2. 📂文件变更清单

- 新增：
  - `docs/plan/20260413-24-MCP_STREAMABLE_HTTP_STANDARD_COMPLIANCE.md`
- 修改：
  - `src/http_server.rs`
  - `src/session.rs`
  - `src/server.rs`
- 删除：
  - 无

### 3. 💻关键代码调整详情

- `src/http_server.rs`
  - 为 `/mcp` 增加 `GET` 路由，补齐新版 endpoint 的 `GET/POST/DELETE` 三种入口。
  - 新增 JSON-RPC 消息分类逻辑，显式区分 request、notification、response。
  - 新增 `validate_origin`、`validate_session_protocol`、`negotiated_protocol_from_initialize` 等辅助逻辑。
  - 将 `initialize` 改为仅走 `200 + JSON`，并从响应中提取协商后的 `protocolVersion` 保存到 session。
  - 将 notification/response 调整为 `202 Accepted` 无 body。
  - 将 `GET /mcp` 改为返回 `text/event-stream`，并先发送一个 priming 空事件。
- `src/session.rs`
  - `Session` 结构从仅保存 channel sender，扩展为保存 `protocol_version + Optional SSE sender`。
  - `SessionManager::create` 改为基于协商协议版本创建会话。
  - 新增 `attach_stream`、`detach_stream`、`protocol_version` 等方法，支撑新版 `/mcp` 的会话级流管理与头校验。
- `src/server.rs`
  - 更新初始化说明与资源文案，使服务端公开的版本信息与实际支持集保持一致。

### 4. ⚠️遗留问题与注意事项

- 当前新版 `/mcp` 已实现标准基础流程，但尚未实现 SSE resumability / `Last-Event-ID` 恢复能力；这属于可选增强项，不影响 Codex 等客户端的基础握手与调用。
- `Origin` 校验当前按本地部署场景放行 `localhost / 127.0.0.1 / [::1] / null / 无 Origin`，如果后续需要公网部署，应进一步收紧为明确白名单。
- 本次验证通过的是本地隔离端口临时实例；要让 Codex 实际生效，仍需要重启您当前正在运行的正式 `vulcan-mcp` 进程。
