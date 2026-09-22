# LuaSkills gRPC 接口说明

本文说明 `vulcan-agent-service` 在统一 gRPC 入口上暴露 LuaSkills 能力的接口边界、请求上下文、预算解析规则和主要 RPC 用法。同一个 gRPC 端点还会同时挂载兼容型 `McpService`、`HostAdapterService` 与 `vmm.v1.VmmService`；本文只聚焦 `LuaSkillsService` 这部分契约。

## 接口文件

- LuaSkills / MCP / Host Adapter Proto：`proto/v1/mcp_service.proto`
- LuaSkills / MCP / Host Adapter Package：`vulcan.mcp.v1`
- VMM Relay Proto：`proto/v1/vmm.proto`
- VMM Relay Package：`vmm.v1`
- 默认监听地址：`127.0.0.1:19202`
- 监听配置：`runtime/configs/config.yaml` 的 `grpc` 字段

服务启动后，同一个 gRPC 端点当前会同时挂载以下 service：

- `vulcan.mcp.v1.McpService`
- `vulcan.mcp.v1.LuaSkillsService`
- `vulcan.mcp.v1.HostAdapterService`
- `vmm.v1.VmmService`

## 设计原则

LuaSkills 的 gRPC 对外面遵循以下规则：

- 常见稳定功能直接映射为显式 RPC，例如工具发现、help 读取、标准 runtime-config dispatcher、skill lifecycle 和运行期配置重载。
- 只有 LuaSkill package 暴露出的动态 runtime entry 通过 `LuaSkillsService.CallTool` 调用。
- LuaSkills gRPC 调用不使用 `session_id` 表达上下文；工具调用本身是请求级上下文。
- 每个 `LuaSkillsService` 请求都必须携带 `context.client_name`。
- `client_name` 是 gRPC 专用的可信客户端名，用于精确解析 bytes 预算。
- gRPC 不走 `VULCAN_CLIENT_MATCH_NAME` 环境变量、`Vulcan-Client-Match-Name` 请求头覆盖，但会在未命中 `grpc_clients` 精确配置时回落到统一 `clients` 通配 match。
- `request_id` 只作为可选请求关联标识，不是会话标识，也不表达对话状态。

## 托管身份字段

LuaSkills gRPC 服务本身不把 `context.request_id` 当作会话，也不会自动把 gRPC 连接状态转换成 LuaSkill 参数。因此，直接使用 `LuaSkillsService.ListTools` / `CallTool` 的普通 gRPC 客户端默认处于非托管模式。

LuaSkills 生态保留 `LUASKILL_SID` 作为通用托管身份字段。该字段是文档级契约，不是 LuaSkills 运行时内置特殊字段。动态工具如果在 `input_schema_json` 中暴露 `LUASKILL_SID`，表示该工具需要稳定会话、任务或上下文身份。

gRPC 对接方应按自身能力选择处理模式：

1. 非托管模式：保持 `input_schema_json` 不变，`CallTool.arguments_json` 由模型、用户或调用方显式提供 `LUASKILL_SID`。如果工具的 create/start/bootstrap 入口支持缺省生成 ID，可以不传该字段并让 LuaSkill 生成。
2. 托管模式：客户端在自己的工具暴露层隐藏 `LUASKILL_SID`，并在调用 `CallTool` 前把稳定身份值写入 `arguments_json`。该隐藏和注入发生在 gRPC 客户端侧，不由 `LuaSkillsService` 自动完成。
3. Host Adapter 模式：宿主插件如果需要统一计算稳定身份，应先调用 `HostAdapterService.BuildHostAdapterRuntime` 或自身上下文归一化逻辑，再把归一化出的 `LUASKILL_SID` 注入动态工具参数。

托管模式下，对接方还应对模型可见 help 增加说明：该身份字段由宿主注入，模型不应询问、打印或保存原始值。如果工具响应带回被注入的原始身份值，对接方应在二次暴露层脱敏或改写为托管状态说明。非托管模式下，对接方应保留原始 help，让工具自己的 create 说明指导模型显式回显和保存公开 ID。

## McpService

`McpService` 是兼容型 MCP gRPC 服务面，主要用于旧调用链兼容、基础健康检查、通用调用和长连接能力保留。

| RPC | 请求 | 响应 | 说明 |
| --- | --- | --- | --- |
| `Healthz` | `google.protobuf.Empty` | `HealthzResponse` | 返回服务存活状态、版本和 MCP 协议版本。 |
| `Call` | `McpCallRequest` | `McpCallResponse` | 旧版通用 MCP 方法调用入口，例如 `tools/call`、`tools/list`。 |
| `Connect` | `ConnectRequest` | `stream ConnectEvent` | 建立带心跳的长连接，当前主要用于基础连接管理。 |

新接入 LuaSkills 能力时，推荐优先使用 `LuaSkillsService` 的显式 RPC。`McpService.Call` 可作为兼容入口保留，但不应承载新的稳定 LuaSkills 管理能力。

