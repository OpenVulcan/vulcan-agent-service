# LuaSkills 完整改造方案 v0.1

## 1. 文档目标

本文用于收敛当前 `vulcan-mcp` / `LuaSkills` 体系的完整改造方向。

本文希望回答以下问题：

- LuaSkills 的真实核心模型应该是什么
- `vulcan-luaskills-lib`、`vulcan-mcp`、未来 `vulcan-grpc` 如何分层
- skill 包格式应该如何调整
- `vulcan.` 运行时 API 应如何标准化
- system tools、skill tools、help、provider、状态与生命周期应如何设计
- 当前项目应按什么顺序改造，才能降低调试和迁移风险

本文不是最终协议文件，而是后续多个细分规范的统一上位设计稿。

## 2. 当前问题

基于当前项目状态，主要问题可以归纳为以下几类。

### 2.1 Skill 真实模型与 MCP 协议对象混在一起

当前 skill 目录和文档中仍然混用了以下概念：

- `tool`
- `resource`
- `resource_template`
- `prompt`

这些对象首先是 **MCP 协议分层**，不是 LuaSkills Core 真相。

继续让 LuaSkills 直接承载这些对象，会带来两个后果：

- skill 会被 MCP 协议污染
- `vulcan-luaskills-lib` 很难成为独立 runtime

### 2.2 Runtime、依赖管理、宿主接入边界不清

当前代码中：

- `vulcan-luaskills` 仓库内的 `lua_engine.rs` 更像 runtime
- `vulcan-luaskills` 仓库内历史上的 `skill_dependency.rs` 更像依赖管理
- 当前主仓库中的 `main.rs / server.rs / http_server.rs / grpc_server.rs` 更像 host / adapter

但这三层目前仍在同一产品边界下混合存在。

### 2.3 `vulcan.` API 逐步增长，但命名尚未标准化

当前 `vulcan.` 已经提供了很多能力，但存在以下问题：

- 顶层函数平铺较多
- `vulcan.process.exec` 与 `vulcan.runtime.lua.exec` 的语义需要持续保持清晰
- `context` / `client_*` / `tool_config` 等字段尚未结构化
- system tools 与普通运行时能力尚未分层

### 2.4 Help 体系仍然不够贴近真实工作流

当前 help 还容易被理解成：

- prompt 的替代物
- tool 级说明文档

但真实更合理的模型应该是：

- skill 主 help
- 子 help 列表
- workflow help

### 2.5 依赖、provider 与状态模型缺少统一标准

当前已经开始讨论：

- `sqlite` / `lancedb`
- provider / capability
- enable / disable / degraded

但这些还没有进入完整统一设计。

## 3. 总体设计原则

本次改造建议采用以下原则。

### 3.1 LuaSkills Core 不以 MCP 协议对象为真相

LuaSkills Core 应只关心：

- skill
- entry
- invocation
- help
- capability
- provider
- status
- lifecycle

不应直接以 MCP `tool/resource/prompt` 作为核心对象模型。

### 3.2 Runtime 只负责执行，不负责依赖安装

`vulcan-luaskills-lib` 只负责：

- 加载
- 调用
- 状态
- 宿主能力绑定
- provider 绑定

依赖下载、安装、升级、卸载，不属于 runtime。

### 3.3 `vulcan-mcp` 是接入层，不是 skill 真相来源

未来推荐关系应为：

- `vulcan-luaskills-lib`：runtime
- `vulcan-mcp`：adapter / host
- `vulcan-grpc`：adapter / host

### 3.4 Skill 格式先收敛，再拆 lib

当前阶段最优顺序不是先拆库，而是：

1. 先改 skill 格式
2. 在当前主仓库中验证
3. 再拆分 `vulcan-luaskills-lib`

原因很简单：

- 一边改协议、一边拆 runtime，调试边界会非常模糊

### 3.5 `vulcan.` 只保留通用运行时标准能力

所有宿主私有能力不应继续污染 `vulcan.` Core。

宿主扩展应使用：

- 私有 namespace
- 或单独 host extension 机制

## 4. 目标架构

推荐最终收敛为四层：

### 4.1 LuaSkills Core Spec

定义：

- skill 包格式
- entry 模型
- help 模型
- capability / provider / degradation
- status / lifecycle

### 4.2 `vulcan-luaskills-lib`

实现：

