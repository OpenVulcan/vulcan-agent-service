# LuaSkills Core Package Layout v0.1

## 1. 文档目标

本文用于定义 LuaSkills Core 的包结构标准。

本文回答的问题是：

- 一个 LuaSkill 包中应该包含哪些目录与文件
- 哪些内容属于 skill 包真相
- 哪些内容不属于 skill 包真相，而属于宿主、runtime 或 adapter

本文默认以下方向已经成立：

- LuaSkills Core 不以 MCP 的 `tool/resource/prompt/resource_template` 为真相
- 先在当前主仓库中完成 skill 结构改造，再拆分 `vulcan-luaskills-lib`

## 2. 设计原则

### 2.1 包结构可以固定，但固定的是 Core 真相

LuaSkills 应拥有稳定的包结构，以便：

- `vulcan-mcp` 易于加载
- 未来 `vulcan-luaskills-lib` 易于独立
- IDE / gRPC / embed 宿主可复用相同格式

但这个结构固定的是 **LuaSkills Core 真相**，而不是 MCP 协议对象。

### 2.2 Skill 包描述的是能力单元，不是协议投影

Skill 包应表达：

- 这个 skill 是什么
- 它有哪些 entry
- 有哪些帮助与 workflow
- 运行时代码放在哪
- 依赖什么能力和 provider

而不应直接表达：

- MCP tool 列表
- MCP prompt 列表
- MCP resource_template 列表

### 2.3 Skill 包不包含宿主状态

以下内容不应写回 skill 包目录：

- enable / disable 状态
- 安装状态
- provider 绑定结果
- 运行期缓存
- 落盘后的超限分页文件
- 宿主生成的临时产物

这些都应由宿主或 runtime 外围状态层负责。

## 3. 推荐目录结构

推荐 LuaSkills Core 包结构如下：

- `skill.yaml`
- `dependencies.yaml`
- `runtime/`
- `help/`
- `overflow_templates/`
- `resources/`
- `licenses/`

可选：

- `README.md`

## 4. 顶层文件说明

### 4.1 `skill.yaml`

`skill.yaml` 是 skill 主清单，是 skill 包中最核心的定义文件。

建议至少承载以下信息：

- skill 标识
- 名称与版本
- 简述
- entry 列表
- help 主入口
- help/workflow 列表
- portability / host-private 标记
- capability/provider 声明
- degradation 策略
- 启用策略
- 平台限制

并建议同时定义：

- `skill_id`
- `entries[].name`

其中：

- `skill_id` 负责提供 skill 命名空间
- `entries[].name` 只负责提供局部名

### 4.2 `dependencies.yaml`

`dependencies.yaml` 用于独立承载依赖定义，避免把依赖模型过度塞入 `skill.yaml`。

建议后续支持以下类别：

- 其他 skill 依赖
- Lua 运行时依赖
- native / ffi 依赖
- host capability 依赖
- provider 偏好

但依赖的下载与安装，不属于 runtime 层职责。

## 5. 目录说明

### 5.1 `runtime/`

`runtime/` 用于存放 skill 的运行时代码。

建议内容包括：

- 主入口文件
- 子 entry 实现
- 共享辅助模块
- 内部 Lua 库

约束：

- 所有可执行逻辑应优先放入 `runtime/`
- 目录内文件组织由 skill 自行维护
- runtime 不应依赖 MCP 的目录命名约定

### 5.2 `help/`

`help/` 用于存放 help 相关内容。

这里的 help 不是“每个 tool 一个说明书”，而是 skill 能力导航系统。

建议支持：

- 主 help
- 子 help 列表
- workflow help
- topic help

典型内容例如：

- `help/help.md`
- `help/tree.md`
- `help/large_repo_workflow.md`

或 Lua 动态 help：

- `help/help.lua`
- `help/tree.lua`

### 5.3 `overflow_templates/`

`overflow_templates/` 用于存放分页/截断等超限场景模板资产。

它的定位是：

- skill 私有超限模板目录
- 面向 runtime 与 host 之间的通用超限协议
- 不是普通回复消息模板目录

注意：

- 普通回复正文直接来自 Lua 返回的中间结果内容
- `overflow_templates/` 只服务于超限输出场景

### 5.4 `resources/`

`resources/` 用于存放 skill 私有静态资源。

例如：

- 规则文件
- 内置字典
- 示例输入
- 静态配置片段

这里的 `resources/` 是 skill 私有资产目录，不等同于 MCP `resource` 对象。

### 5.5 `licenses/`

