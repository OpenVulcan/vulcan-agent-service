# LuaSkills Capability Provider & Degradation Rules v0.1 草案

## 1. 文档定位

本文是面向未来 `luaskills` 的能力提供器与降级规则草案。

本文只讨论以下问题：

- skill 如何声明自己需要什么能力
- 宿主如何为 skill 解析 capability 与 provider
- provider 缺失时 skill 如何自动降级
- skill 的启用、关闭、降级状态如何对外暴露

本文**不**讨论以下内容：

- MCP `tool/resource/prompt` 协议分层
- gRPC 暴露模型
- skill 完整分发协议
- 目录结构最终定稿

这些内容应在独立的 LuaSkills Core Spec 与 Adapter Spec 中讨论。

## 2. 设计目标

本草案的目标是解决以下问题：

- `luaskills` 需要明确哪些底层能力是基础依赖，哪些是增强能力
- skill 不能把某个私有 provider 当成唯一前提，除非它本身就是宿主私有 skill
- 某些 provider 在部分平台上不可用时，skill 需要可预测地降级，而不是静默失效
- `skills list` 与 `skills info` 必须能明确解释一个 skill 当前为何可用、为何降级、为何被关闭

## 3. 核心术语

### 3.1 capability

`capability` 表示 skill 从运行时请求的一类逻辑能力，而不是具体实现。

示例：

- `storage.relational`
- `storage.vector`
- `search.keyword`
- `search.semantic`
- `memory.work`
- `exec.process`

### 3.2 provider

`provider` 表示某个 capability 的具体实现者。

示例：

- `storage.relational`
  - `vldb-sqlite`
  - `pgsql`
- `storage.vector`
  - `vldb-lancedb`
  - `pgvector`
- `search.keyword`
  - `sqlite-bm25`
- `search.semantic`
  - `lancedb+embedding`
  - `pgvector+embedding`

### 3.3 dependency

`dependency` 表示 skill 运行或增强所需的依赖项。

依赖项可以分为：

- Lua 依赖
- native / FFI 依赖
- provider 依赖
- host capability 依赖
- 其他 skill 依赖

### 3.4 degradation

`degradation` 表示在部分 capability 或 provider 不可用时，skill 仍继续加载，但缩减功能边界。

典型降级路径：

- `向量检索 + BM25` -> `仅 BM25`
- `BM25` -> `关闭该检索功能`
- `宿主浏览器能力` -> `仅返回 URL 文本`

### 3.5 status

`status` 表示 skill 当前可用性的宿主观测状态。

推荐状态值：

- `enabled`
- `degraded`
- `disabled_manual`
- `disabled_missing_dependency`
- `disabled_missing_capability`
- `disabled_incompatible`
- `disabled_runtime_error`

## 4. 基本原则

### 4.1 capability 是标准面，provider 是实现面

skill 应优先声明 `capability`，而不是直接把某个底层 provider 写死为唯一前提。

推荐：

- 声明 `search.semantic`
- 接受 provider：`vldb-lancedb`、`pgvector`

不推荐：

- 直接把 `vldb-lancedb` 当作唯一公开能力模型

### 4.2 公共 capability 允许私有 provider 实现

公共 capability 可以由私有 provider 承载，但必须满足以下条件之一：

- 有其他 provider 可替代
- skill 声明了明确的降级策略

如果某个 skill 依赖某个私有 provider，且没有替代或降级，则该 skill 应被视为宿主私有 skill，而不是通用 skill。

### 4.3 skill 可声明需要什么，不拥有 provider 决定权

skill 只能声明：

- 需要什么 capability
- 接受哪些 provider
- 缺失时如何降级

skill 不负责决定：

- provider 最终选谁
- provider 的物理存储路径
- provider 的底层初始化参数

这些由宿主与 `luaskills` 负责。

### 4.4 缺失必须可见，不允许静默隐藏

如果 skill 因依赖缺失、capability 缺失或 provider 不兼容而不可用，宿主必须在 `list/info` 中暴露原因。

不允许：

- skill 直接从列表里消失
- skill 被关闭但不给原因
- skill 自动降级后不提示功能边界变化

## 5. lib 级基础依赖与增强依赖

### 5.1 推荐基础依赖

当前建议 `luaskills` 将以下能力视为基础依赖：

- `vldb-sqlite`

原因：

- 更通用
- 平台覆盖更广
- 适合作为本地状态、工作记忆、结构化存储与 BM25 的底座

### 5.2 推荐可选增强依赖

当前建议将以下能力视为可选增强依赖：

- `vldb-lancedb`
- embedding provider

原因：

- `lancedb` 在部分平台上可能不可用
- 向量检索并非所有 skill 的基础生存前提
- `search.semantic` 本身通常是“向量数据库 + embedding provider”的组合能力

### 5.3 组合能力规则

以下能力不应被视为单点能力，而应被视为组合能力：

- `search.semantic`

其成立通常要求：

- `storage.vector` provider 可用
- embedding provider 可用

如果缺少其中任意一项，则不能视为完整的语义检索能力。

## 6. 当前推荐 capability 模型

### 6.1 存储类 capability

- `storage.relational`
- `storage.vector`

### 6.2 检索类 capability

