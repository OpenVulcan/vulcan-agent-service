# LuaSkills 先就地改造后拆分路线设计 v0.1

## 1. 文档目标

本文用于回答一个当前阶段最关键的问题：

**为什么 `vulcan-agent-service` 不应该立刻拆出 `luaskills`，而应该先在当前仓库中把运行时、skill 格式与宿主边界改造成目标形态，再进行拆分。**

本文关注的是“先在当前项目内完成目标式改造，再拆分 runtime/lib”的完整流程，而不是最终协议的逐字段细节。

## 2. 当前阶段为何不能先拆

### 2.1 现在拆，会把两类问题混在一起

当前项目同时存在两类变更需求：

- **协议层改造**
  - skill 结构调整
  - `skill.json` 向 `skill.yaml` 迁移
  - 去掉 `prompt / template / resources` 作为 LuaSkills Core 真相
  - help/workflow 模型收敛
  - 运行时上下文与返回协议调整
- **架构层改造**
  - 抽出 `luaskills`
  - 将 `vulcan-agent-service` 明确收口为统一服务中枢 / host-adapter 层
  - 后续支持 `grpc/lib/ide` 等宿主

如果现在先拆库，就会出现一个很难调试的问题：

- 是 skill 格式变更导致的问题
- 还是 runtime 拆分导致的问题
- 还是宿主适配边界调整导致的问题

这会显著抬高验证成本。

### 2.2 当前代码中仍存在明显的宿主耦合点

基于现有代码，至少存在以下几类紧耦合：

- 迁移前的 `src/lua_engine.rs`
  - 当时直接解析 Lua 返回值为 `ToolCallOutput`
  - 当时仍然理解 `overflow_mode`、`template_name` 等宿主渲染相关信息
  - 当时仍然直接向 Lua 注入 `client_budget`、`tool_config`
- `src/tool_result_format.rs`
  - 当前直接承担分页、截断、模板选择、宿主指针页渲染
- `src/client_budget.rs` 与 `src/tool_config.rs`
  - 当前由宿主链路直接读取配置文件
- `src/host_core/runtime.rs`
  - 当前直接承接 runtime 输出并进行调度，与 `support/tool_result_format.rs` 一起完成宿主渲染链路
- `src/transport/`
  - 当前承接 MCP / gRPC / HTTP / stdio 的外层协议适配；旧阶段集中在 `src/server.rs` 的职责已拆入该目录与 `host_core/`

其中第一类问题已经随着 runtime 迁移到 `luaskills` 新仓库而被拆出主仓；当前主仓仍需持续收口的，是宿主渲染与宿主配置真相。

## 3. 推荐总策略

推荐采用以下总路线：

1. **先在当前仓库内按目标架构完成就地改造**
2. **在主仓库内验证新 skill 格式、新运行时上下文与新结果协议**
3. **待边界稳定后，再拆出 `luaskills`**

一句话概括：

**先把“真相”改对，再把“代码位置”拆开。**

## 4. 当前仓库内应先完成的目标式改造

### 4.1 Skill 包格式先收敛到独立 runtime 可接受的结构

推荐先在当前仓库内完成 skill 包结构改造，使其天然面向未来 `luaskills`。

建议收敛为：

- `skill.yaml`
- `dependencies.yaml`
- `runtime/`
- `help/`
- `overflow_templates/`
- `resources/`
- `licenses/`

其中：

- `skill.yaml`
  - skill 主清单
- `dependencies.yaml`
  - 依赖描述
- `runtime/`
  - 所有运行时代码
- `help/`
  - 主 help、子 help、workflow help
- `overflow_templates/`
  - 分页模板、截断模板等超限输出模板
- `resources/`
  - 运行时静态资源
- `licenses/`
  - 第三方规则、资产、依赖许可证

### 4.2 先完成 `json -> yaml` 迁移

推荐在当前仓库中先把 skill 主清单从 `skill.json` 迁移为 `skill.yaml`。

原因：

- `yaml` 更适合后续扩展多级配置
- 更适合表达 help、workflow、capability、status、degradation 等结构
- 可以让未来 runtime/lib 与统一服务中枢 / host-adapter 统一围绕同一格式演进

### 4.3 取消 `prompt / template / resources` 作为 LuaSkills Core 真相

需要明确：

- `prompt`
- `resource_template`
- MCP `resource`

这些首先是 **MCP 侧对象模型**，不应继续作为 LuaSkills Core 真相。

