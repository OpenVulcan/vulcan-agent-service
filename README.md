# vulcan-mcp

`vulcan-mcp` 是 Vulcan 生态中的 **MCP 宿主与协议适配层**。  
它基于 [`luaskills`](https://github.com/LuaSkills/luaskills) 提供：

- MCP 协议接入
- HTTP / gRPC / stdio 服务与本地调试模式
- 宿主配置读取与策略注入
- MCP 结果渲染、分页与截断处理
- system tools 的宿主包装
- LuaSkills 的统一 MCP 暴露

## 当前定位

当前架构已经拆成两层：

- `luaskills`
  - LuaSkills 核运行时库
  - 负责 skill 加载、调用、help 树、`vulcan.*` / `vulcan.runtime.*` 注入
- `vulcan-mcp`
  - 宿主层
  - 负责 MCP 协议映射、客户端预算、工具配置、结果分页/截断、spill 文件落盘

一句话说：

**`luaskills` 负责运行，`vulcan-mcp` 负责对外说话。**

## 主要能力

- 支持 MCP 多版本协议协商
- 支持 HTTP 服务模式、gRPC 服务模式、stdio 服务模式与本地调试模式
- 通过 Cargo 原生版本依赖接入 `luaskills`
- 数据库访问固定走 `space_controller` 控制器模式
- 自动加载运行根下符合规则的 LuaSkills
- 把 skill entry 映射成 MCP tools
- 提供宿主封装的 strict help 工具与统一 `luaskill-config` 配置工具
- 在宿主层处理工具结果的分页、截断与 spill 文件输出
- 支持宿主级 `client_budgets.yaml`、`tool_configs.yaml`、`model_config.yaml` 与统一 Skill 运行时配置
- 提供 LuaSkills 专用 gRPC 服务面，稳定能力走显式 RPC，动态 entry 走 `CallTool`

接口文档：

- [LuaSkills gRPC 接口说明](docs/grpc_luaskills_api_cn.md)
- [LuaSkills 模型能力宿主接口说明](docs/luaskills_models_host_api_cn.md)

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
  - 可通过 `make deps` 自动下载对应平台 release 产物，构建时会自动复制到这里

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
- `output/bin/vldb-controller(.exe)` 应尽量通过 `make deps + make build` 生成；如果手工替换二进制，必须确保它与当前仓库锁定的 `vldb-controller-client` 使用同一 release tag，避免静默版本漂移

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

当 Lua engine 成功加载了运行期 skill 后，help 会由宿主包装为：

- `vulcan-help-list`
- `vulcan-help-detail`

其中：

- `vulcan-help-list`
  - 列出所有已注册 help 节点与简要说明
- `vulcan-help-detail`
  - 按 `skill + flow` 读取具体帮助节点

### 3. Luaskill Config 工具

统一 Skill 配置文件会由宿主额外包装为：

- `luaskill-config`

其中：

- 调用约束
  - 只有用户明确要求查看或修改 LuaSkill 配置时，才应调用该工具
  - 执行 `set` / `delete` 后，调用方必须向用户明确回报受影响的 `skill_id` / `key` 与最终工具结果
- `action`
  - 支持 `list` / `get` / `set` / `delete`
- `skill_id`
  - `list` 时可选，用于只查看单个 skill 命名空间
  - `get` / `set` / `delete` 时必填
- `key`
  - `get` / `set` / `delete` 时必填
- `value`
  - `set` 时必填
- 返回结果
  - 返回面向 AI 的纯文本结果，不再附带 JSON 代码块
  - `list` 为空时会明确提示无配置；非空时按 `skill_id -> key/value` 分组展示
  - 与 Lua skill 内部的 `vulcan.config.*` 共用同一份宿主统一运行期配置

### 4. RunLua 暴露策略

`runlua` 的 system 能力保留在 `luaskills` 内部与 `vulcan.runtime.lua.exec` 链路中，  
`vulcan-mcp` 通过 `vulcan-lua` skill 对外提供对应执行能力。

MCP 侧推荐通过 `vulcan-lua` skill 使用：

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
├─ main.rs                # 入口：配置、宿主构建、运行模式
├─ server.rs              # MCP server：协议处理、tool 注册、宿主包装
├─ luaskills_host.rs      # 宿主到 luaskills 的接线与上下文映射
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

- CLI 入口已收敛为 `--runtime-root` 或标准运行目录自动发现，不再支持 `--config`

- `config.yaml`
- `client_budgets.yaml`
- `tool_configs.yaml`
- `skill_config.json`

其中：

- `config.yaml` / `client_budgets.yaml` / `tool_configs.yaml`
  - 属于宿主层配置，由 `vulcan-mcp` 自己读取
- `skill_config.json`
  - 由宿主随 `runtime_root` 统一推导并传给 `luaskills`
  - 当前产品不再提供单独文件路径覆盖，避免与运行根参数产生冲突
  - 当前能力会通过宿主 `luaskill-config` MCP 工具对外提供 `list/get/set/delete` 入口
  - 工具返回纯文本结果，不暴露底层配置文件物理地址
  - Lua skill 内部的 `vulcan.config.*` 与宿主 `luaskill-config` 共用这一份统一运行期配置文件
  - 仓库内提供默认空模板，初始内容为 `{}`，便于运行目录直接复制使用

### Lua VM 池配置

当前 `config.yaml` 中有两套互不干扰的 VM 池参数：

- `lua_vm_pool_*`
  - 普通 skill 调用与普通 `run_lua` 主池
- `runlua_pool_config`
  - 隔离 `vulcan.runtime.lua.exec` 专用池

默认模板如下：

```yaml
lua_vm_pool_min_size: 2
lua_vm_pool_max_size: 8
lua_vm_pool_idle_ttl_secs: 600

runlua_pool_config:
  min_size: 1
  max_size: 4
  idle_ttl_secs: 60
```

其中 `runlua_pool_config` 会映射到 `LuaRuntimeHostOptions.runlua_pool_config`。  
如果旧配置文件里暂时没有该配置段，宿主会保留 `luaskills` 上游默认值。

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

当直接从仓库源码目录启动时，需要显式指定运行根，例如：

```bash
cargo run -- --stdio --runtime-root output
```

如果直接执行构建产物，则继续使用运行目录自动发现：

```bash
./output/bin/vulcan-mcp --stdio
```

### ROOT Skill 管理命令

以下命令只执行本地 ROOT 层 skill 管理动作，执行完成后直接退出，不会启动 HTTP、gRPC 或 stdio 服务。

安装指定 skill 到 ROOT 层：

```bash
cargo run -- --install-root-skill LuaSkills/vulcan-codekit --runtime-root output
```

如果直接执行构建产物：

```bash
./output/bin/vulcan-mcp --install-root-skill LuaSkills/vulcan-codekit --runtime-root output
```

安装时也可以显式指定来源类型：

```bash
./output/bin/vulcan-mcp --install-root-skill LuaSkills/vulcan-codekit --source-type github --runtime-root output
```

更新 ROOT 层全部受管 skill：

```bash
cargo run -- --update-root-skills --runtime-root output
```

如果直接执行构建产物：

```bash
./output/bin/vulcan-mcp --update-root-skills --runtime-root output
```

`--update-root-skills` 只会更新带受管安装记录的 ROOT skill；手工放入 ROOT 但没有安装记录的目录会被跳过。

## 与 `luaskills` 的关系

当前仓库通过 Cargo 原生版本依赖引用：

```toml
luaskills = "0.2.0"
```

相关地址：

- 仓库：<https://github.com/LuaSkills/luaskills>
- Cargo：<https://crates.io/crates/luaskills>

但职责边界不变：

- `luaskills`：运行时库
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
- 推进 `luaskills` 的 FFI 导出形态
- 逐步把官方 skill 拆分为独立仓库

## License

MIT
