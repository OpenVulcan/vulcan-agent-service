# vulcan-agent-service

`vulcan-agent-service` 是 Vulcan 生态中的 **Agent 服务中枢与协议适配层**。  
它基于 [`luaskills`](https://github.com/LuaSkills/luaskills) 提供：

- 面向 Trae / VSCode / CodeBuddy 等客户端的 MCP 接入
- 面向 openclaw / opencode / qwencode / Hermes 等执行端的 gRPC 接入
- 与 `vulcan-memory-mesh` 的统一交互
- HTTP / gRPC / stdio 服务与本地调试模式
- 宿主配置读取与策略注入
- MCP 结果渲染、分页与截断处理
- system tools 的宿主包装
- LuaSkills 与宿主能力的统一对外暴露

## 当前定位

当前架构已经拆成两层：

- `luaskills`
  - LuaSkills 核运行时库
  - 负责 skill 加载、调用、help 树、`vulcan.*` / `vulcan.runtime.*` 注入
- `vulcan-agent-service`
  - 宿主与服务中枢层
  - 负责 MCP / gRPC 协议映射、客户端预算、工具配置、结果分页/截断、spill 文件落盘，以及与 `vulcan-memory-mesh` 的协同

一句话说：

**`luaskills` 负责运行，`vulcan-agent-service` 负责对外接入、协调与转发。**

## 主要能力

- 支持 MCP 多版本协议协商
- 支持 HTTP 服务模式、gRPC 服务模式、stdio 服务模式与本地调试模式
- 通过 Cargo 原生版本依赖接入 `luaskills`
- 数据库访问固定走 `space_controller` 控制器模式
- 自动加载运行根下符合规则的 LuaSkills
- 把 skill entry 映射成 MCP tools
- 提供宿主封装的 strict help 工具与统一 `runtime-config` 配置工具
- 在宿主层处理工具结果的分页、截断与 spill 文件输出
- 支持宿主级 `client_budgets.yaml`、`tool_configs.yaml`、`model_config.yaml` 与统一 Skill 运行时配置
- 提供统一 gRPC 服务面：兼容型能力走 `McpService`，LuaSkills 稳定能力走显式 RPC，动态 entry 走 `CallTool`，Host Adapter / VMM 走独立 service

接口文档：

- [LuaSkills gRPC 接口说明](docs/grpc_luaskills_api_cn.md)
- [LuaSkills 模型能力宿主接口说明](docs/luaskills_models_host_api_cn.md)

## 数据库访问模型

`vulcan-agent-service` 采用 **controller-only** 产品形态：

- SQLite 只通过 `vldb-controller` 访问
- LanceDB 只通过 `vldb-controller` 访问
- 服务中枢固定使用控制器模式，不暴露数据库 provider 模式切换

这样做的原因很直接：

- `vulcan-agent-service` 可能被多开，且不同 MCP / gRPC / IDE 入口可能并发落到同一运行根
- 多实例可能同时访问同一 workspace / user space 数据库
- 只有把数据库 ownership 收口到独立 controller 进程，才能真正避免直连数据库导致的文件锁冲突

因此运行时需要准备：

- `output/lua_runtime/bin/vldb-controller(.exe)`
  - 可通过 `make deps` 自动下载对应平台 release 产物，构建时会自动复制到这里

通用宿主工具依赖则位于：

- `output/lua_runtime/bin`

同时建议通过 `runtime/configs/config.yaml` 中的 `space_controller` 段配置：

- `endpoint`
- `auto_spawn`
- `executable_path`
- `process_mode`

其中如果需要修改 controller 默认端口，直接修改：

- `space_controller.endpoint`

例如把默认 `19801` 改成 `20333`：

```yaml
format_version: 1
space_controller:
  endpoint: "http://127.0.0.1:20333"
```

其中有三个约束需要特别注意：

- `auto_spawn=true` 只能和**本地可拉起**的 controller endpoint 搭配使用
- 如果 `endpoint` 指向远端 controller，则必须改为 `auto_spawn=false`，并由外部保证 controller 已经启动
- `output/lua_runtime/bin/vldb-controller(.exe)` 应尽量通过 `make deps + make build` 生成；如果手工替换二进制，必须确保它与当前仓库锁定的 `vldb-controller-client` 使用同一 release tag，避免静默版本漂移

## 当前对外服务面

`vulcan-agent-service` 当前同时维护三类对外服务面：

- MCP / IDE 适配面：面向 Trae / VSCode / CodeBuddy 等客户端，支持 stdio、HTTP / SSE。
- 统一 gRPC 服务面：面向 openclaw / opencode / qwencode / Hermes-agent 等执行端，同一端口挂载 `McpService`、`LuaSkillsService`、`HostAdapterService` 与 `vmm.v1.VmmService`。
- 宿主包装能力面：把 LuaSkills、help、skill config、分页/截断与宿主级 system tools 统一投影成可消费接口。

## 服务模式

`vulcan-agent-service` 现在已经提供统一的跨平台 `service` 命令面，用于把当前宿主注册为平台原生长期托管服务。

当前首版支持矩阵：

- Windows：`SCM`
- Linux：`systemd`
- macOS：`launchd`

统一命令面如下：

```text
vulcan-agent-service service install [--service-name <name>]
vulcan-agent-service service uninstall [--service-name <name>]
vulcan-agent-service service start [--service-name <name>]
vulcan-agent-service service stop [--service-name <name>]
vulcan-agent-service service restart [--service-name <name>]
vulcan-agent-service service status [--service-name <name>]
vulcan-agent-service service print-definition [--service-name <name>]
```

服务管理器实际拉起的是内部宿主入口：

```text
vulcan-agent-service service run [--service-name <name>]
```

有两条约束需要特别注意：

1. `service install` / `service run` 默认按当前宿主布局解析运行根，优先接受 `<runtime_root>`、`<runtime_root>/bin`、`<cwd>/output` 这类标准目录；确有需要时也可以继续显式传入 `--runtime-root` 覆盖。
2. 默认服务名为 `VulcanAgentService`；若传入 `--service-name`，则会以该名称注册，并且后续 `start/stop/status/uninstall` 也应使用同一名称。
3. 服务模式强烈建议显式配置 `skill_roots`；否则默认 `USER` 技能目录会跟随服务账户变化，而不是跟随当前登录开发者用户变化。

推荐安装示例：

```text
vulcan-agent-service service install --start
```

Linux 用户级安装示例：

```text
vulcan-agent-service service install --service-name vulcan-agent-service --scope user --startup manual
```

macOS 预览当前定义而不安装：

```text
vulcan-agent-service service print-definition
```

### 1. LuaSkills

LuaSkills 是当前对外的核心能力面。  
在 MCP 面上它表现为 tools，在 gRPC 面上则由 `LuaSkillsService` 的显式 RPC 与 `CallTool` 动态入口共同暴露。  
官方 skill 目前包括：

- `vulcan-lua`
- `vulcan-codekit`
- `vulcan-curl`
- `vulcan-file`
- `vulcan-ai-memory`
- `vulcan-workmem`
- `vulcan-testkit`

`vulcan-ai-memory` 默认以 skill 形式加载。  
如果显式配置 `vmm_enable=true` 且提供 `vmm` gRPC 端点，宿主会跳过 `vulcan-ai-memory`，由 VMM 接管 AI 记忆能力。  
`vulcan-workmem` 不属于 VMM gRPC 接管范围，会继续走 SQLite skill。

这些 skill 已迁移到新的目录结构：

```text
runtime/lua_runtime/skills/<skill>/
├─ skill.yaml
├─ help/
├─ runtime/
├─ overflow_templates/
├─ resources/
└─ licenses/
```

### 2. Help 工具

当 Lua engine 成功加载了运行期 skill 后，help 会由服务中枢包装为：

- `vulcan-help-list`
- `vulcan-help-detail`

其中：

- `vulcan-help-list`
  - 列出所有已注册 help 节点与简要说明
- `vulcan-help-detail`
  - 按 `skill + flow` 读取具体帮助节点

### 3. Runtime Config 工具

LuaSkills 0.5.7 的技能包配置 dispatcher 由服务中枢直接暴露为：

- `runtime-config`

其中：

- 调用约束
  - 工具整体标记为需要用户确认，因为同一标准入口既可披露原始值，也可修改持久化配置
  - 宿主只负责授权边界和传输，不重复实现上游声明、校验、revision、CAS 或错误码语义
- `action`
  - 支持 `describe` / `validate` / `list` / `get` / `set` / `delete` / `refresh`
- 写入契约
  - 支持单键 `key + value` 或类型化批量 `values`
  - `expected_revision` 用于 CAS；revision 使用规范十进制字符串
  - 只能写入 `skill.yaml` 顶层 `config` 已声明且校验通过的键
- 返回结果
  - 原样返回上游稳定 JSON 包络：`ok`、`action`、`result`、`error`
  - Lua skill 内部的 `vulcan.config.*` 与宿主 `runtime-config` 共用同一套声明、路由、缓存和存储

### 4. RunLua 暴露策略

`runlua` 的 system 能力保留在 `luaskills` 内部与 `vulcan.runtime.lua.exec` 链路中，  
`vulcan-agent-service` 通过 `vulcan-lua` skill 对外提供对应执行能力。

在对外 tool 暴露面上，推荐通过 `vulcan-lua` skill 使用：

- `vulcan-lua-run`

当前隔离 `vulcan.runtime.lua.exec` 已对接 `luaskills` 的独立 `runlua` VM 池：

- 默认值为 `min_size=1 / max_size=4 / idle_ttl_secs=60`
- 宿主配置通过 `config.yaml` 中的 `runlua_pool_config` 透传到 `LuaRuntimeHostOptions.runlua_pool_config`
- 该池只影响隔离 `runlua` 执行链，不改变普通 skill VM 主池和普通 `run_lua` 主池行为

### 5. Client Match Override

当前客户端预算匹配默认使用 MCP 请求里上报的 `clientInfo.name`。  
若某些宿主集成上报的是通用壳名称，例如 `mcphost`、`Copilot` 等，而不是实际产品名，可通过环境变量强制覆盖：

- `VULCAN_CLIENT_MATCH_NAME`

只要该环境变量存在且非空，宿主就会用这个值替换原本获取到的客户端名参与匹配。  
`stdio` 模式推荐直接使用该环境变量。  
`http` / `sse` 模式推荐通过请求头传递：

- `Vulcan-Client-Match-Name`

只要该请求头存在且非空，宿主就会优先用它替换当前请求实际拿到的客户端名参与匹配。  
覆盖优先级为：

1. `Vulcan-Client-Match-Name`
2. `VULCAN_CLIENT_MATCH_NAME`
3. `clientInfo.name`

## 仓库结构

```text
src/
├─ main.rs                # 入口：配置装配、服务启动与运行模式选择
├─ bootstrap/             # CLI、运行根初始化、预载与启动流程
├─ config/                # 宿主配置、预算、模型与工具配置解析
├─ luaskills/             # luaskills 接线、tool 映射、上下文与运行时路径
├─ host_core/             # 服务中枢核心：runtime、host tools、skill 管理、投影与调度
├─ transport/             # 对外协议层
│  ├─ mcp/                # MCP 协议、dispatcher、视图与 tool 暴露
│  ├─ grpc/               # McpService / LuaSkillsService / HostAdapterService / VMM relay
│  ├─ http/               # HTTP / SSE 入口
│  └─ stdio/              # stdio 入口
├─ backends/              # VMM 等外部后端适配
├─ model_provider/        # 模型能力桥接与回调注册
└─ support/               # 结果格式化、日志、运行时上下文与临时文件维护

runtime/
├─ configs/               # 宿主配置模板，仅供主程序读取
└─ lua_runtime/           # 完整 LuaSkills 运行时源码镜像
   ├─ bin/                # controller 与宿主提供工具
   ├─ skills/             # 官方内建 LuaSkills
   ├─ dependencies/       # Skill 依赖、受管发行包与环境
   ├─ databases/          # SQLite / LanceDB 数据目录
   ├─ resources/          # LuaSkills 共享资源与公共模板
   ├─ libs/               # FFI 与原生动态库
   ├─ lua_packages/       # Lua 包目录
   ├─ state/              # 技能状态与安装记录
   └─ temp/               # LuaSkills 临时文件

output/
├─ bin/                   # release 宿主主程序
├─ debug/                 # debug 宿主主程序
├─ configs/               # 构建同步后的宿主配置
├─ logs/                  # 宿主日志目录
└─ lua_runtime/           # 实际运行使用的完整 LuaSkills 包
   ├─ bin/                # vldb-controller 与宿主提供工具
   ├─ skills/             # ROOT 系统技能
   ├─ dependencies/       # Skill 依赖、runtimes 与 envs
   ├─ databases/          # SQLite / LanceDB 数据目录
   ├─ resources/          # LuaSkills 共享资源
   ├─ libs/               # FFI 与原生动态库
   ├─ lua_packages/       # Lua 包目录
   ├─ state/              # 技能状态与安装记录
   └─ temp/               # LuaSkills 临时文件
```

## 运行要求

### 配置文件

`vulcan-agent-service` 使用以下宿主配置文件：

- CLI 入口使用 `--runtime-root` 或标准运行目录自动发现。

- `config.yaml`
- `client_budgets.yaml`
- `tool_configs.yaml`
- `model_config.yaml`

其中：

- 四份 YAML 都必须显式声明 `format_version: 1`；缺失版本、版本不匹配和各自固定结构中的未知字段都会直接导致加载失败
- `config.yaml` / `client_budgets.yaml` / `tool_configs.yaml` / `model_config.yaml`
  - 属于宿主层配置，由 `vulcan-agent-service` 自己读取
  - `tool_configs.yaml` 的顶层结构固定为 `format_version` 与 `skills`，具体技能配置放在 `skills.<skill_name>` 下
  - `tool_configs.yaml` 的 `bytes_per_token` 与 `unlimited_bytes_cap` 必须写成 YAML 无符号整数；字符串、负数、浮点数、布尔值、`null` 与数组会在预载或热重载时被拒绝
- 技能包配置
  - `config.yaml` 可通过 `skill_config_root` 指定绝对用户级目录
  - 未配置时使用 `%USERPROFILE%\.vulcan\agent-service\config`（Windows）或 `$HOME/.vulcan/agent-service/config`（Unix）
  - LuaSkills 分别持久化到 `<skill_config_root>/skills/config.json` 与 `<skill_config_root>/system-skills/config.json`
  - ROOT 技能固定进入 `system-skills`，其他正式层进入 `skills`
  - 构建和打包不会复制、清空或覆盖该用户配置根
  - 宿主通过标准 `runtime-config` MCP 工具和 `RuntimeConfig` gRPC RPC 暴露上游严格 JSON 契约

### Lua VM 池配置

当前 `config.yaml` 中有两套互不干扰的 VM 池参数：

- `lua_vm_pool_*`
  - 普通 skill 调用与普通 `run_lua` 主池
- `runlua_pool_config`
  - 隔离 `vulcan.runtime.lua.exec` 专用池

默认模板如下：

```yaml
format_version: 1
lua_vm_pool_min_size: 2
lua_vm_pool_max_size: 8
lua_vm_pool_idle_ttl_secs: 600

runlua_pool_config:
  min_size: 1
  max_size: 4
  idle_ttl_secs: 60
```

其中 `runlua_pool_config` 会映射到 `LuaRuntimeHostOptions.runlua_pool_config`；省略该可选段时使用 `luaskills` 上游默认值。

### Skill 目录规则

当前 LuaSkills 目录名与 `skill_id` 采用严格规则：

```regex
^[a-z]([a-z0-9-]*[a-z0-9])?$
```

也就是说：

- 只允许小写字母、数字、连字符 `-`
- 不能以数字开头
- 不能以 `-` 结尾
- 不支持大写与特殊符号

## 本地开发

### 构建

```bash
make deps
make build
```

依赖可按域单独准备：

```bash
make deps host       # 宿主原生依赖
make deps lua        # 官方 LuaSkills 运行时包
make deps managed    # 受管 Python + Node 发行包
make deps python     # 仅受管 Python
make deps node       # 仅受管 Node + pnpm
```

受管运行时当前锁定为 Python 3.14.6、uv 0.11.28、Node 24.18.0 与 pnpm 11.11.0。拉取脚本会验证上游校验和/完整性，并由构建流程复制到 `output/lua_runtime/dependencies/runtimes`。

### 检查

```bash
cargo check
```

### 测试

```bash
cargo test
```

### stdio 启动

当直接从仓库源码目录启动时，需要显式指定运行根，例如：

```bash
cargo run -- --stdio --runtime-root output
```

如果直接执行构建产物，则继续使用运行目录自动发现：

```bash
./output/bin/vulcan-agent-service --stdio
```

### ROOT Skill 管理命令

以下命令只执行本地 ROOT 层 skill 管理动作，执行完成后直接退出，不会启动 HTTP、gRPC 或 stdio 服务。

安装指定 skill 到 ROOT 层：

```bash
cargo run -- --install-root-skill LuaSkills/vulcan-codekit --runtime-root output
```

如果直接执行构建产物：

```bash
./output/bin/vulcan-agent-service --install-root-skill LuaSkills/vulcan-codekit --runtime-root output
```

安装时也可以显式指定来源类型：

```bash
./output/bin/vulcan-agent-service --install-root-skill LuaSkills/vulcan-codekit --source-type github --runtime-root output
```

更新 ROOT 层全部受管 skill：

```bash
cargo run -- --update-root-skills --runtime-root output
```

如果直接执行构建产物：

```bash
./output/bin/vulcan-agent-service --update-root-skills --runtime-root output
```

`--update-root-skills` 只会更新带受管安装记录的 ROOT skill；手工放入 ROOT 但没有安装记录的目录会被跳过。

## 与 `luaskills` 的关系

当前仓库通过 Cargo 原生版本依赖引用：

```toml
luaskills = "0.5.7"
```

相关地址：

- 仓库：<https://github.com/LuaSkills/luaskills>
- Cargo：<https://crates.io/crates/luaskills>
- Runtime packages：<https://github.com/LuaSkills/luaskills-packages>

当前 `0.5.7` 对接下，`luaskills` 主仓库发布 Rust crate、FFI SDK、demo 与调试工具；
Lua runtime packages 与原生依赖包已经独立到 `luaskills-packages` 发布，
本仓库里的依赖拉取脚本也按这个拆分后的发布模型工作。

`0.5.7` 保持固定 `runtime_root`、受管 Python/Node 发行根、可写环境根与 Worker/持久会话资源策略，并将 Rust controller client 与受管 VLDB 运行时统一对齐到 `vldb-controller 0.2.3` 和 `vldb-sqlite 0.1.6`。技能包配置使用显式用户级 `skill_config_root`、普通与 ROOT 系统双存储、类型化声明、revision、CAS、缓存监听和标准 `runtime-config` dispatcher。

本次从 `0.5.5` 同步到 `0.5.7`，包含上游的运行时、缓存、文件监听与 FFI 修复；`0.5.7` 修复原子替换配置文件时的监听路由，并恢复 Linux ARM64 技能包业务校验。运行时资源包继续使用独立的 `luaskills-packages` `0.1` 版本线。详见 [上游发布说明](https://github.com/LuaSkills/luaskills/releases/tag/v0.5.7)。

本仓库的 PowerShell / Shell 受管 Python、Node 与包管理器拉取脚本同步了上游清单复用校验：只有版本、平台、运行时类型与根内入口文件均有效时才复用安装，否则重新安装；默认暂存目录与发行根仍位于 `third_party`。

同时，宿主直接复用 LuaSkills 导出的工具说明文本；
`vulcan-agent-service` 现在直接复用 `luaskills` 导出的 entry description、
parameter description 与 final AI-facing `input_schema`，
不再额外做宿主侧二次拼接或格式修正。

但职责边界不变：

- `luaskills`：运行时库
- `vulcan-agent-service`：Agent 服务中枢与多协议宿主层

## 运行目录约定

- `runtime/configs` 只保存宿主配置模板；`runtime/lua_runtime` 保存完整 LuaSkills 源码运行时镜像
- `output/` 是应用根，只允许保留宿主主程序、`configs`、`logs` 与 `lua_runtime` 容器
- `output/bin` 与 `output/debug` 只保存宿主主程序，不再承载 controller 或 Lua 工具
- `output/lua_runtime` 是唯一 LuaSkills 根；`skills`、`dependencies`、`databases`、`temp`、`libs`、`lua_packages`、`resources`、`state` 全部位于其下
- `vldb-controller(.exe)` 与共享宿主工具固定放在 `output/lua_runtime/bin`
- 受管 Python/Node 发行包固定放在 `output/lua_runtime/dependencies/runtimes`，可写环境固定放在 `output/lua_runtime/dependencies/envs`
- 构建只生成并同步上述当前目录，不扫描或改写当前布局以外的输出目录

## 后续方向

- 继续完善 system tools 的宿主枚举与包装模型
- 进一步收紧 `system` 与 `skill` 的公开边界
- 推进 `luaskills` 的 FFI 导出形态
- 逐步把官方 skill 拆分为独立仓库

## License

MIT
