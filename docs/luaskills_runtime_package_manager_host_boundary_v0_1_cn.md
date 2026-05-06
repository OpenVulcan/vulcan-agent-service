# LuaSkills Runtime / Package Manager / Host 三层职责边界 v0.1 草案

## 1. 文档定位

本文用于定义未来 LuaSkills 体系中的三层职责边界：

- `runtime`
- `package manager`
- `host`

本文的目标不是给出最终实现细节，而是先把边界钉住，避免后续在拆分 `luaskills` 时继续混淆：

- 执行职责
- 依赖安装职责
- 宿主接入职责

本文默认以下总体方向成立：

- 先调整 LuaSkills / MCP Skill 格式
- 再拆分 `luaskills`
- `vulcan-agent-service` 最终是 `luaskills` 的接入层之一

## 2. 设计目标

本文试图解决以下问题：

- Lua runtime 是否应该自己处理依赖下载与安装
- MCP 是否应该继续承担 skill 真相来源
- `vulcan-agent-service`、未来的 `vulcan-grpc`、以及嵌入式宿主如何共享同一套 skill runtime
- skill 生命周期、安装策略、宿主 UI 与策略控制应该落在哪一层

## 3. 三层模型总览

推荐采用以下三层结构：

### 3.1 Runtime

`runtime` 指 `luaskills` 或其核心执行引擎。

它负责：

- 加载 skill
- 解析 skill 入口
- 绑定宿主能力
- 调用 Lua entry
- 暴露 help / list / info 所需的最小运行时信息
- 维护 skill 启用、降级、失败等运行时状态

它不负责：

- 下载依赖
- 安装依赖
- 选择下载源
- 决定是否允许联网安装
- 具体宿主 UI / CLI 交互
- MCP / gRPC 协议对象建模

### 3.2 Package Manager

`package manager` 指 LuaSkills 的包与依赖管理层，可独立实现为单独库或由宿主承载。

它负责：

- source / registry 解析
- skill 安装 / 升级 / 卸载
- Lua 依赖解析
- native / FFI 依赖解析
- provider 依赖解析
- lockfile / 校验 / 缓存

它不负责：

- skill 实际执行
- Lua VM 生命周期
- 协议层对外暴露
- skill 调用时的上下文传递

### 3.3 Host

`host` 指具体承载 LuaSkills 的产品或运行环境。

示例：

- `vulcan-agent-service`
- 未来 `vulcan-grpc`
- 任何嵌入 `luaskills` 的 Agent 宿主

它负责：

- 创建 runtime
- 调用 package manager
- 提供宿主能力与 provider
- 暴露 CLI / UI / API
- 决定默认安装哪些 skill 与 provider
- 决定安全策略与启用策略

它不应负责：

- 重写 skill 核心执行逻辑
- 直接成为 skill 真相来源

## 4. Runtime 的职责边界

### 4.1 Runtime 应负责的内容

推荐 `runtime` 只负责以下内容：

- Skill Descriptor 解析后的加载
- entry 调用
- Lua VM / VM pool 管理
- 宿主注入 namespace 与 capability 绑定
- provider 绑定结果的运行时使用
- `enabled / degraded / disabled_*` 状态建模
- skill help 元信息读取
- skill list / info 的运行时侧数据生成

### 4.2 Runtime 不应负责的内容

推荐明确禁止 `runtime` 负责以下内容：

- GitHub 下载
- source / registry 选择
- zip / tar.gz 解压安装
- 依赖版本决策
- 安装时是否默认勾选哪些 skill
- 产品安装器逻辑

### 4.3 Runtime 的输入

推荐 runtime 只吃以下输入：

- 已解析的 skill 描述
- 宿主提供的 capability / provider 绑定
- skill 调用请求
- 宿主策略结果

也就是说，runtime 不应以“仓库目录布局细节”或“下载后安装流程细节”为真相来源。

## 5. Package Manager 的职责边界

### 5.1 Package Manager 应负责的内容

推荐 `package manager` 负责：

- 解析 `skill` 包来源
- 解析 `dependencies`
- 安装 / 升级 / 卸载 skill
- 下载安装 Lua / native / provider 相关依赖
- 维护安装状态与缓存
- 生成 runtime 可消费的已安装 skill 目录或描述结果

### 5.2 Package Manager 不应负责的内容

推荐禁止 `package manager` 负责：

- Lua entry 执行
- skill 实际调用
- MCP / gRPC 协议适配
- skill 调用时的上下文构造

### 5.3 为什么依赖管理不应放进 Runtime

因为依赖管理天然涉及：

- 是否允许联网
- 走哪个源
- 是否允许安装 native 二进制
- 安装路径与缓存路径
- 企业镜像 / 私有源 / 校验策略

这些都属于产品策略层，而不是 Lua runtime 的执行职责。

## 6. Host 的职责边界

### 6.1 Host 应负责的内容

推荐 `host` 负责：