`licenses/` 用于存放第三方资产、规则、依赖的许可证文件。

适用于：

- 第三方规则集
- 第三方词典
- 预置资源包
- 依赖组件说明

## 6. Skill 与 Entry 的命名规则

### 6.1 总体原则

推荐采用两段式命名：

- `skill namespace`
- `entry local name`

最终 canonical name 由 runtime 自动组合。

### 6.2 Canonical Name

canonical name 是 LuaSkills Core 中的标准名字。

推荐格式：

- `skill_id-entry_name`

例如：

- skill 目录或 `skill_id` 为 `vulcan-codekit`
- entry 局部名为 `ast-tree`

则最终 canonical name 为：

- `vulcan-codekit-ast-tree`

### 6.3 Local Name

entry 在 skill 包内部应优先声明局部名，而不是完整前缀名。

推荐：

- `ast-tree`

不再推荐：

- `vulcan-codekit-ast-tree` 直接作为 entry 本地声明名

因为 skill namespace 应由 runtime 自动补齐，而不应由 skill 作者在每个 entry 上重复书写。

### 6.4 Adapter Alias

不同宿主未必适合直接复用 runtime 的 canonical 名，因此建议区分：

因此建议区分：

- Core canonical name
- adapter exposed alias

例如：

- Core：`vulcan-codekit-ast-tree`
- 若发生同名冲突：`vulcan-codekit-ast-tree-2`
- 某个 adapter 也可以继续原样暴露该名字，或在 UI 层做二次显示映射

也就是说：

- runtime 内部维护 canonical name
- host/adapter 决定最终对外暴露名

### 6.5 目录名与标识符规则

skill 目录名、`skill_id` 与 `entries[].name` 都必须匹配同一套严格规则：

- `^[a-z]([a-z0-9-]*[a-z0-9])?$`

也就是说：

- 只允许小写英文字母、数字与短横线 `-`
- 不允许大写字母
- 不允许下划线与其他特殊符号
- 不允许数字开头
- 不允许以 `-` 结尾

宿主在搜索 skill 目录时，应直接忽略不匹配该规则的目录。

因此：

- `__demo`
- `MySkill`
- `2demo`
- `demo_kit`
- `demo-`

都不是合法的可自动加载 skill 目录。

### 6.6 冲突编号规则

runtime 先生成基础 canonical 名：

- `skill_id-entry_name`

若不同 skill 组合后出现相同名称，则按稳定顺序自动追加编号：

- 第一个：`skill_id-entry_name`
- 第二个：`skill_id-entry_name-2`
- 第三个：`skill_id-entry_name-3`

该规则是严格真相，不保留 `::` 兼容命名。

### 6.7 推荐规则总结

- skill 使用目录名或 `skill_id` 作为命名空间
- entry/tool 只写局部名
- runtime 自动生成 `skill_id-entry_name`
- 冲突时自动追加 `-2`、`-3`
- skill 目录名与标识符必须匹配 `^[a-z]([a-z0-9-]*[a-z0-9])?$`
- 不存在 `::` 兼容名

## 7. 目录中不应出现的 Core 真相对象

推荐不再把以下目录作为 LuaSkills Core 标准目录：

- `prompts/`
- `resources_templates/`
- `template/`
- `mcp_resources/`

原因：

- 这些更偏 MCP 或旧实现投影
- 会污染未来独立 runtime 的边界

如果某个宿主需要这些能力，应在 adapter 层自行映射生成，而不是写入 LuaSkills Core。

## 8. 与宿主职责的边界

### 8.1 Skill 包不决定以下内容

- 是否启用
- 是否安装
- 是否联网下载依赖
- 是否落盘超限文件
- 超限文件落盘位置
- runtime 配置注入方式
- 最终输出如何渲染

### 8.2 Skill 包只表达可移植真相

Skill 包应尽量只描述：

- 自己的能力
- 自己的代码
- 自己的帮助
- 自己的静态资源
- 自己的超限模板资产

这样才适合未来从当前仓库平滑抽离成独立 skill 仓库或被其他宿主复用。

## 9. 一句话结论

LuaSkills Core Package Layout 应固定为围绕 `skill.yaml + runtime/help/overflow_templates/resources/licenses` 的稳定结构，其中 skill 包只保存能力真相与运行时资产，不承载 MCP 协议对象，也不承载宿主状态与宿主生成产物；entry 命名则应采用 `skill_id-entry_name` 的 canonical 结构，由 runtime 自动组合，并在冲突时自动追加 `-2`、`-3`。