- Lua runtime
- entry 调用
- `vulcan.` Core API
- system tools 注册
- skill 状态管理
- provider 绑定

### 4.3 Host / Adapter

包括：

- `vulcan-mcp`
- `vulcan-grpc`
- 未来任意嵌入式宿主

负责：

- 初始化 runtime
- 提供宿主能力
- 包装 system tools
- 暴露最终产品交互面

### 4.4 Package / Dependency Manager

独立于 runtime 的安装与依赖层。

负责：

- source / registry
- install / uninstall / upgrade
- Lua / native / provider 依赖处理

## 5. Skill Core 模型

### 5.1 Skill 的真实核心

LuaSkills Core 推荐只定义以下对象：

- `skill`
- `entry`
- `help`
- `capabilities`
- `providers`
- `status`

### 5.2 Entry 代替 MCP 的 `tool/resource/prompt`

在 LuaSkills Core 中，不再把 `tool/resource/prompt/resource_template` 当成真相。

推荐统一成：

- `entry`

entry 是可调用单元，MCP 再决定如何将其映射为 `tool` 或其他对象。

另外，skill 级开关建议只保留单个 `enable` 字段：

- `enable: false`：显式关闭该 skill
- `enable: true`：显式启用该 skill
- 不声明时：默认启用

运行时仍然需要继续计算：

- 依赖是否满足
- 平台是否兼容
- provider 是否就绪

因此 skill 的最终状态不等同于 `enable` 字段本身。

并建议同步采用命名空间规则：

- skill 提供 namespace
- entry 只提供局部名
- canonical name 由 runtime 自动生成

推荐 canonical 格式：

- `skill_id-entry_name`

例如：

- `vulcan-codekit-ast-tree`

这样可以避免每个 entry 反复手写完整前缀，同时保留清晰的 skill 分组边界。

同时应补充严格目录与标识符规则：

- skill 目录名、`skill_id` 与 `entries[].name` 都必须匹配 `^[a-z]([a-z0-9-]*[a-z0-9])?$`
- 不允许大写字母
- 不允许下划线与其他特殊符号
- 不允许数字开头
- 不允许以 `-` 结尾

因此像 `__demo` 这样的目录天然不会被自动发现，不需要再写额外的 `__` 前缀跳过逻辑。

若不同 skill 组合后得到相同 canonical 名，则 runtime 应按稳定顺序自动追加：

- `-2`
- `-3`

不再保留旧的 `::` 兼容名。

### 5.3 Help 不再按 tool 一一绑定

### 5.4 保护技能与双平面生命周期管理

LuaSkills 需要同时支持两类技能管理平面：

- `skills` 平面
- `system` 平面

二者的职责边界如下：

- `skills` 平面：面向普通统一 skill 管理
- `system` 平面：面向宿主核心包、保留名称与内部维护

保护技能不是“当前已安装的某个技能实例”，而是“被宿主保留的 skill 名称”。

因此：

- 即使某个保护技能当前并不存在
- 只要其名称在保护名单中
- 就不能通过 `skills` 平面对其执行 `install / update / reload / enable / disable / uninstall`

但保护技能仍然允许由 `system` 平面处理。

这意味着：

- `skills.*`：不能操作保护技能
- `system.*`：可以操作保护技能

宿主应通过配置对象向 `vulcan-luaskills` 注入：

- `protected_skill_ids`

该配置属于宿主策略，而不是 skill 包自身元数据。

### 5.5 install 与 update 的语义边界

`install` 与 `update` 对外应保持两个独立入口，但内部可以复用同一条 package apply 主链。

推荐规则：

- `install`
  - skill 不存在时执行安装
  - skill 已存在时返回结构化状态
  - 不做隐式升级
- `update`
  - 明确用于更新已安装 skill
  - skill 不存在时返回结构化状态

在 skillhub 与完整安装规则尚未落地前，运行时至少应预留：

- `install` 占位入口
- `update` 占位入口
- 结构化状态返回

这样后续接入：

- GitHub
- URL
- skilllist
- skillhub

时无需推翻已有 API 语义。

### 5.6 宿主回调与注册表变化通知

LuaSkills 在执行以下动作后，应允许向宿主发送结构化回调：

- reload
- enable
- disable
- uninstall
- install
- update

回调至少分为两类：

- 技能生命周期事件
- entry 注册表差异事件

其中：

