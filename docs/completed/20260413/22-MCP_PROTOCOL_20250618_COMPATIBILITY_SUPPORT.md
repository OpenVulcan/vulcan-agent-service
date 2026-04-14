# 任务目标

为 `vulcan-mcp-client` 增加对 MCP 协议版本 `2025-06-18` 的兼容支持，修复 Codex 与当前服务进行 `initialize` 握手时因协议版本不匹配而失败的问题。

# 执行步骤

1. 审查 `src/protocol.rs` 中的协议版本常量、版本协商逻辑和 feature 映射逻辑。
2. 将 `2025-06-18` 加入兼容协议版本集合，并更新 `negotiate_version(...)` 的支持范围说明。
3. 为 `has_feature(...)` 增加 `2025-06-18` 的判定分支，优先按与 `2025-11-25` 同级的能力集合处理，确保 Codex 握手可以顺利完成。
4. 构建验证，确认协议层修改不会引入编译错误。
5. 补充执行变更总结并迁移计划文件到完成目录。

# 技术选型

- 采用最小改动兼容策略，不调整现有 JSON-RPC 结构。
- 先把 `2025-06-18` 视作与 `2025-11-25` 同等能力等级，以优先恢复 Codex 可连接性；若后续发现协议细节差异，再按需要细化。

# 验收标准

- 服务端可接受 `2025-06-18` 作为 `initialize.protocolVersion`。
- `has_feature(...)` 不再把 `2025-06-18` 视为未知版本。
- `cargo build` 通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次为 `vulcan-mcp-client` 增加了对 MCP 协议版本 `2025-06-18` 的兼容支持，修复 Codex 在 `initialize` 阶段因版本不被接受而握手失败的问题。当前修复策略是：

1. 将 `2025-06-18` 纳入兼容版本集合；
2. 将 `2025-06-18` 的 feature 支持级别按与 `2025-11-25` 同级处理；
3. 保持现有最新版本 `2025-11-25` 仍为主版本，不改变主版本常量定义。

## 2. 📂文件变更清单

- 新增并更新：`docs/plan/20260413-22-MCP_PROTOCOL_20250618_COMPATIBILITY_SUPPORT.md`
- 修改：`src/protocol.rs`

## 3. 💻关键代码调整详情

- 在 [protocol.rs](D:\projects\vulcan-mcp-client\src\protocol.rs:11) 中，将 `PROTOCOL_VERSION_COMPATIBLE` 扩展为：
  - `2025-06-18`
  - `2025-03-26`
  - `2024-11-05`
- 在 [protocol.rs](D:\projects\vulcan-mcp-client\src\protocol.rs:14) 更新了支持版本注释说明。
- 在 [protocol.rs](D:\projects\vulcan-mcp-client\src\protocol.rs:42) 与 [protocol.rs](D:\projects\vulcan-mcp-client\src\protocol.rs:49) 中，为 `2025-06-18` 补齐：
  - `Sampling / Roots / Completions / Elicitation / ProgressToken / Cancellation`
  - `Streaming / StructuredLogging / ToolAnnotations / AudioContent / EmbeddedResource`
  的 feature 判定。

## 4. ⚠️遗留问题与注意事项

1. 本次采取的是“先兼容握手、后细化差异”的策略；如果未来确认 `2025-06-18` 与 `2025-11-25` 在某些字段语义上存在细微差别，再单独做精细化分流即可。
2. 本地 `cargo build` 已通过，说明协议层改动没有引入编译错误。
3. 要让 Codex 启动 MCP 时真正生效，仍需要重启当前正在运行的 `vulcan-mcp` 进程。