- `search.keyword`
- `search.semantic`

### 6.3 执行类 capability

- `exec.process`
- `fs.readwrite`

### 6.4 可选宿主能力

- `host.browser`
- `host.desktop`
- `host.window`

宿主私有能力允许存在，但不应被误标为通用 capability。

## 7. 当前推荐 provider 模型

### 7.1 `storage.relational`

可接受 provider 示例：

- `vldb-sqlite`
- `pgsql`

### 7.2 `storage.vector`

可接受 provider 示例：

- `vldb-lancedb`
- `pgvector`

### 7.3 `search.keyword`

可接受 provider 示例：

- `sqlite-bm25`

### 7.4 `search.semantic`

可接受 provider 示例：

- `vldb-lancedb + embedding`
- `pgvector + embedding`

注意：

`search.semantic` 不应直接等价于 `vldb-lancedb`。

## 8. 自动降级规则

### 8.1 基本要求

凡依赖增强能力的 skill，必须声明以下内容之一：

- 替代 provider
- 自动降级策略
- 缺失时直接关闭对应功能

### 8.2 推荐降级策略类型

- `disable_feature`
- `keyword_only`
- `readonly_mode`
- `manual_step_required`
- `reduced_accuracy`
- `fail_load`

### 8.3 检索能力推荐降级路径

对典型检索型 skill，推荐以下默认路径：

1. 若 `search.semantic` 与 `search.keyword` 均可用  
   状态：`enabled`

2. 若 `search.semantic` 不可用，但 `search.keyword` 可用  
   状态：`degraded`
   行为：自动降级为 `keyword_only`

3. 若 `search.keyword` 也不可用  
   状态：`disabled_missing_capability`
   行为：关闭对应检索功能，必要时关闭整个 skill

### 8.4 宿主私有能力推荐降级路径

对依赖宿主私有能力的 skill，推荐以下策略：

- 有替代文本输出路径时：进入 `degraded`
- 无替代路径时：进入 `disabled_missing_capability`

## 9. Skill 状态模型

### 9.1 必要状态

宿主至少应支持以下状态：

- `enabled`
- `degraded`
- `disabled_manual`
- `disabled_missing_dependency`
- `disabled_missing_capability`
- `disabled_incompatible`
- `disabled_runtime_error`

### 9.2 `list` 必须暴露的信息

`skills list` 至少应暴露：

- `name`
- `version`
- `status`
- `reason`
- `missing_dependencies`
- `missing_capabilities`
- `resolved_providers`
- `degraded_features`
- `can_retry`

### 9.3 `info` 应暴露的补充信息

`skills info <name>` 应补充：

- 完整 capability 解析结果
- provider 选择结果
- 平台兼容性判断
- 降级策略命中结果
- 手动 enable/disable 状态

## 10. Skill 声明建议

以下为建议性声明模型，仅作为 v0.1 草案参考。

```yaml
capabilities:
  required:
    - name: storage.relational
      providers:
        - vldb-sqlite
        - pgsql
    - name: search.keyword
      providers:
        - sqlite-bm25
  optional:
    - name: storage.vector
      providers:
        - vldb-lancedb
        - pgvector
    - name: search.semantic
      providers:
        - vldb-lancedb+embedding
        - pgvector+embedding

degradation:
  search.semantic: keyword_only
  search.keyword: disable_feature
```

解释：

- `storage.relational` 与 `search.keyword` 被视为当前 skill 的基础能力
- `storage.vector` 与 `search.semantic` 被视为增强能力
- 若向量能力缺失，则退化为 `keyword_only`
- 若连关键词检索都不可用，则关闭该功能，必要时关闭整个 skill

## 11. 宿主职责

宿主与 `luaskills` 应承担以下职责：

- 探测当前平台上可用的 provider
- 对 skill 声明的 capability 进行解析
- 选择合适 provider 或进入降级路径
- 记录并暴露 skill 当前状态与原因
- 允许手动 enable / disable / retry / reload

宿主不应把以下责任交给 skill：

- provider 实际装载路径选择
- provider 生命周期管理
- provider 物理存储目录决定

## 12. 与当前项目实现的对应关系

基于当前仓库现状，推荐将以下事实写入后续实现：

- `sqlite` 系列能力可以作为更基础的宿主能力底座
- `lancedb` 不应被视为所有 skill 的硬前提
- 凡声明使用 `lancedb` 或语义检索的 skill，都必须允许降级或明确关闭
- `skills list` 未来必须能解释：
  - 为什么启用
  - 为什么降级
  - 为什么关闭

## 13. 当前 v0.1 范围外的问题

以下问题重要，但不在本文直接规定范围内：

- skill 目录结构最终定稿
- `tool/resource/prompt` 与 LuaSkills Core 的关系
- package source / registry / install / uninstall
- provider 二进制自动下载规范
- host 私有 namespace 的标准声明方式

这些内容建议分别进入：

- LuaSkills Core Package Layout Spec
- LuaSkills Lifecycle Spec
- LuaSkills Package & Registry Spec
- LuaSkills Host Extension Spec

## 14. 一句话原则

**LuaSkills 应声明 capability，而不是绑死 provider；provider 可以不同，但缺失时必须可替代、可降级、或可解释地关闭。**