## LuaSkillsService

`LuaSkillsService` 是 LuaSkills 的主服务面，稳定能力全部通过明确 RPC 暴露，动态工具只通过 `CallTool` 暴露。

| 分组 | RPC | 说明 |
| --- | --- | --- |
| 工具发现 | `ListSkills` | 列出已加载 LuaSkill package。 |
| 工具发现 | `ListTools` | 列出 LuaSkills 动态 runtime entry 暴露出的可调用工具。 |
| 工具发现 | `GetTool` | 按标准工具名获取单个动态工具描述。 |
| 动态调用 | `CallTool` | 调用一个 LuaSkills 动态工具。 |
| Help | `ListHelp` | 返回已注册 LuaSkills help 树的 Markdown 渲染文本。 |
| Help | `GetHelp` | 按 `skill_id + flow` 返回指定 help 节点的 Markdown 文本。 |
| 配置 | `RuntimeConfig` | 分发 LuaSkills 0.5.7 标准技能包配置 JSON 请求，并返回稳定 JSON 响应包络。 |
| 安装管理 | `ListInstalledSkills` | 渲染 USER 层受管 LuaSkills 清单。 |
| 安装管理 | `InstallSkill` | 从来源安装一个 USER 层 LuaSkill。 |
| 安装管理 | `UpdateSkill` | 按 `skill_id` 更新一个 USER 层 LuaSkill。 |
| 安装管理 | `UninstallSkill` | 卸载一个 USER 层 LuaSkill，并保留 SQLite / LanceDB 数据。 |
| 运行期配置 | `ReloadRuntimeConfigs` | 重载可热更新的运行期配置，目前覆盖 `client_budgets.yaml` 与 `tool_configs.yaml`。 |

## 请求上下文

所有 `LuaSkillsService` 请求都包含 `LuaSkillClientContext context`。

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `client_name` | 是 | 受信任 gRPC 客户端名称，用于精确 bytes 预算解析。空字符串会返回 `INVALID_ARGUMENT`。 |
| `client_version` | 否 | 客户端版本，进入运行时上下文用于诊断。 |
| `request_id` | 否 | 请求关联标识；不是 `session_id`，不参与会话状态管理。 |

示例：

```json
{
  "context": {
    "clientName": "workbench-grpc",
    "clientVersion": "1.0.0",
    "requestId": "req-20260429-001"
  }
}
```

上例采用 protobuf JSON 映射的 lowerCamel 字段名。实际业务代码应以目标语言生成的 gRPC stub 字段名为准。

## 预算解析

LuaSkills gRPC 专用预算配置位于 `runtime/configs/client_budgets.yaml`：

```yaml
format_version: 1
grpc_clients:
  workbench-grpc:
    budgets:
      tool_result:
        bytes:
          default: 50000
        lines:
          default: -1
      file_read:
        bytes:
          default: 50000
        lines:
          default: -1
```

解析规则：

- `grpc_clients` 的 key 必须与请求中的 `context.client_name` 完全一致。
- `grpc_clients` 仅作为精确覆盖层，不支持通配符、正则或包含匹配。
- 如果没有命中 `grpc_clients`，会使用同一个 `context.client_name` 继续匹配通用 `clients` pattern。
- 不读取 `VULCAN_CLIENT_MATCH_NAME` 环境变量。
- 不读取 HTTP / SSE 的 `Vulcan-Client-Match-Name` 请求头。
- 如果 `grpc_clients` 和 `clients` pattern 都没有命中，才会回退到默认预算。

## 动态工具发现

`ListTools` 返回 LuaSkills 动态工具描述：

| 字段 | 说明 |
| --- | --- |
| `name` | 标准动态工具名，调用 `CallTool` 时使用。 |
| `description` | 工具描述。 |
| `input_schema_json` | JSON 编码的 MCP input schema。 |
| `annotations_json` | JSON 编码的 MCP tool annotations。 |
| `skill_id` | 拥有该工具的 LuaSkill package 标识。 |
| `entry_name` | package 内本地 entry 名称。 |
| `root_name` | 提供该工具的 runtime root 名称。 |
| `skill_dir` | 具体 skill 目录路径。 |

如果 `input_schema_json.properties` 中包含 `LUASKILL_SID`，客户端需要决定是否托管该字段。托管客户端向模型二次暴露工具时应移除该字段；非托管客户端应保持原样。

示例：

```bash
grpcurl -plaintext \
  -d '{"context":{"clientName":"workbench-grpc"}}' \
  127.0.0.1:19202 \
  vulcan.mcp.v1.LuaSkillsService/ListTools
```

## 动态工具调用

`CallTool` 只调用 LuaSkills 动态 runtime entry。宿主稳定能力不要再包装成动态 tool 调用，应使用上面的显式 RPC。

