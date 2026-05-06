# LuaSkills Runtime Environment & Result v0.1

## 1. 文档目标

本文用于定义 LuaSkills runtime 的两个核心对象：

- `Runtime Environment`
- `Runtime Invocation Result`

本文回答的问题是：

- runtime 应该从宿主接收什么信息
- runtime 应该向宿主返回什么中间结果
- 哪些信息应该由 runtime 提供，哪些应继续留给宿主决定

## 2. 设计原则

### 2.1 Runtime 不直接读取宿主配置文件

runtime 不应直接依赖：

- `client_budgets.yaml`
- `tool_configs.yaml`
- 宿主私有路径规则

这些都应先由宿主解析，再注入到 runtime 环境中。

### 2.2 Runtime 返回的是中间结果，不是最终输出

runtime 不应直接返回：

- 最终分页文本
- 最终截断文本
- 最终宿主展示块

runtime 应返回的是：

- 内容
- 元信息
- 预算信息
- render/overflow 建议

由宿主继续决定最终展示。

### 2.3 Runtime 面向多宿主，而不是只面向 MCP

Environment 与 Result 的设计应同时适用于：

- `vulcan-agent-service`
- IDE 集成
- gRPC 宿主
- 其他嵌入式 host

因此字段必须尽量通用。

## 3. Runtime Environment

### 3.1 定义

`Runtime Environment` 是宿主在调用某个 skill entry 前，构造并注入给 runtime 的统一上下文。

它的作用是：

- 替代 runtime 直接读取宿主配置
- 统一承载 client/host/budget/provider 等上下文
- 让 runtime 能基于标准上下文运行，而不绑定具体宿主实现

### 3.2 建议字段

建议至少包含以下信息：

- `request_id`
- `skill_name`
- `entry_name`
- `host_name`
- `host_instance`
- `client_kind`
- `client_name`
- `client_capabilities`
- `budget`
- `tool_config`
- `feature_flags`
- `provider_availability`

### 3.3 字段含义

#### `host_name`

表示当前宿主名称，例如：

- `vulcan-agent-service`
- `vscode-host`
- `embedded-agent`

#### `host_instance`

可选。

用于表示当前宿主实例，例如：

- 当前设备名称
- 当前会话标识
- 当前进程上下文

#### `client_kind`

表示客户端类型，例如：

- `mcp`
- `ide`
- `embedded`

#### `client_name`

表示客户端名称。

示例：

- MCP 场景下可为连接设备或客户端名称
- IDE 场景下可为固定 IDE 名称，例如 `vscode`

#### `budget`

由宿主解析并注入的预算快照。

建议至少包含：

- `inline_bytes_limit`
- `read_bytes_limit`
- `inline_lines_limit`
- `read_lines_limit`
- `compression_allowed`
- `compression_threshold_bytes`

#### `tool_config`

由宿主侧解析后的配置快照。

runtime 可以消费，但不应自己读取宿主配置文件。

#### `provider_availability`

表示当前宿主可提供的标准能力与 provider 状态。

例如：

- `sqlite = enabled`
- `lancedb = disabled`
- `vector_embedding = unavailable`

### 3.4 Environment 不应承载的内容

不建议把以下内容塞进环境对象：

- 宿主最终分页文件路径
- 宿主 UI 状态
- 宿主内部展示模板
- 安装器状态
- skill enable/disable 持久化状态

这些不属于单次运行时调用上下文。

## 4. Runtime Invocation Result

### 4.1 定义

`Runtime Invocation Result` 是 runtime 对某个 skill entry 调用后返回给宿主的统一中间结果。

它是：

- Lua 返回值的规范化结果
- runtime 可移植输出模型
- host 渲染输入

### 4.2 建议字段

建议至少包含：

- `content_blocks`
- `metadata`
- `budget_hint`
- `render_hint`
- `split_hint`
- `template_hint`
- `diagnostics`

### 4.3 字段说明

#### `content_blocks`

承载核心内容。

可支持：

- 文本块
- JSON 块
- 字节说明块
- 指针说明块

注意：

- 这里承载的是原始中间内容
- 不是宿主最终展示文本

#### `metadata`

用于承载通用元信息，例如：

- 内容类型
- 内容来源
- skill 私有附加元数据

#### `budget_hint`

由 runtime 返回预算感知结果。

建议至少包含：

- `content_bytes`
- `content_lines`
- `bytes_limit_hit`
- `lines_limit_hit`
- `compressible`
- `estimated_compressed_bytes`

#### `render_hint`

表示 runtime 对展示方式的建议。

例如：

- `recommended_mode = inline`
- `recommended_mode = page`
- `recommended_mode = truncate`

这里只是建议，不是最终决定。

#### `split_hint`

表示 Lua/runtime 建议的内容分割方式。

例如：

- 按行分割
- 按块分割
- 按固定大小分割

具体枚举值后续可再细化。

#### `template_hint`

表示建议采用的超限模板信息。

例如：

- `family`
- `key`
- `variables`

这里只是建议，host 可接受也可忽略。

#### `diagnostics`

用于承载诊断信息，例如：

- 当前是否降级
- 缺失了哪些 provider
- 当前建议为何发生

## 5. Runtime Result 不应直接包含的内容

runtime 结果中不应直接包含：

- 宿主最终分页后的文本
- 宿主最终截断后的文本
- 宿主落盘后的绝对文件路径
- 宿主 UI 指令
- MCP 私有结构对象

这些应由 host 基于 runtime result 继续处理。

## 6. 与当前项目改造的关系

当前项目要走向独立 `luaskills`，应先在主仓库内把以下几件事改成以 Environment/Result 为中心：

- `lua_engine` 不再直接面向最终 `ToolCallOutput`
- `client_budget` / `tool_config` 由宿主先解析，再注入 runtime environment
- Lua skill 返回规范化的 runtime result
- `tool_result_format` 改为消费 runtime result 的 host render 层

## 7. 一句话结论

`Runtime Environment` 是宿主注入给 runtime 的标准上下文，`Runtime Invocation Result` 是 runtime 返回给宿主的标准中间结果；runtime 不再直接读取宿主配置，也不直接产出宿主最终输出。