但这不等于运行时不能拥有模板目录。

这里需要明确区分两类“模板”：

- **MCP 协议模板**
  - 例如 `resource_template`
  - 不应继续作为 LuaSkills Core 真相
- **LuaSkills 运行时超限模板**
  - 例如分页模板、截断模板
  - 这是 skill 私有输出资产，应该保留在 skill 包中

这里特指的是**超限输出模板**，不是普通回复消息模板。

普通回复消息本体仍然应该由：

- Lua 运行时中间结果中的内容块
- 宿主最终组合逻辑

直接决定，而不是再额外走一层“回复模板目录”。

在当前仓库里应先完成以下调整：

- skill 核心只保留“可调用 entry”与 help/workflow 描述
- 模板不再作为 skill 包协议中的 MCP 对象类型
- 新增 `overflow_templates/` 目录，用于承载分页与截断模板
- `resources/` 目录保留，但定义为 **skill 私有运行时资源目录**，不是 MCP Resource 真相

### 4.4 引入运行时中间结果层

这是本轮改造里最重要的一项。

当前问题在于：

- Lua 直接返回宿主最终输出模型过近
- `ToolCallOutput` 仍然掺杂了宿主渲染语义
- runtime 与 mcp 在“谁决定最终展示文本”上边界不清

推荐改为：

- Lua skill 返回 **运行时中间结果**
- runtime 只负责规范化与传递
- host 再决定如何渲染为 MCP/IDE/其他输出

建议引入中间层对象，例如：

- `RuntimeInvocationResult`
- `RuntimeContentBlock`
- `RuntimeRenderHint`
- `RuntimeBudgetHint`

其职责建议如下：

- `RuntimeInvocationResult`
  - 本次调用的统一返回根对象
- `RuntimeContentBlock`
  - 承载原始内容，可能是字符串、结构化 JSON、字节块说明等
- `RuntimeRenderHint`
  - 只表达 runtime 的建议，例如：
    - 建议截断
    - 建议分页
    - 建议使用哪个模板族
    - 建议压缩
- `RuntimeBudgetHint`
  - 告知当前内容体积、bytes、lines、是否超限、压缩前后尺寸等

这样可以把“运行时语义”和“宿主输出语义”分开。

### 4.5 bytes 限制必须进入统一协议

bytes 限制不应只是 MCP 私有概念，而应成为 runtime 与 skill 都能感知的通用能力。

原因：

- skill 可能需要判断是否应压缩结果
- runtime 可能需要判断是否应切换中间返回策略
- IDE / MCP / gRPC 等宿主都需要处理大结果问题

建议在运行时上下文中统一注入预算信息，例如：

- `inline_bytes_limit`
- `read_bytes_limit`
- `inline_lines_limit`
- `read_lines_limit`
- `compression_allowed`
- `compression_threshold_bytes`

同时在返回中保留：

- 原始内容 bytes
- 估算可展示 bytes
- 是否超限
- 是否建议压缩

### 4.6 runtime 应独立出自己的环境对象

当前运行时直接读取 `client_budgets.yaml`、`tool_configs.yaml` 这类配置的方式，未来不适合作为 `luaskills` 的核心模型。

推荐调整为：

- runtime 不直接读取宿主配置文件
- 宿主在初始化 runtime 或调用 entry 时，构造 **Runtime Environment**
- 将预算、配置、客户端信息、宿主能力等统一注入 runtime

建议引入统一环境对象，例如：

- `RuntimeEnvironment`

由宿主构造后传给 runtime，至少包含：

- client 信息
- host 名称
- client 类型
- budget 快照
- tool/skill 配置快照
- provider 可用性
- runtime 特性开关

这样可以保证：

- `mcp` 只是 RuntimeEnvironment 的一个生产者
- IDE 也可以生产自己的 RuntimeEnvironment
- runtime 自身不再依赖某个特定配置文件路径

### 4.7 模板逻辑保留在 runtime，但分页逻辑交给 host

这部分需要明确分责。

推荐原则：

- **模板规则是通用协议的一部分**
- **runtime 只返回完整信息、分割方式建议与模板建议**
- **分页、截断与最终输出拼装属于 host**
- **超限内容是否落盘、落到哪里，也属于 host**

更具体地说：

- runtime 可以返回：
  - 内容
  - render hint
  - split strategy / split mode
  - template family / template key
  - 变量上下文