请求字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `context` | 是 | `LuaSkillClientContext`。 |
| `tool_name` | 是 | `ListTools` 或 `GetTool` 返回的标准工具名。 |
| `arguments_json` | 否 | JSON 编码的工具参数；为空时按 `{}` 处理。 |

托管客户端调用 `CallTool` 前必须把隐藏的 `LUASKILL_SID` 补入 `arguments_json`。非托管客户端不做补入；如果调用的是支持缺省生成身份的 create/start/bootstrap 入口，可以允许省略 `LUASKILL_SID`，由工具生成并返回公开 ID。

示例：

```bash
grpcurl -plaintext \
  -d '{"context":{"clientName":"workbench-grpc","requestId":"req-001"},"toolName":"vulcan-file-read","argumentsJson":"{\"file\":\"D:/tmp/example.txt\"}"}' \
  127.0.0.1:19202 \
  vulcan.mcp.v1.LuaSkillsService/CallTool
```

响应字段：

| 字段 | 说明 |
| --- | --- |
| `result_json` | JSON 编码的 MCP 兼容 `ToolCallResult`。 |
| `text` | 从 `result_json.content` 文本块提取并拼接出的便捷文本。 |
| `is_error` | 动态工具是否返回工具级错误。 |
| `message` | `is_error=true` 时的错误摘要。 |

## Help 接口

`ListHelp` 和 `GetHelp` 返回 `LuaSkillTextResponse`：

| RPC | 参数 | 说明 |
| --- | --- | --- |
| `ListHelp` | `context` | 返回全部已注册 help 节点目录。 |
| `GetHelp` | `context`、`skill_id`、`flow` | 返回指定技能的指定 help flow。`flow=main` 表示 skill 包说明节点。 |

## 配置接口

Skill 配置由宿主授权，协议、声明校验、双存储路由、revision、CAS、缓存与事件由 LuaSkills 0.5.7 管理；Lua skill 内部的 `vulcan.config.*` 使用同一套配置服务。

| RPC | 参数 | 说明 |
| --- | --- | --- |
| `RuntimeConfig` | `context`、`request_json` | `request_json` 是完整 JSON 编码的 `RuntimeSkillConfigToolRequest`；`response_json` 是完整 JSON 编码的 `RuntimeSkillConfigToolResponse`。 |

`request_json.action` 支持 `describe`、`validate`、`list`、`get`、`set`、`delete`、`refresh`。写入支持类型化单键或批量值，并可通过 `expected_revision` 执行 CAS。上游严格拒绝未知字段、无关动作字段和重复批量键。

`response_json` 始终保留 `ok`、`action`、`result`、`error.code`、`error.message` 和 `error.details`。请求形态、声明、校验或 revision 冲突属于该稳定响应包络，不转换为模糊文本；只有缺少受信任上下文、LuaEngine 不可用、线程任务失败或锁中毒等宿主执行失败才映射为 gRPC status。

客户端必须在调用前完成授权：`include_values=true`、`mode=installed`、`root_name`、`list/get/set/delete/refresh` 均可能披露敏感状态或改变持久化数据。服务端不会从任意自声明的 `client_capabilities` 推测授权。

## 安装管理接口

安装管理 RPC 固定面向 USER 层：

| RPC | 参数 | 说明 |
| --- | --- | --- |
| `ListInstalledSkills` | `context` | 列出 USER 层受管 LuaSkills。 |
| `InstallSkill` | `context`、`source`、可选 `source_type` | 安装 LuaSkill。`source_type` 可为 `github` 或 `url`，为空时按来源自动推导。 |
| `UpdateSkill` | `context`、`skill_id` | 更新指定 LuaSkill。 |
| `UninstallSkill` | `context`、`skill_id` | 卸载指定 LuaSkill，并保留 SQLite / LanceDB 数据。 |

## 错误语义

常见 gRPC status 映射如下：

| Status | 触发场景 |
| --- | --- |
| `INVALID_ARGUMENT` | 缺少 `context`、`context.client_name` 为空、`arguments_json` 不是合法 JSON、必填业务参数缺失。 |
| `NOT_FOUND` | 请求的动态工具或内部方法不存在。 |
| `INTERNAL` | LuaSkills 运行期、配置存储或宿主内部错误。 |
| `UNKNOWN` | 未归类的内部错误。 |

注意：`CallToolResponse.is_error=true` 表示工具级错误，gRPC 调用本身仍可能是成功返回；只有协议级或宿主级失败才会转为 gRPC status error。

## 当前边界

- 当前没有提供工具变更 watch stream，客户端需要主动调用 `ListTools` 刷新动态工具清单。
- `request_id` 当前是请求关联字段，不作为会话状态，也不替代 VMM 场景可能需要的业务上下文。
- 本文不描述 VMM 接口；VMM 的上下文模型和 LuaSkills 工具调用模型应保持分离。