- 初始化 runtime
- 调用 package manager
- 解析并提供宿主 capability
- 提供 CLI / UI / API
- 决定默认安装 profile
- 决定默认启用哪些 bundled core skills
- 决定安全策略、联网策略、自动安装策略

### 6.2 Host 不应负责的内容

Host 不应把以下内容重新内嵌为私有逻辑：

- skill 的真实格式定义
- skill entry 真相模型
- runtime 内部执行语义

换句话说：

Host 是接入层，不是 LuaSkills Core 真相来源。

## 7. 与 MCP 的关系

### 7.1 MCP 是接入层，不是 skill 真相层

未来推荐关系应为：

- `luaskills` 是 runtime
- `vulcan-agent-service` 是统一服务中枢 / host-adapter

当前仓库中的 `vulcan-agent-service` 不只是一层 MCP 壳，还承担 gRPC 暴露、VMM 协调与宿主能力投影。

MCP 中的：

- `tool`
- `resource`
- `prompt`
- `resource_template`

不应继续作为 LuaSkills Core 的真实对象模型。

### 7.2 MCP 的职责

MCP 层更适合负责：

- 把 LuaSkills entry 映射为 MCP 对象
- 暴露 `list` / `call` / `help`
- 处理 MCP 协议细节

而不应继续承担：

- skill 包规范定义
- skill 依赖安装规范定义
- runtime 真相定义

## 8. 运行时与宿主能力的关系

### 8.1 Runtime 负责注册标准能力入口

例如：

- `vulcan.fs.*`
- `vulcan.process.exec`
- `vulcan.json.encode`
- `vulcan.json.decode`
- `vulcan.call`

### 8.2 Host 负责注入扩展能力

宿主可以注入：

- `tare.xxx`
- 其他私有 namespace

但 runtime 只负责：

- 提供注册机制
- 暴露 capability 结果

而不负责为宿主私有扩展做产品决策。

## 9. Skill 状态在三层中的归属

### 9.1 Runtime 状态

runtime 应维护：

- `enabled`
- `degraded`
- `disabled_missing_capability`
- `disabled_runtime_error`

这类与实际装载和执行有关的状态。

### 9.2 Package Manager 状态

package manager 应维护：

- `installed`
- `not_installed`
- `upgrade_available`
- `install_failed`
- `dependency_unresolved`

### 9.3 Host 展示状态

host 最终面对用户时，可以把两者组合为：

- 安装状态
- 启用状态
- 降级状态
- 原因解释

但 host 不应伪造 runtime 真相。

## 10. 当前项目映射

结合当前仓库现状，可做如下映射。

### 10.1 更接近 Runtime 的模块

- 已迁移到 `luaskills` 仓库的 `lua_engine.rs`
- 已迁移到 `luaskills` 仓库的 `lua_skill.rs`
- 已迁移到 `luaskills` 仓库的 `sqlite_host.rs`
- 已迁移到 `luaskills` 仓库的 `lancedb_host.rs`

这些模块更像未来 `luaskills` 的核心部分。

### 10.2 更接近 Package Manager 的模块

- 已从主仓拆出的 `skill_dependency.rs`

该模块在迁移前位于主仓库中，但长期更适合作为上层宿主能力或独立 package manager 的一部分，而不是 runtime 本体。

### 10.3 更接近 Host / Adapter 的模块

- `src/main.rs`
- `src/bootstrap/`
- `src/host_core/`
- `src/transport/`
- `src/config/`

这些模块主要承担：

- 启动入口
- 接入协议
- 网络服务
- 配置读取

更接近 host / adapter。
旧阶段文档中提到的 `src/server.rs / src/http_server.rs / src/grpc_server.rs`，在当前仓库里已经分别拆分并收口到了上述目录。

## 11. 为什么现在更适合先改格式，再拆 Runtime

在当前阶段，推荐先调整 skill 格式，而不是立刻拆分 runtime。

原因：

- 一边拆 runtime，一边改协议，调试边界会很模糊
- 很难判断问题是由“格式变化”还是“架构拆分”引起
- 当前 skill 目录结构、help 机制、capability/provider 规则都还在收敛中

因此推荐顺序是：

1. 先调整 skill 格式
2. 在现有 `vulcan-agent-service` 中验证新格式
3. 再拆分 `luaskills`

## 12. 推荐分阶段路线

### 12.1 第一阶段

先定义并验证：

- 新 skill 包格式
- help 机制
- capability / provider / degradation
- enable / disable / status 模型

### 12.2 第二阶段

在当前主仓库中完成：

- 新格式兼容
- 官方 skill 迁移
- `vulcan-codekit` 验证

### 12.3 第三阶段

再正式拆分：

- `luaskills`
- `vulcan-agent-service`
- 未来 `vulcan-grpc`

### 12.4 第四阶段

补上：

- package manager
- install / uninstall / reload
- registry / source
- 生命周期管理

## 13. 一句话原则

**Runtime 只负责执行，Package Manager 只负责依赖与安装，Host 只负责接入、策略与体验；任何一层都不应越级成为另一层的真相来源。**
