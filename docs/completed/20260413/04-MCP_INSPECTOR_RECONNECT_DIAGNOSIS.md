# 任务计划：排查 MCP Inspector 刷新或服务端重启后的重连失败问题

## 任务目标

排查并判断以下问题的根因归属：

1. MCP Inspector 在页面刷新或服务端重启后，无法自动恢复连接，必须重启 Inspector 才能重新正常连接。
2. 相关报错包含：
   - `SseError: Premature close`
   - `connect ECONNREFUSED 127.0.0.1:19201`
   - `HTTP 404: Invalid OAuth error response`
   - `Cannot POST /register`
3. 需要明确这是：
   - MCP 服务端实现问题；
   - MCP Inspector 客户端问题；
   - 还是双方在重连、会话恢复、OAuth 发现/注册流程上的兼容性问题。

## 执行步骤

1. 检查仓库内 HTTP/SSE/Streamable HTTP 传输实现、会话管理与错误路径，确认当前服务端的实际行为。
2. 对照 MCP 官方最新传输规范与 Inspector 文档，核验当前实现是否满足：
   - Streamable HTTP 的 GET/POST/DELETE 端点要求
   - SSE 断连后的恢复方式
   - 会话失效后的 HTTP 状态码与客户端重建流程
   - OAuth 元数据发现与 `/register` 回退路径
3. 结合用户提供的错误日志，推导 Inspector 的行为路径，判断 `/register` 404 是认证分支误触发、客户端状态残留，还是服务端返回不符合预期导致。
4. 输出结论、给出验证方法与修复优先级建议。
5. 在本计划末尾补充执行变更总结，并迁移至 `docs/completed/20260413/`。

## 技术选型

- 优先以仓库内现有 Rust HTTP 传输实现为主线分析，不引入额外依赖。
- 对外部资料仅使用 MCP 官方文档、官方 SDK 文档以及必要的公开 issue 作为辅助判断依据。
- 本轮以“根因判断与修复建议”为主，除非发现必须立即修复的实现错误，否则不直接扩大到完整重构。

## 验收标准

- 能明确指出问题主要归因于服务端、Inspector，或双方协议兼容性组合问题中的哪一类。
- 能给出至少一条与代码实现直接对应的证据链，而不是停留在猜测层面。
- 能解释 `Cannot POST /register`、`Premature close`、`ECONNREFUSED` 三类报错之间的关系。
- 计划文件最终包含完整的执行变更总结，并迁移到 `docs/completed/20260413/`。

---

## 执行变更总结

### 1. 核心修复与调整概述

本次未直接修改运行时代码，重点完成了对 MCP Inspector 刷新/服务端重启后无法恢复连接问题的根因排查与归因确认。最终结论如下：

1. `Cannot POST /register` 不是当前 Rust MCP 服务端直接返回的错误，更像是 Inspector 自身 Node/Express 侧触发了 OAuth 动态注册兜底路径后的 404 页面。
2. `Premature close` 与 `ECONNREFUSED` 属于底层 SSE 连接在服务端重启时被中断、随后短时间内重连失败的直接表现。
3. 当前问题不是单点问题，而是“Inspector 走了已废弃的 legacy SSE 路径 + 当前服务端会话全内存化且不具备真正的可恢复流能力”共同叠加的结果。
4. 如果继续使用 Inspector 的 `sse` 传输模式，则出现服务端重启后必须重开 Inspector 的概率本来就会显著升高；更推荐切换到 `streamable-http` 并补全服务端对新规范的恢复语义支持。

### 2. 📂文件变更清单

新增/修改：

- 修改 `docs/plan/20260413-04-MCP_INSPECTOR_RECONNECT_DIAGNOSIS.md`，补充排查结论与执行总结。

删除：

- 无。

### 3. 💻关键代码调整详情

本次未改动业务代码，但完成了以下证据链核对：

1. 服务端传输实现核对
   - `src/http_server.rs` 仅暴露 `POST /mcp`、`DELETE /mcp`、`GET /sse`、`POST /message`，说明 legacy SSE 与 streamable HTTP 两套路径并存。
   - `src/session.rs` 中 `SessionManager` 与 `SseSessionManager` 都是纯内存会话管理，服务端重启后历史会话必然全部丢失，不存在跨重启恢复。
   - `src/http_server.rs` 中 streamable HTTP 的 `POST /mcp` 在 `Accept: text/event-stream` 分支会再次创建新的流式会话，当前实现更偏“临时推流”，并未形成完整的可恢复 GET SSE 通道。

2. Inspector 本地实现核对
   - 本地安装的 Inspector 版本为 `0.21.1`，依赖 SDK `^1.25.2`。
   - Inspector 服务端 `server/build/index.js` 的 `/sse` 路由明确写有注释：`The SSE transport is deprecated and has been replaced by StreamableHttp`。
   - Inspector 的 `createTransport` 在 `transportType === "sse"` 时会直接实例化 `SSEClientTransport`。
   - `SSEClientTransport` 在 `sdk/dist/esm/client/sse.js` 中已被显式标注为 `deprecated`，且首次 `onerror` 就会 `reject(error)`，这意味着服务端重启造成的 `Premature close` 很容易把本次连接直接打成失败状态。

3. `/register` 404 归因核对
   - Inspector 客户端代码中存在 `registerClient(...)` 逻辑，当缺失完整元数据时会默认对 `authorizationServerUrl + /register` 发起 POST。
   - Inspector 自身 Node 服务是 Express 应用，并未注册 `/register` 路由。
   - 用户日志中的 HTML 报文 `<!DOCTYPE html> ... Cannot POST /register` 与 Express 默认 404 页面特征一致，不像当前 Rust `axum` 服务直接输出的风格。
   - 因此可以判断：`/register` 404 更偏向 Inspector 端错误进入了 OAuth 注册兜底逻辑，或者使用了错误的基准 URL，并非你的 MCP 服务端“正常工作所必须实现的 /register 接口缺失”。

### 4. ⚠️遗留问题与注意事项

1. 若 Inspector 继续使用 `sse` 模式调试，服务端一旦重启，旧 SSE 连接断开后无法优雅恢复是高概率事件，不建议再把它作为主调试链路。
2. 当前服务端虽然有 `/mcp`，但距离最新 streamable HTTP 的“稳定恢复体验”仍有差距，尤其是：
   - 会话仅存在内存中；
   - 缺少真正的 GET SSE 恢复通道；
   - 断线恢复所需的事件重放/恢复语义未完整建立。
3. 下一步如果要彻底修复，应按优先级处理：
   - 优先在 Inspector 中改用 `streamable-http`；
   - 其次完善服务端 `POST /mcp` 与恢复语义；
   - 最后再排查是否仍会误触发 Inspector 的 OAuth `/register` 分支。
