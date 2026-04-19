# vulcan-mcp

`vulcan-mcp` 是 Vulcan 生态中的 **MCP 宿主与协议适配层**。  
它不再承担 LuaSkills 核运行时真相，而是基于 [`vulcan-luaskills`](https://github.com/OpenVulcan/vulcan-luaskills) 提供：

- MCP 协议接入
- HTTP / gRPC 服务与本地调试模式
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
- 支持 HTTP 服务模式、gRPC 服务模式与本地调试模式
- 通过本地依赖接入 `vulcan-luaskills`
- 自动加载 `runtime/lua_skills/` 下符合规则的 LuaSkills
- 把 skill entry 映射成 MCP tools
- 提供宿主封装的 strict help 工具
- 在宿主层处理工具结果的分页、截断与 spill 文件输出
- 支持宿主级 `client_budgets.yaml` 与 `tool_configs.yaml`

## 当前公开方式

### 1. LuaSkills

LuaSkills 是当前 MCP 对外暴露的主能力面。  
官方 skill 目前包括：

- `vulcan-runtime`
- `vulcan-codekit`
- `vulcan-curl`
- `vulcan-ai-memory`
- `vulcan-work-memory`

这些 skill 已迁移到新的目录结构：

```text
runtime/lua_skills/<skill>/
├─ skill.yaml
├─ help/
├─ runtime/
├─ overflow_templates/
├─ resources/
└─ licenses/
```

### 2. Help 工具

help 不再通过 skill tool 直接暴露，而是由宿主包装为：

- `vulcan-help-list`
- `vulcan-help-detail`

其中：

- `vulcan-help-list`
  - 列出所有已注册 help 节点与简要说明
- `vulcan-help-detail`
  - 按 `skill + flow` 读取具体帮助节点

### 3. RunLua 暴露策略

`runlua` 的 system 能力仍然保留在 `vulcan-luaskills` 内部与 `vulcan.runtime.lua.exec` 链路中，  
但 `vulcan-mcp` **不再直接公开 `runlua` MCP tool**。

MCP 侧推荐通过 `vulcan-runtime` skill 使用：

- `vulcan-runtime-lua-exec`
- `vulcan-runtime-lua-file`

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
├─ configs/               # 宿主配置
└─ lua_skills/            # 官方内建 LuaSkills
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

## 与 `vulcan-luaskills` 的关系

当前仓库通过本地 path dependency 引用：

```toml
vulcan-luaskills = { path = "../vulcan-luaskills" }
```

后续独立发布后，可以切换为远程仓库依赖或版本依赖。  
但职责边界不变：

- `vulcan-luaskills`：运行时库
- `vulcan-mcp`：MCP 宿主

## 后续方向

- 继续完善 system tools 的宿主枚举与包装模型
- 进一步收紧 `system` 与 `skill` 的公开边界
- 推进 `vulcan-luaskills` 的 FFI 导出形态
- 逐步把官方 skill 拆分为独立仓库

## License

MIT
