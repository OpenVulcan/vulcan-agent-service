# LuaSkills 模型能力宿主接口说明

本文档说明 `vulcan-agent-service` 统一服务中枢中，面向 LuaSkills 提供的简化模型能力子集。当前已基于 `luaskills 0.5.5` 接入专用 `vulcan.models.*` 宿主回调，并由宿主配置决定是否注册 embedding / LLM 能力。宿主侧直接消费 `luaskills` 导出的 entry / parameter description 与 final AI-facing `input_schema`。

## 设计边界

- 只支持 OpenAI-compatible HTTP API。
- 不向 Lua skill 暴露原始 OpenAI API、API key、base URL 或完整 request schema。
- LLM 固定为单轮、非流式调用。
- embedding 固定为单文本调用，不支持批量向量。
- 模型配置由宿主管理，独立于 `skill_config_root` 下的 LuaSkills 技能包配置双存储。
- Lua skill 只根据能力是否存在决定是否启用增强逻辑。

## Lua API

LuaSkills 侧固定暴露以下接口：

```lua
local status = vulcan.models.status()
local ok = vulcan.models.has("embed")
local embed_result = vulcan.models.embed("hello")
local llm_result = vulcan.models.llm("system prompt", "user prompt")
```

能力名仅建议支持：

- `embed`
- `llm`

## 返回结构

embedding 成功：

```lua
{
  ok = true,
  vector = { 0.1, 0.2 },
  dimensions = 2,
  usage = {
    input_tokens = 3,
    output_tokens = nil,
  },
}
```

LLM 成功：

```lua
{
  ok = true,
  assistant = "分析结果",
  usage = {
    input_tokens = 10,
    output_tokens = 20,
  },
}
```

失败统一返回：

```lua
{
  ok = false,
  error = {
    code = "provider_error",
    message = "model provider returned HTTP 400",
    provider_message = "供应商原始错误信息，已由宿主脱敏",
    provider_code = "bad_request",
    provider_status = 400,
  },
}
```

稳定错误码：

- `model_unavailable`
- `invalid_argument`
- `provider_error`
- `timeout`
- `budget_exceeded`
- `internal_error`

## 宿主配置

配置文件位于：

```text
runtime/configs/model_config.yaml
```

核心结构：

```yaml
format_version: 1
openai_compatible:
  enabled: false

  embedding:
    enabled: false
    base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1"
    api_key: "${env:DASHSCOPE_EMBEDDING_API_KEY}"
    model: ""
    timeout_ms: 30000
    request_overrides: {}

  llm:
    enabled: false
    base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1"
    api_key: "${env:DASHSCOPE_LLM_API_KEY}"
    model: ""
    temperature: 0.2
    max_tokens: 1200
    timeout_ms: 60000
    request_overrides: {}
```

embedding 与 llm 的 `base_url/api_key` 必须分别配置在各自能力块下；`openai_compatible` 顶层只接受当前结构定义的字段，避免宿主在多供应商、多账号场景下误用密钥。

生效规则：

- embedding 只读取 `openai_compatible.embedding.base_url/api_key`
- llm 只读取 `openai_compatible.llm.base_url/api_key`
- 未启用的能力不会校验对应 endpoint/key
- 已启用的能力缺少自身 endpoint/key 时，能力不会注册

`request_overrides` 用于处理不同供应商不统一的扩展字段，例如关闭 thinking 的供应商专用参数。宿主会忽略 `model`、`input`、`messages`、`stream` 等保留字段，确保 Lua skill 不能改变接口边界。

## Rust 接入点

当前已预留两个宿主模块：

- `src/model_config.rs`
  - 负责读取、预载、热重载 `model_config.yaml`
  - 支持 embedding 与 llm 独立配置 `base_url/api_key`
  - 支持 `${env:NAME}` 形式的 API key 引用
  - 默认关闭或能力未启用时不会要求对应环境变量存在
- `src/model_provider.rs`
  - 提供 `model_status`
  - 提供 `has_model_capability`
  - 提供 `model_embed` / `model_llm`
  - 提供 `install_luaskills_model_callbacks`，按当前配置注册或清理 LuaSkills 模型回调

## 回调注册策略

`vulcan-agent-service` 在启动预载和 `reload_vulcan_mcp_configs` 后都会根据 `model_config.yaml` 重新注册回调：

- `embedding.enabled=true` 且 embedding 专属 provider 配置完整时注册 `vulcan.models.embed`
- `llm.enabled=true` 且 llm 专属 provider 配置完整时注册 `vulcan.models.llm`
- 未注册时 `vulcan.models.has(...)` 返回 `false`
- 未注册时直接调用会得到 `ok=false` 与 `model_unavailable`

LuaSkills 传给宿主的 caller context 会包含：

- `skill_id`
- `entry_name`
- `canonical_tool_name`
- `root_name`
- `skill_dir`
- `client_name`
- `request_id`

## 热重载

`reload_vulcan_mcp_configs` 现在会重载：

- `client_budgets.yaml`
- `tool_configs.yaml`
- `model_config.yaml`

它仍然不会重载 `config.yaml` 或需要重启的传输配置。
