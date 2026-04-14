## 任务目标

分析 Qwen Code 的源代码与相关文档，确认其在 MCP 连接场景下对 HTTP/Streamable HTTP/SSE 的实际支持情况，并与当前 `vulcan-mcp` 服务端实现进行对照，判断“Qwen Code 无法通过 HTTP 模式连接”究竟是客户端自身不支持，还是我们的协议兼容性仍存在缺口。

## 执行步骤

1. 获取 Qwen Code 的官方代码仓库、配置文档和与 MCP 相关的实现入口。
2. 定位其 MCP transport 相关代码，重点分析：
   - 是否支持 `streamable_http`
   - 是否只支持 `stdio` / `sse`
   - HTTP 模式的配置字段、握手逻辑、请求头和状态码期望
3. 对照 MCP 官方规范，判断 Qwen Code 的实现是：
   - 明确不支持新版 Streamable HTTP
   - 部分支持但有限制
   - 理论支持，但与我们服务端行为不兼容
4. 将 Qwen Code 的实现细节与当前 `vulcan-mcp` 的 `/mcp`、`/sse` 行为进行逐项对比，识别最可能的真实断点。
5. 输出结论、证据链与下一步建议；若发现我们仍存在协议偏差，则明确指出对应改进方向。

## 技术选型

- 以 Qwen Code 官方仓库源码和官方文档为主，避免依赖二手讨论。
- 以 MCP 官方 2025-11-25 规范作为新版 Streamable HTTP 的判断标准。
- 分析结果以“源码行为 + 协议要求 + 当前服务实现”三方交叉验证为准。

## 验收标准

1. 明确给出 Qwen Code 是否支持 HTTP/Streamable HTTP 的结论。
2. 若支持，指出它对 transport 的具体实现方式及约束。
3. 若不支持，指出源码或文档中的直接证据。
4. 将结论落到当前 `vulcan-mcp` 上，明确是客户端限制还是服务端兼容问题。
5. 形成完整中文分析记录，并在任务结束后追加执行变更总结并归档。

## 执行变更总结

### 1. 核心修复与调整概述

- 本次任务未修改业务代码，重点完成了对 Qwen Code 官方源码、官方文档和 MCP 官方传输规范的交叉分析。
- 结论是：Qwen Code 明确支持新版 Streamable HTTP，并不是“客户端本身不支持 HTTP 模式”。
- Qwen Code 在源码中直接引入并实例化 `@modelcontextprotocol/sdk` 的 `StreamableHTTPClientTransport`，且在官方文档中把 `http` 标注为推荐的远程 MCP 传输方式。
- 因此，若 Qwen Code 的 HTTP 模式连接失败，更可能是服务端协议兼容性、服务行为细节或配置使用方式的问题，而不是 Qwen Code 完全不支持。
- 同时发现一个很关键的配置细节：Qwen Code 的 `qwen mcp add` 命令会把所有 `http://` / `https://` URL 在未显式指定 transport 时自动识别为 `http`，而不是根据路径名自动识别成 `sse`。这意味着如果用户直接添加了 `/sse` 地址但没有写 `--transport sse`，Qwen Code 会错误地用 Streamable HTTP 客户端去连 SSE 端点。

### 2. 📂文件变更清单

- 新增：
  - `docs/plan/20260413-25-QWEN_CODE_HTTP_TRANSPORT_COMPATIBILITY_ANALYSIS.md`
- 修改：
  - 无
- 删除：
  - 无

### 3. 💻关键代码调整详情

- 无代码改动。
- 关键分析证据如下：
  - Qwen Code 开发文档明确写明支持三种 MCP transport：`stdio`、`sse`、`Streamable HTTP`。
  - Qwen Code 用户文档中明确区分：
    - `httpUrl` 用于 HTTP（streamable HTTP）
    - `url` 用于 SSE
  - Qwen Code 源码 `packages/core/src/tools/mcp-client.ts` 中直接：
    - `import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js'`
    - 当配置存在 `httpUrl` 时，调用 `new StreamableHTTPClientTransport(...)`
    - 当配置存在 `url` 时，调用 `new SSEClientTransport(...)`
  - `packages/core/package.json` 依赖 `@modelcontextprotocol/sdk` `^1.25.1`，说明其 HTTP 逻辑并不是自写的弱兼容，而是建立在官方 SDK 上。
  - `packages/cli/src/commands/mcp/add.ts` 中若用户未显式指定 `--transport`，只要参数以 `http://` 或 `https://` 开头，就默认设置为 `http`；不会自动识别 `/sse` 路径并切到 SSE。
  - `integration-tests/test-mcp-server.ts` 使用官方 `StreamableHTTPServerTransport` 起的测试服务只挂了 `POST /mcp`，说明 Qwen Code 的 HTTP 基础连接路径并不是“客户端完全不支持 HTTP”，反而已经有实际覆盖。

### 4. ⚠️遗留问题与注意事项

- Qwen Code 是否能成功连接，除了“是否支持 HTTP”之外，还取决于您在配置里是否真的使用了 `httpUrl` 或 `qwen mcp add --transport http`；若误填到 `url` 字段，它会走 SSE 而不是新版 HTTP。
- 反过来，如果您给的是 `/sse` 地址，但在 `qwen mcp add` 时没有显式写 `--transport sse`，Qwen Code 会把它当成 `http` 来配，造成错误 transport 连接。
- MCP 官方规范要求新版 Streamable HTTP 客户端先对同一 MCP endpoint 发起 POST 初始化，而不是沿用旧版 `/sse + /message` 流程；如果服务端在返回码、会话头、GET/POST 语义等方面不满足规范，Qwen Code 会失败。
- 如果后续需要进一步定位“Qwen Code 具体卡在哪一步”，最佳下一步是抓取它对 `/mcp` 的真实请求日志，核对它发出的 `Accept`、`MCP-Protocol-Version`、`MCP-Session-Id` 以及它对 GET/POST 的调用顺序。
