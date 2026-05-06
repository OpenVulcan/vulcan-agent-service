# Host Adapter Public gRPC Contract v0.1

## 1. 文档目标

本文档定义 `vulcan-agent-service` 对外暴露的 Host Adapter gRPC 契约。该契约用于连接 OpenCode、OpenClaw、Claude Code、Qwen Code、Hermes、Generic MCP 等 Agent 插件或桥接端，使这些接入方可以通过统一服务中转层获取宿主能力画像、运行时上下文归一化、工具注册表差异和工具刷新提示。

## 2. 设计原则

1. Host Adapter 是 `vulcan-agent-service` 统一 gRPC 服务面中的独立 service，不混入 `McpService` 或 `LuaSkillsService`。
2. 公开 gRPC 层只负责 DTO 传输与调用 `host_core`，不承载业务判断。
3. 复杂内部 DTO 以 JSON 字符串形式传输，降低多语言客户端接入成本。
4. 响应保留关键快捷字段，避免客户端为了常用判断必须解析完整 JSON。
5. 插件端必须保留本地 fallback，不能假设 `vulcan-agent-service` 一定已经升级到该协议版本。

## 2.1 LuaSkills 托管身份字段

Host Adapter 面向原生插件和混合接入场景，通常比普通 MCP / LuaSkills gRPC 更容易拿到真实宿主会话。因此，插件端在二次暴露 LuaSkills 工具时应支持托管身份字段契约。

LuaSkills 生态保留 `LUASKILL_SID` 作为通用托管身份字段。该字段是文档级契约，不要求 LuaSkills 运行时或 LuaSkill 代码内置特殊处理。

插件端推荐流程：

1. 调用 `LuaSkillsService.ListTools` 获取原始动态工具 schema。
2. 调用 `HostAdapterService.BuildHostAdapterRuntime` 或插件本地逻辑归一化宿主上下文。
3. 如果工具参数包含 `LUASKILL_SID`，且当前 adapter 有稳定身份，则从模型可见 schema 中隐藏该字段，并在调用工具前注入稳定值。
4. 如果当前 adapter 没有稳定身份，插件端必须保留字段可见，让模型或用户显式提供，或者使用 LuaSkill create/start/bootstrap 入口的缺省生成逻辑。

托管模式下，插件端的模型可见 help 应覆盖或追加说明：身份字段由宿主注入，模型不应询问、打印、保存或要求用户提供原始值。如果工具响应带回被注入的原始身份值，插件端应在返回给模型前脱敏或改写为托管状态说明。非托管模式下，插件端应保留 LuaSkill 原始 help；当 create 入口生成公开 ID 时，应让模型显式告诉用户该 ID，并询问是否保存到项目规则文件。

## 3. 服务定义

新增服务：

```proto
service HostAdapterService {
  rpc GetHostAdapterProfile(HostAdapterProfileRequest) returns (HostAdapterProfileResponse);
  rpc BuildHostAdapterRuntime(HostAdapterRuntimeRequest) returns (HostAdapterRuntimeResponse);
  rpc DiffToolRegistry(HostAdapterDiffToolRegistryRequest) returns (HostAdapterDiffToolRegistryResponse);
  rpc BuildToolRefreshNotice(HostAdapterToolRefreshNoticeRequest) returns (HostAdapterToolRefreshNoticeResponse);
}
```

## 4. 通用上下文

`HostAdapterClientContext` 承载插件客户端身份：

1. `client_name`：受信任客户端名称，例如 `opencode-plugin`。
2. `client_version`：可选版本号，用于诊断。
3. `request_id`：可选请求关联标识，不表示会话身份。

该上下文当前不参与业务判断，但保留为后续预算、诊断、审计和 capability 分流入口。

## 5. GetHostAdapterProfile

### 5.1 请求字段

1. `context`：客户端上下文。
2. `host_kind`：宿主类型或别名，例如 `opencode`、`generic-mcp`、`claude`。

### 5.2 响应字段

1. `adapter_json`：完整 `HostAdapterDescriptor` JSON。
2. `profile_json`：完整 `HostCapabilityProfile` JSON。
3. `host_kind`：归一化宿主类型。
4. `display_name`：宿主展示名称。
5. `refresh_mode`：刷新模式，取值为 `dynamic`、`restart-required`、`unsupported`。
6. `identity_mode`：身份模式，取值为 `native-session`、`session-or-workmem`、`workmem-only`。
7. `is_error`：是否发生服务级错误。
8. `message`：错误摘要或诊断消息。

## 6. BuildHostAdapterRuntime

### 6.1 请求字段

