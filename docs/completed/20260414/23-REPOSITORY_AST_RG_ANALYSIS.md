# 任务计划：基于 AST 与 RG 的仓库代码分析

## 任务目标

基于当前仓库的实际代码结构，使用 `vmcp_ast` 与 `vmcp_rg` 两个工具完成一轮工程化代码分析，形成对项目入口、核心模块、主要执行链路以及关键搜索命中的整体认识，并输出结构化分析结论。

## 执行步骤

1. 检查仓库顶层结构与当天计划编号，创建本次分析计划文件。
2. 使用 `vmcp_ast` 对仓库主要源码目录进行结构扫描，识别核心模块、关键函数与可能的入口点。
3. 基于 AST 结果，选择能代表主执行流程的关键标识符或函数名。
4. 使用 `vmcp_rg` 对关键标识符进行文本检索，并映射回函数/方法结构上下文。
5. 汇总 AST 与 RG 的结果，整理项目架构、调用链特征、代码组织方式及潜在关注点。
6. 对照计划完成自检，在文末补充执行变更总结，并将计划归档到 `docs/completed/20260414/`。

## 技术选型

- 优先使用 `vmcp_ast` 获取结构化轮廓，避免仅凭文本搜索推断代码关系。
- 再使用 `vmcp_rg` 围绕主流程关键词做定点钻取，以提高定位效率。
- 分析对象优先聚焦 `src/` 目录，必要时再补充 `mcp_server/`、`proto/` 等辅助目录。

## 验收标准

- 能明确指出项目的主要语言与主入口所在位置。
- 能归纳至少一条核心执行链路或模块调用关系。
- 能给出基于 `vmcp_ast` 的结构性结论与基于 `vmcp_rg` 的命中性结论。
- 不修改正式业务代码。
- 计划文件补充完整执行变更总结并完成归档。

## 验证结果

1. 已确认仓库主语言为 Rust，主入口位于 `src/main.rs`，其中 `async_main` 负责配置加载、运行时初始化、服务构建与网络传输启动。
2. 已通过 `vmcp_ast` 明确识别出核心模块：`main.rs`、`server.rs`、`http_server.rs`、`grpc_server.rs`、`lua_engine.rs`、`grpc_client.rs`、`config.rs`、`protocol.rs`、`session.rs` 等。
3. 已通过 `vmcp_rg` 串联出主执行链路：
   - `Config::load` -> `build_server`
   - `build_server` -> `with_lancedb` / `with_sqlite` / `with_vmm` / `with_lua_skills`
   - `run_network_transports` -> `run_http` + `run_grpc`
   - HTTP/gRPC 请求入口 -> `server.handle_message`
   - `handle_message` -> `handle_request`
   - `handle_request` -> `handle_tools_call` / `handle_initialize` 等具体 MCP 方法处理器
4. 已确认仓库具备“内建工具 + 外部服务客户端 + Lua 技能动态扩展”三层能力组装模式。
5. 本次仅进行结构分析，不涉及任何正式业务代码修改。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作未进行代码修复，而是基于 `vmcp_ast` 与 `vmcp_rg` 对当前仓库完成了一轮结构化代码分析。分析结果表明该项目是一个 Rust 实现的 MCP 服务端/客户端桥接工程，采用“启动层组装 + 协议层分发 + 传输层适配 + 扩展层挂载”的模块化结构，其中 `server.rs` 为协议与工具调度中枢，`main.rs` 负责运行模式与依赖接入，`lua_engine.rs` 提供技能型扩展能力。

### 2. 📂文件变更清单

- 新增：`docs/plan/20260414-23-REPOSITORY_AST_RG_ANALYSIS.md`
- 无业务代码文件新增、修改或删除
- 后续归档目标：`docs/completed/20260414/23-REPOSITORY_AST_RG_ANALYSIS.md`

### 3. 💻关键代码调整详情

- 使用 `vmcp_ast` 对 `src/` 目录递归扫描，识别出如下关键职责分层：
  - `main.rs`：启动编排、运行模式选择、服务组装、并发启动 HTTP/gRPC
  - `server.rs`：MCP Server 核心对象、默认工具注册、协议请求分发、工具执行
  - `http_server.rs` / `grpc_server.rs`：不同传输协议入口，并统一回落到 `handle_message`
  - `lua_engine.rs`：Lua VM 池、技能加载、资源/提示词/工具动态注册
  - `grpc_client.rs`：对 LanceDB、SQLite、VMM 以及 Scratchpad 的外部 gRPC 客户端封装
  - `protocol.rs`：MCP 协议数据结构与能力声明
- 使用 `vmcp_rg` 重点验证了如下链路：
  - `async_main` 中通过 `Config::load`、`build_server`、`run_network_transports` 完成主启动流程
  - `with_lua_skills` 会调用 `LuaEngine::load_from_dirs`，再把技能生成的 tools/resources/resource_templates/prompts 注入 `McpServer`
  - `http_server.rs` 与 `grpc_server.rs` 都不会直接实现业务工具逻辑，而是统一封装后调用 `server.handle_message`
  - `server.rs` 中 `handle_request` 是 JSON-RPC/MCP 方法总分发表，`handle_tools_call` 是工具执行的关键枢纽
  - `register_defaults` 在服务构造阶段注册默认 tool/resource/prompt/root，并混合数据库、Scratchpad 与 `runlua` 能力

### 4. ⚠️遗留问题与注意事项

- 从结构上看，`server.rs` 体量较大，默认注册、协议分发、参数解析、工具执行都集中在单文件内，后续如果继续扩展工具数量，维护复杂度会继续上升。
- `lua_engine.rs` 也是高复杂度模块，既负责 VM 池化，又负责技能元数据投影与 Lua/Rust 数据互转，适合后续拆分为加载、执行、桥接三个子模块。
- 本次为静态结构分析，未执行编译、测试或运行时验证，因此结论聚焦于代码组织与调用关系，不代表运行态行为已全部确认。
