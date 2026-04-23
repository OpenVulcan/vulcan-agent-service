# vulcan-mcp

`vulcan-mcp` 是 Vulcan 生态中的 **MCP 宿主与协议适配层**。  
它基于 [`vulcan-luaskills`](https://github.com/OpenVulcan/vulcan-luaskills) 提供：

- MCP 协议接入
- HTTP / gRPC / stdio 服务与本地调试模式
- 宿主配置读取与策略注入
- MCP 结果渲染、分页与截断处理
- system tools 的宿主包装
- LuaSkills 的统一 MCP 暴露

## 当前定位

当前架构已经拆成两层：

- `vulcan-luaskills`
  - LuaSkills 核运行时库
  - 负责 skill 加载、调用、help 树、`vulcan.*` / `vulcan.runtime.*` 注入
- `vulcan-mcp`
  - 宿主层
  - 负责 MCP 协议映射、客户端预算、工具配置、结果分页/截断、spill 文件落盘

一句话说：

**`vulcan-luaskills` 负责运行，`vulcan-mcp` 负责对外说话。**

## 主要能力

- 支持 MCP 多版本协议协商
- 支持 HTTP 服务模式、gRPC 服务模式、stdio 服务模式与本地调试模式
- 通过本地依赖接入 `vulcan-luaskills`
- 数据库访问固定走 `space_controller` 控制器模式
- 自动加载运行根下符合规则的 LuaSkills
- 把 skill entry 映射成 MCP tools
- 提供宿主封装的 strict help 工具
- 在宿主层处理工具结果的分页、截断与 spill 文件输出
- 支持宿主级 `client_budgets.yaml` 与 `tool_configs.yaml`

## 数据库访问模型

`vulcan-mcp` 采用 **controller-only** 产品形态：

- SQLite 只通过 `vldb-controller` 访问
- LanceDB 只通过 `vldb-controller` 访问
- MCP 宿主固定使用控制器模式，不暴露数据库 provider 模式切换

这样做的原因很直接：

- MCP 宿主可能被多开
- 多实例可能同时访问同一 workspace / user space 数据库
- 只有把数据库 ownership 收口到独立 controller 进程，才能真正避免直连数据库导致的文件锁冲突

因此运行时需要准备：

- `output/bin/vldb-controller(.exe)`
  - 可通过 `make deps host` 自动下载对应平台 release 产物，构建时会自动复制到这里

通用宿主工具依赖则位于：

- `output/bin/tools`

同时建议通过 `runtime/configs/config.yaml` 中的 `space_controller` 段配置：

- `endpoint`
- `auto_spawn`
- `executable_path`
- `process_mode`

其中如果需要修改 controller 默认端口，直接修改：

- `space_controller.endpoint`

例如把默认 `19801` 改成 `20333`：

```yaml
space_controller:
  endpoint: "http://127.0.0.1:20333"
```

其中有三个约束需要特别注意：

- `auto_spawn=true` 只能和**本地可拉起**的 controller endpoint 搭配使用
- 如果 `endpoint` 指向远端 controller，则必须改为 `auto_spawn=false`，并由外部保证 controller 已经启动
- `output/bin/vldb-controller(.exe)` 应尽量通过 `make deps host + make build` 生成；如果手工替换二进制，必须确保它与当前仓库锁定的 `vldb-controller-client` 使用同一 release tag，避免静默版本漂移

## 当前公开方式

### 1. LuaSkills

LuaSkills 是当前 MCP 对外暴露的主能力面。  
官方 skill 目前包括：

- `vulcan-lua`
- `vulcan-codekit`
- `vulcan-curl`
- `vulcan-ai-memory`
- `vulcan-work-memory`

`vulcan-ai-memory` 默认以 skill 形式加载。  
如果显式配置 `vmm_enable=true` 且提供 `vmm` gRPC 端点，宿主会跳过 `vulcan-ai-memory`，由 VMM 接管 AI 记忆能力。  
`vulcan-work-memory` 不属于 VMM gRPC 接管范围，会继续走 SQLite skill。

这些 skill 已迁移到新的目录结构：

```text
runtime/skills/<skill>/
├─ skill.yaml
├─ help/
├─ runtime/
├─ overflow_templates/
├─ resources/
└─ licenses/
```

### 2. Help 工具

help 由宿主包装为：

- `vulcan-help-list`
- `vulcan-help-detail`

其中：

- `vulcan-help-list`
  - 列出所有已注册 help 节点与简要说明
- `vulcan-help-detail`
  - 按 `skill + flow` 读取具体帮助节点

### 3. RunLua 暴露策略

`runlua` 的 system 能力保留在 `vulcan-luaskills` 内部与 `vulcan.runtime.lua.exec` 链路中，  
`vulcan-mcp` 通过 `vulcan-lua` skill 对外提供对应执行能力。

MCP 侧推荐通过 `vulcan-lua` skill 使用：

- `vulcan-lua-run`

### 4. Client Match Override

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
├─ main.rs                # 入口：配置、宿主构建、运行模式
├─ server.rs              # MCP server：协议处理、tool 注册、宿主包装
├─ luaskills_host.rs      # 宿主到 vulcan-luaskills 的接线与上下文映射
├─ tool_result_format.rs  # 宿主层结果渲染、分页与截断
├─ client_budget.rs       # MCP 客户端预算配置与解析
├─ tool_config.rs         # MCP 工具配置解析
├─ grpc_client.rs         # gRPC 客户端
├─ http_server.rs         # HTTP transport
├─ protocol.rs            # MCP 协议模型
└─ session.rs             # HTTP session

runtime/
├─ configs/               # 仓库内配置模板
├─ skills/                # 官方内建 LuaSkills 模板
├─ resources/             # 仓库内共享资源与公共模板
└─ examples/              # 示例 skill 与模板

output/
├─ configs/               # 构建同步后的运行配置
├─ skills/                # 实际运行使用的技能目录
├─ dependencies/          # 运行期共享/私有依赖
├─ databases/             # SQLite / LanceDB 数据目录
├─ resources/             # 实际运行使用的共享资源
├─ bin/                   # 宿主主程序与 controller
├─ libs/                  # 宿主提供通用原生动态库
├─ lua_packages/          # 宿主提供 Lua 包目录
├─ state/                 # 技能状态与安装状态
├─ temp/                  # 临时下载与渲染产物
└─ logs/                  # 日志目录
```

## 运行要求

### 配置文件

`vulcan-mcp` 仍然使用宿主配置文件，例如：

- `config.yaml`
- `client_budgets.yaml`
- `tool_configs.yaml`

这些配置属于宿主层，**不会进入 `vulcan-luaskills` 库内部读取逻辑**。

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
cargo build
```

### 检查

```bash
cargo check
```

### 测试

```bash
cargo test
```

### stdio 启动

当宿主需要以标准输入输出方式被外部 MCP 客户端直接拉起时，可使用：

```bash
cargo run -- --stdio
```

构建产物则可直接执行：

```bash
./output/bin/vulcan-mcp --stdio
```

## 与 `vulcan-luaskills` 的关系

当前仓库通过本地 path dependency 引用：

```toml
vulcan-luaskills = { path = "../vulcan-luaskills" }
```

后续独立发布后，可以切换为远程仓库依赖或版本依赖。  
但职责边界不变：

- `vulcan-luaskills`：运行时库
- `vulcan-mcp`：MCP 宿主

## 运行目录约定

- `runtime/` 用于存放仓库内的基础模板文件
- `output/` 是实际运行根，构建时会把 `runtime/configs`、`runtime/resources`、`runtime/skills` 同步进去
- 运行期产生的：
  - `dependencies`
  - `databases`
  - `temp`
  - `logs`
  - `libs`
  都应位于 `output/` 下
- `output/bin` 只用于宿主级主程序与 controller 这类系统可执行文件
- `output/bin/tools` 用于共享命令行工具依赖，例如 `rg`、`ast-grep`
- `output/bin/tools` 不是数据库 controller 目录；`vldb-controller(.exe)` 固定放在 `output/bin/`
- `output/libs` 用于宿主提供通用原生依赖

## 后续方向

- 继续完善 system tools 的宿主枚举与包装模型
- 进一步收紧 `system` 与 `skill` 的公开边界
- 推进 `vulcan-luaskills` 的 FFI 导出形态
- 逐步把官方 skill 拆分为独立仓库

## License

MIT