- 生命周期事件用于告诉宿主某个技能发生了什么状态变化
- 注册表差异事件用于告诉宿主有哪些 tool/entry 新增、移除或更新

这样宿主可以自行决定：

- 是否立即刷新 MCP tools 注册表
- 是否更新 IDE slash command / palette
- 是否记录审计日志
- 是否忽略部分 system events

LuaSkills 只负责发出结构化变化，不负责决定宿主如何展示这些变化。

### 5.7 共享依赖不应依赖记忆文件

共享依赖是否仍被使用，不应以持久化引用计数文件作为最终真相。

原因在于：

- skill 包可能被人工修改
- `dependencies.yaml` 可能被手工替换
- override 目录可能新增或删除 skill

因此更合理的方式是：

- 每次启动时重新扫描 skill
- 每次 reload 时重新扫描 skill
- 每次 enable / disable / uninstall 后重新扫描 skill

然后由运行时根据实时扫描结果决定：

- 哪些 shared 依赖仍然被使用
- 哪些 shared 依赖已经变成孤立目录

孤立 shared 依赖才允许被清理。

### 5.8 MCP 宿主包装的 system tools

当前 `vulcan-mcp` 宿主应包装一组面向普通 skill 管理面的 system tools：

- `vulcan-skill-enable`
- `vulcan-skill-disable`
- `vulcan-skill-uninstall`
- `vulcan-skill-reload`

这些工具的职责是：

- 调用 `vulcan-luaskills` 的技能管理入口
- 让 runtime 自身完成状态计算与 delta 生成
- 宿主根据 runtime delta 自动调整自身已注册的 MCP tools

保护技能仍然不应通过这组普通 tools 处理，而应保留给宿主自己的 system plane。

help 的组织方式应改为：

- 主 help
- 子 help 列表
- workflow help

### 5.4 超限输出模板属于通用协议资产，不属于 MCP 模板对象

需要明确区分：

- MCP 的 `resource_template`
- LuaSkills 的超限输出模板

前者属于 MCP 协议语义，不应继续作为 LuaSkills Core 真相。

后者则是通用 skill 非常需要的一类运行时资产，例如：

- 分页结果模板
- 截断结果模板

这里特指分页/截断场景下的模板，并不是普通回复消息模板。

普通回复消息的正文内容，仍应直接来自：

- Lua 返回的中间结果内容
- 宿主的最终拼装逻辑

因此推荐在 skill 包中保留独立目录，例如：

- `overflow_templates/`

它的定位应是：

- skill 私有超限模板资产目录
- 由 runtime 返回模板建议、模板 key/family、分割方式建议与上下文信息
- 由 host 决定最终是否使用、如何与分页/展示逻辑拼装

也就是说，模板规则可以是通用的，但真正的分页/截断处理权不应放在 runtime，而应放在上层宿主。

这样：

- MCP 可以根据自己的协议与展示模型决定如何生成分页输出
- IDE 可以根据自己的交互模型决定如何折叠、展开、路由或快捷展示
- runtime 只需要稳定提供可消费的完整元信息

并且还应进一步明确：

- runtime 不应决定是否真正截断内容
- runtime 不应决定是否真正分页切块
- runtime 不应决定是否写入超限中间文件
- runtime 不应决定超限文件的物理落点

这些都属于宿主控制面。

例如：

- `vulcan-mcp` 作为当前宿主，只能把相关超限产物放到自己控制的运行目录体系中
- IDE 宿主则可以把相关超限产物放到项目目录下，例如 `.vscode/` 等宿主管理目录

因此未来更合理的模型应是：

- runtime 返回完整内容与超限建议
- host 决定是否落盘、如何落盘、落到哪里

help topic 不要求与 tool 名严格一一对应。

例如：

- `vulcan-codekit`
- `tree`
- `safe-replace-workflow`
- `large-repo-analysis`

## 6. 新的 Skill 包格式方向

推荐新的 LuaSkills Core 包结构如下：

- `skill.yaml`
- `dependencies.yaml`
- `runtime/`
- `help/`
- `resources/`
- `licenses/`
- 可选：`README.md`

### 6.1 `skill.yaml`

作为 skill 主清单，负责描述：

- 名称
- 版本
- 描述
- entry 列表
- help 主节点与子节点
- capability / provider / degradation
- portability / host-private 属性

### 6.2 `dependencies.yaml`

