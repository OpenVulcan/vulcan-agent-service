# 任务目标

在 MCP 客户端完成注册后，将客户端注册信息对 Lua 侧开放，使 Lua 运行时能够读取客户端相关上下文，并据此执行差异化逻辑，例如根据客户端类型决定字符串截取策略。

# 执行步骤

1. 梳理当前客户端注册、初始化与会话建立链路，明确客户端信息在服务端的保存位置与生命周期。
2. 梳理 Lua 调用链路，确认当前 Lua 技能执行时已经注入的上下文内容以及缺失点。
3. 设计并实现客户端注册信息向 Lua 暴露的方案，确保信息来源稳定、生命周期清晰，并避免破坏现有协议流程。
4. 为新增结构与关键逻辑补充必要的中英文注释，保证后续维护者能够理解设计意图。
5. 完成自检与必要验证，确认 Lua 侧能够稳定获取客户端信息，且原有流程不回归。

# 技术选型

- 优先复用现有初始化与会话管理链路保存客户端信息，避免在 Lua 层重复推断。
- 通过服务端运行时上下文向 Lua 注入只读客户端信息，避免 Lua 对核心协议状态产生反向写入。
- 保持默认行为兼容：若当前调用不存在客户端注册上下文，Lua 侧应拿到空值或空对象，而不是导致调用失败。

# 验收标准

1. 服务端在客户端注册完成后，能够保存并检索客户端注册信息。
2. Lua 运行时在技能调用场景下可读取客户端信息，并可基于客户端类型做逻辑分支。
3. 不影响现有工具调用、资源读取、Prompt 获取等原有能力。
4. 计划文件在任务完成后补齐执行变更总结，并归档到 `docs/completed/20260415/`。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次将客户端注册后的关键信息收敛为请求级上下文，并打通到 Lua 运行时。服务端现在会在 Streamable HTTP `initialize` 成功后，把客户端信息、协议版本与客户端能力保存到会话；后续该会话上的 `tools/call`、Lua 资源读取、Lua Prompt 解析与 `runlua` 都能够读取这些上下文。

## 2. 📂文件变更清单

修改：

- `src/protocol.rs`
- `src/session.rs`
- `src/http_server.rs`
- `src/grpc_server.rs`
- `src/server.rs`
- `src/lua_engine.rs`
- `src/main.rs`
- `docs/plan/20260415-09-LUA_CLIENT_REGISTRATION_CONTEXT.md`

新增：

- 无

删除：

- 无

## 3. 💻关键代码调整详情

- 在 `protocol.rs` 中新增 `RequestContext`，用于统一承载 `transport`、`session_id`、`protocol_version`、`client_info` 与 `client_capabilities`。
- 在 `session.rs` 中扩展 Streamable HTTP 会话结构，使会话不仅保存协商协议版本，也保存完整请求上下文，并提供按会话读取上下文的方法。
- 在 `http_server.rs` 中调整 `initialize` 后的会话创建逻辑，确保客户端注册信息在会话创建时落库；后续普通请求、通知与响应都会带着该上下文进入服务端处理链。
- 在 `server.rs` 中新增 `handle_message_with_context`，并把 Lua 技能调用、Lua 资源读取、Lua Prompt 与 `runlua` 全部改为透传请求上下文。
- 在 `lua_engine.rs` 中新增 `populate_vulcan_request_context`，将请求上下文注入到 `vulcan.context`、`vulcan.client_info` 与 `vulcan.client_capabilities`，并在执行结束后清空，避免 Lua VM 复用时残留上一次请求信息。
- 在 `grpc_server.rs` 中为 gRPC 单次调用补充了基础传输上下文标识；由于当前 gRPC unary 调用协议本身不携带初始化后的会话绑定信息，因此暂只暴露 `transport=grpc_unary`，不伪造客户端注册信息。

## 4. ⚠️遗留问题与注意事项

- 当前完整的客户端注册上下文透传主要覆盖 Streamable HTTP 会话链路；gRPC unary 调用由于没有现成的会话绑定参数，暂无法自动恢复 `client_info`。
- Lua 侧现在可直接读取 `vulcan.context.client_info.name`、`vulcan.context.protocol_version` 等字段；若调用发生在无会话上下文场景，相关字段会为空或空对象，技能实现需要自行兜底。
- 本次验证以 `cargo check` 为主，未额外搭建真实 MCP 客户端进行端到端回归。