- host 根据：
  - 内容大小
  - bytes 限制
  - 当前宿主类型
  - 分页策略
  - 模板族
  - content block
  - render hint
  - split strategy
  - template hint
  
  决定最终如何输出

也就是说，后续不应继续由 runtime 直接生成 MCP 指针页文本。

应改为：

- runtime 告知“这是一段大结果、建议 page、推荐 page 模板族”
- runtime 返回完整内容元信息、Lua 建议的分割方式以及模板建议
- mcp 自己决定如何分页、如何拼接展示文本、如何构造分页指令
- ide 也可以基于同一批信息，按自己的交互方式决定是否分页、是否折叠、是否做快捷入口展示

这里还应进一步明确：

- runtime 不负责真正执行截断
- runtime 不负责真正执行分页切块
- runtime 不负责决定是否写入临时文件
- runtime 不负责决定超限文件的物理存放位置

这些都应交由宿主决定。

例如：

- `vulcan-agent-service`
  - 当前受自身宿主形态限制，通常只能把超限中间文件放到 MCP 运行目录或其管理目录
- IDE 宿主
  - 则完全可以把超限文件放到项目目录下，例如 `.vscode/`、`.idea/` 或宿主自定义目录

因此从协议层看，runtime 应只返回：

- 是否建议分页/截断
- 分割方式
- 模板建议
- 内容体积与边界信息

而不返回“已经写好的分页文件路径”这类带强宿主语义的最终结果。

### 4.8 客户端类型信息应作为通用上下文透传

客户端类型信息不应是 MCP 私有临时字段，而应该成为 runtime 的标准上下文输入。

建议纳入 RuntimeEnvironment 或 RequestContext：

- `client_kind`
  - 例如 `mcp`、`ide`、`embedded`
- `client_name`
  - 例如 IDE 名称、设备名称、接入端名称
- `host_name`
  - 例如 `vulcan-agent-service`
- `host_instance`
  - 可选，实例名/设备标识

推荐规则：

- IDE 集成时，`client_name` 应是固定 IDE 名称
- MCP 场景下，`client_name` 可以是连接设备/客户端名称

这样 skill/running logic 可以基于统一字段决定输出偏好，而不需要知道当前是不是某个特定协议。

### 4.9 System tools 应先在当前仓库中明确为 runtime control plane

在当前仓库内应先把 system tools 边界收敛清楚。

推荐分层：

- `vulcan.*`
  - 通用运行时能力
- `vulcan.runtime.*`
  - system/control plane

建议至少覆盖：

- `vulcan.runtime.lua.exec`
- `vulcan.runtime.lua.help`
- 后续 skill/list/info/reload/enable/disable 等管理能力

同时保留原则：

- system tools 可以暴露到 Lua VM 内部
- 宿主可以自己封装暴露
- 如果宿主不封装，可使用官方 `vulcan-runtime` 这类系统 skill 包装层

### 4.10 help 模型先改成“主 help + 工作流节点”

help 不应继续与 tool 一一绑定。

建议当前仓库里先完成以下模式：

- 主 help
  - 返回 skill 能力概览
  - 返回可用 workflow/topic 列表
- workflow help
  - 返回某个工作流的详细执行方式
- topic help
  - 返回某个主题说明

这样 AI/IDE 的调用模式会更自然：

1. 先获取主节点
2. 选择 workflow
3. 再拉取对应 workflow help

## 5. 建议新增的中间层协议能力

为了支持后续拆分，这一轮就地改造建议补齐以下内容。

### 5.1 Runtime Environment

建议定义统一环境对象，至少包含：

- request id
- skill name
- entry name
- client kind
- client name
- host name
- client capabilities
- budget snapshot
- tool config snapshot
- feature flags
- provider availability

### 5.2 Runtime Invocation Result

建议定义统一返回模型，至少包含：

- content blocks
- metadata
- bytes / lines
- compression hint
- render hint
- split hint
- template hint
- diagnostics

### 5.3 Host Render Callback

建议从设计层明确：

- runtime 返回中间结果
- host 按自身协议决定最终渲染
- host 按自身策略决定是否落盘以及落盘目录

哪怕短期不做真正回调接口，也应先在当前仓库内把流程重构成：

1. Lua -> runtime result
2. runtime result -> host render input
3. host render input -> MCP/CLI 文本

如有需要，还可扩展为：

4. host render input -> host-managed overflow artifact

### 5.4 Compression/Overflow Signal

建议 runtime 统一输出：