单独描述依赖，而不继续把依赖规则和运行时规则混写在一起。

### 6.3 `runtime/`

只放运行时代码：

- 主入口 Lua
- 辅助模块
- 共享模块

### 6.4 `help/`

放 skill 主 help 与子 help。

支持：

- `.md`
- `.lua`

### 6.5 `resources/`

这里的 resources 是 **skill 运行时资源文件**，不是 MCP resource。

例如：

- ast-grep rules
- 静态模板
- SQL 片段
- 词典

### 6.6 `licenses/`

放第三方授权文件。

例如：

- `ast-grep-license.md`

## 7. Help 协议方向

### 7.1 主 help

主 help 节点负责回答：

- skill 是干什么的
- 有哪些 workflow
- 每个 workflow 一句话说明
- 推荐的使用入口

### 7.2 子 help

子 help 节点负责：

- 某个 workflow 的步骤与建议
- 某个 topic 的说明
- 某个子能力域的边界

### 7.3 Workflow 概念优先于 tool 说明

未来 help 更应该服务于 AI 与 IDE 的工作流导航，而不是变成单纯函数说明书。

### 7.4 Help 的实现层

help 不建议作为 `vulcan.` Core API。

更合理的方式是：

- 作为 system tools 暴露
- 再由官方 skill 包装

## 8. `vulcan.` API 重构方向

### 8.1 Core API 应保留的能力

建议保留为标准运行时能力：

- `vulcan.fs.*`
- `vulcan.path.*`
- `vulcan.os.*`
- `vulcan.json.*`
- `vulcan.cache.*`
- `vulcan.process.exec`
- `vulcan.context.*`
- `vulcan.sqlite.*`
- `vulcan.lancedb.*`

### 8.2 Cache 保留为 Core

缓存属于通用能力，因为很多 skill 需要临时数据存储与中间结果复用。

它应被定义为：

- 宿主管理
- 临时数据
- 非长期业务数据
- 非正式数据库

### 8.3 `exec` 与 `luaexec` 分离

当前代码已经体现出两条能力链：

- `vulcan.process.exec`
  - 真实语义是系统进程执行
- `vulcan.runtime.lua.exec`
  - 真实语义是隔离 Lua 执行

### 8.4 Context 应结构化

当前平铺字段：

- `context`
- `client_info`
- `client_capabilities`
- `client_budget`
- `tool_config`
- `skill_dir`
- `entry_dir`
- `entry_file`

后续建议收敛为结构化 context 模型。

## 9. 固定数据库能力的方向

### 9.1 `sqlite` 与 `lancedb` 保留为标准能力

当前建议不继续把：

- `pgsql`
- `pgvector`

这类大型数据库接入直接作为 LuaSkills Core 标准能力。

原因：

- skill 应保持轻量本地能力单元定位
- `sqlite + lancedb` 对绝大多数 skill 已足够
- 更大型或宿主私有的数据能力，应走宿主扩展 namespace

### 9.2 一 skill 一库

数据库声明按 skill 包维度，而不是按 tool 维度。

原则：

- 一个 skill 包最多绑定一个 `sqlite` 实例
- 一个 skill 包最多绑定一个 `lancedb` 实例
- 不允许跨 skill 访问其他 skill 的数据库

### 9.3 自动创建由宿主负责

skill 只声明是否需要数据库，不决定实际存放路径。

宿主负责：

- 自动建库
- 提供逻辑数据库上下文
- 决定物理存储路径

### 9.4 向量能力必须允许降级

对使用 `lancedb` 的能力，必须允许：

- `向量 + BM25`
- 降级为 `仅 BM25`
- 再不行则关闭对应功能

## 10. Capability / Provider / Status 模型

### 10.1 Capability

skill 声明自己需要什么能力，而不是直接写死 provider。

### 10.2 Provider

provider 是 capability 的实现面。

### 10.3 Status

至少应支持：

- `enabled`
- `degraded`
- `disabled_manual`
- `disabled_missing_dependency`
- `disabled_missing_capability`
- `disabled_incompatible`
- `disabled_runtime_error`

### 10.4 List / Info 可观测性

宿主必须能在 `list/info` 中解释：

- 为什么启用
- 为什么降级
- 为什么关闭

## 11. System Tools / Skill Tools / Host Wrapper

### 11.1 System Tools