1. `context`：客户端上下文。
2. `host_kind`：宿主类型或别名。
3. `adapter_host_kind`：可选覆盖宿主类型。
4. `session_id`：原生宿主 session id。
5. `workmem_id`：显式 WorkMem id。
6. `turn_id`：宿主 turn 或 message id。
7. `workspace`：工作区路径或项目键。
8. `user_message`：当前用户消息，用于诊断或提示词规划。
9. `conversation_id`：非 session 命名体系中的会话 id。
10. `root_session_id`：根 session id。

### 6.2 响应字段

1. `runtime_json`：完整 `HostAdapterRuntime` JSON。
2. `host_kind`：归一化宿主类型。
3. `session_id`：归一化 session id。
4. `workmem_id`：有效 WorkMem id。
5. `workmem_source`：WorkMem id 来源。
6. `identity_ready`：当前身份策略是否满足运行条件。
7. `degraded_reasons`：降级原因列表。
8. `is_error`：是否发生服务级错误。
9. `message`：错误摘要或诊断消息。

### 6.3 LuaSkills 注入建议

`BuildHostAdapterRuntime` 只负责归一化宿主上下文，不直接修改 LuaSkills 工具 schema，也不直接调用 LuaSkill。插件端需要基于响应自行完成工具暴露和参数注入。

推荐规则：

1. 如果 LuaSkill 工具参数包含 `LUASKILL_SID`，且当前宿主上下文能提供稳定身份，插件端应隐藏该字段并在调用前注入。
2. 注入值应来自同一会话、根会话、会话等价标识或插件端稳定 fallback，不能每次调用随机变化。
3. 如果只能使用工作区级 fallback，插件端应在模型可见 help 中说明该身份不是原生会话身份。
4. 如果无法得到稳定身份，插件端应保留 `LUASKILL_SID` 可见，让非托管 create/start/bootstrap 流程处理。

## 7. DiffToolRegistry

### 7.1 请求字段

1. `context`：客户端上下文。
2. `previous_snapshot_json`：旧 `ToolRegistrySnapshot` JSON。
3. `next_snapshot_json`：新 `ToolRegistrySnapshot` JSON。
4. `refresh_mode`：可选显式刷新模式。
5. `dynamic_tool_refresh_supported`：可选动态刷新支持标记。
6. `host_restart_required`：生命周期操作是否显式要求重启。

### 7.2 响应字段

1. `diff_json`：完整 `ToolRegistryDiff` JSON。
2. `changed_tool_ids`：所有变化 tool id。
3. `added_tool_ids`：新增 tool id。
4. `removed_tool_ids`：删除 tool id。
5. `updated_tool_ids`：更新 tool id。
6. `restart_required`：是否需要宿主重启或重连。
7. `summary`：紧凑摘要。
8. `is_error`：是否发生服务级错误。
9. `message`：错误摘要或诊断消息。

## 8. BuildToolRefreshNotice

### 8.1 请求字段

1. `context`：客户端上下文。
2. `host_kind`：宿主类型或别名。
3. `previous_snapshot_json`：旧 `ToolRegistrySnapshot` JSON。
4. `next_snapshot_json`：新 `ToolRegistrySnapshot` JSON。

### 8.2 响应字段

1. `notice_json`：完整 `ToolRefreshNotice` JSON。
2. `changed`：是否有 tool 变化。
3. `refresh_mode`：刷新模式。
4. `severity`：提示级别，取值为 `none`、`info`、`warning`、`error`。
5. `restart_required`：是否需要重启或重连。
6. `changed_tool_ids`：变化 tool id。
7. `model_message`：模型可见提示。
8. `user_message`：用户可见提示。
9. `is_error`：是否发生服务级错误。
10. `message`：错误摘要或诊断消息。

## 9. 错误语义

1. JSON 解析失败返回 `invalid_argument`。
2. DTO 序列化失败返回 `internal`。
3. tool snapshot 出现重复 id 返回 `invalid_argument`。
4. 未知 `refresh_mode` 返回 `invalid_argument`。

## 10. 插件端 fallback 要求

插件端调用 HostAdapterService 失败时必须回退到本地能力矩阵和本地 refresh notice 逻辑。常见 fallback 场景包括：

1. `vulcan-agent-service` 版本过旧，没有 `HostAdapterService`。
2. gRPC endpoint 未配置或不可连接。
3. 请求超时。
4. 响应 JSON 无法解析。

## 11. 当前范围外

1. 不在本服务里执行 LuaSkill install/uninstall/update。
2. 不在本服务里刷新宿主 UI。
3. 不在本服务里直接调用 VMM 记忆服务。
4. 不承诺所有宿主都支持 precheck/postaction，只表达支持等级和降级原因。