- `overflow`
- `recommended_mode`
- `split_strategy`
- `compressible`
- `estimated_compressed_bytes`
- `bytes_limit_hit`

而不是直接输出最终的“截断文本/分页文本”。

同样也不应直接输出宿主已经写入的超限文件路径；若宿主最终选择写文件，应由宿主自行决定路径、命名和返回格式。

## 6. 还需要补充的改造项

除了用户已经点出的内容，当前仓库还建议同步补上以下事项。

### 6.1 兼容层策略

改造不是一次性硬切，需要明确兼容层。

建议：

- `skill.json` 与 `skill.yaml` 可短期双读
- 旧的 `prompt/resource/template` 注册行为逐步降级为 adapter 兼容
- `ToolCallOutput` 与新中间结果层短期并存，最终迁移到单一路径

### 6.2 状态与诊断输出

skill 改造后建议统一暴露：

- `enabled`
- `disabled_manual`
- `disabled_missing_dependency`
- `disabled_incompatible`
- `disabled_runtime_error`
- `degraded`

并能给出：

- 缺失能力
- 缺失 provider
- 降级原因
- 推荐修复方式

### 6.3 测试与验收必须前置设计

这次改造跨度很大，不能只改协议不改验证方式。

建议同步设计：

- skill 结构迁移验证
- runtime environment 注入验证
- bytes 限制与压缩 hint 验证
- host render 与 pagination 分责验证
- IDE/MCP 两类 client 上下文验证

### 6.4 文档与官方 skill 同步迁移

当前仓库中的官方 skill 至少需要逐步迁移和验证：

- `vulcan-runtime`
- `vulcan-codekit`
- `vulcan-curl`

原因：

- 这些 skill 能覆盖 system tools、workflow help、大输出、网络调用等关键路径
- 它们是验证新结构和新中间协议的最佳样本

## 7. 就地改造的阶段划分

### 阶段 1：先改 skill 包格式与帮助模型

目标：

- 新增 `skill.yaml`
- 明确 `runtime/help/resources/licenses` 目录
- 主 help + workflow help 模式跑通
- 让官方 skill 在当前仓库内兼容新结构

### 阶段 2：引入 Runtime Environment 与中间结果层

目标：

- 宿主构造 environment 注入 runtime
- runtime 不直接读取 host 配置
- Lua 返回统一改为 runtime 中间结果
- bytes / overflow / compression hint 纳入标准返回

### 阶段 3：将宿主渲染逻辑与 runtime 彻底分开

目标：

- runtime 不再直接产出分页/截断最终文本
- `tool_result_format` 迁移为 host render 层
- MCP 按自身需要生成分页与输出模板

### 阶段 4：system tools 与普通能力分层收敛

目标：

- `vulcan.*` Core 与 `vulcan.runtime.*` 分层明确
- `vulcan-runtime` skill 成为 system tool 官方包装层

### 阶段 5：在当前仓库中完成全面验证

目标：

- 官方 skill 全部切到新格式和新协议
- MCP、CLI、内嵌调用路径都验证通过
- 输出模型、分页、budget、help/workflow 稳定

### 阶段 6：最后再拆分 `luaskills`

只有在前述阶段稳定后，再进入真正拆分：

- 抽出 `luaskills`
- 将 `vulcan-agent-service` 收口为统一服务中枢 / host-adapter
- 后续再扩展 `grpc/lib/ide`

## 8. 未来拆分后的结构建议

在完成当前仓库内改造后，再推荐拆成：

- `luaskills`
  - runtime 核心
- `vulcan-agent-service`
  - 统一服务中枢 / host-adapter
- `vulcan-grpc`
  - 专用 gRPC host-adapter
- `luaskills-pm`
  - package/dependency manager

此时拆分会更安全，因为：

- skill 格式已经稳定
- 返回中间层已经稳定
- runtime environment 已经稳定
- host render 边界已经稳定

## 9. 一句话结论

当前项目最合理的路线不是“现在马上拆 lib”，而是：

**先在 `vulcan-agent-service` 内部，把 skill 结构、运行时环境、返回中间层、模板/分页职责、bytes 限制与客户端上下文全部改造成独立 runtime 目标形态；待这些真相稳定后，再顺势拆出 `luaskills`。**

这条路线的最大优势不是“保守”，而是：

**把协议重构、运行时重构、宿主重构拆成可验证的阶段，显著降低调试难度和返工成本。**