system tools 属于 `lib` 的内部控制面。

它们不是普通 skill tools，但必须能注册到 Lua VM 中。

推荐命名空间：

- `vulcan.runtime.*`

### 11.2 推荐的 `vulcan.runtime.*`

至少包括：

- `vulcan.runtime.lua.exec`
- `vulcan.runtime.lua.help`
- `vulcan.runtime.skill.list`
- `vulcan.runtime.skill.info`
- `vulcan.runtime.skill.reload`
- `vulcan.runtime.skill.enable`
- `vulcan.runtime.skill.disable`

### 11.3 Skill Tools

skill tools 是 skill 自己对外公开的 entry。

它们属于 LuaSkills 生态能力面。

### 11.4 Host Wrapper

宿主可以：

- 直接包装 system tools
- 或复用官方包装 skill

例如：

- `vulcan-runtime`
- `vulcan-help-list`
- `vulcan-help-detail`

## 12. `vulcan-runtime` 的定位

`vulcan-runtime` 不应被视为 runtime 真相本身。

它更适合被定义为：

- 官方系统 skill 包
- 对 `vulcan.runtime.*` 的包装层

例如：

- `vulcan-lua-exec`
- `vulcan-lua-file`

而 help 能力则更适合被宿主包装为：

- `vulcan-help-list`
- `vulcan-help-detail`

也就是说：

- 执行能力可以来自 `vulcan-runtime` 这类官方系统 skill 包
- help 能力更适合直接来自 lib/system tools，再由宿主决定如何命名与是否公开

## 13. `vulcan-codekit` 的定位

### 13.1 当前阶段

继续保留在主仓库中，作为：

- 官方内建核心 skill
- 协议验证样本
- 高复杂度参考实现

### 13.2 后续阶段

在 `vulcan-luaskills-lib` 与 `vulcan-mcp` 接近正式发布的最后阶段，再独立出去更合适。

### 13.3 独立后的定位

README 中应明确：

- 最快、最佳体验方式是 `vulcan-mcp`
- 同时支持所有实现 `vulcan-luaskills-lib` 的宿主

### 13.4 `vulcan-codekit` 在 `vulcan-mcp` 中的角色

它可以继续作为：

- bundled core skill
- 默认安装
- 默认启用
- 动态加载

但不应被写死成不可移除模块。

## 14. 安装分层方向

未来 `vulcan-mcp` 应尽量瘦身。

建议：

- 本体只保留最小宿主能力与 runtime
- 核心 skill 与增强库独立成包
- 安装时默认勾选官方核心包
- 用户可以取消勾选或卸载

## 15. 实施顺序

推荐按以下顺序推进。

### 15.1 第一阶段：先定格式

优先完成：

- 新 skill 包结构
- help 协议
- capability / provider / degradation
- status / lifecycle

### 15.2 第二阶段：在现有主仓库中兼容新格式

优先迁移：

- `vulcan-runtime`
- `vulcan-curl`
- `vulcan-codekit`

### 15.3 第三阶段：重构 `vulcan.` 命名

优先完成：

- `vulcan.process.exec`
- `vulcan.runtime.lua.exec`
- `vulcan.runtime.lua.help`
- context 结构化

### 15.4 第四阶段：拆分 `vulcan-luaskills-lib`

在新格式稳定、官方 skill 验证充分后，再拆出：

- `vulcan-luaskills-lib`
- `vulcan-mcp`
- 未来 `vulcan-grpc`

### 15.5 第五阶段：补 package manager

独立补上：

- registry / source
- install / uninstall / upgrade
- 依赖解析
- 生命周期控制

## 16. 需要后续继续拆分的子文档

本文之后，建议继续拆出以下正式规范：

- LuaSkills Core Package Layout v0.1
- LuaSkills Help Protocol v0.1
- LuaSkills Core Runtime API v0.1
- LuaSkills System Tools Protocol v0.1
- LuaSkills Skill Status & Lifecycle v0.1
- LuaSkills Package & Registry Spec v0.1

## 17. 一句话总结

**本次改造的核心不是继续给 `vulcan-mcp` 加功能，而是把 LuaSkills 从 MCP 附属机制，提升为以 `vulcan-luaskills-lib` 为中心、以 skill 包格式为核心、以 `vulcan.` 标准 API 和 `vulcan.runtime.*` system tools 为骨架的独立运行时体系。**
