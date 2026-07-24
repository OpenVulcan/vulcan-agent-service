# Skill 调试文档

## 1. 功能目的

`--call-tools` 是一个本地调试入口，用于在**不启动 HTTP / gRPC 服务**的情况下，直接初始化配置、共享缓存、LuaEngine 与 Lua skill，然后调用指定 tool。

这个模式适合以下场景：

- 调试新写的 Lua skill 是否能被正确加载
- 调试 skill 的依赖初始化是否正常
- 验证 tool 的参数解析、返回结构与错误处理
- 调试 `vmcp-ast`、`vmcp-rg` 这类只依赖本地代码扫描能力的工具

它**不适合**替代完整 MCP 联调。若要验证 HTTP / Streamable / gRPC 传输行为，仍应使用正常服务模式。

## 2. 命令格式

通用格式：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools <tool_name> '<json_arguments>'
```

说明：

- `--call-tools`：进入本地 tool 调试模式
- `<tool_name>`：要调用的 tool 名称，例如 `vmcp-ast`、`vmcp-rg`
- `<json_arguments>`：传给 tool 的 JSON 参数对象，必须是一个合法 JSON 字符串

如果 tool 无需参数，也可以省略第三段参数：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools current_time
```

### 2.1 调试隐藏 `luaexec` 入口

除了 `--call-tools` 之外，当前仓库还保留了一个隐藏的内部调试入口：

- `--internal-luaexec-request`

它的用途不是调试普通 MCP tool，而是直接执行一次隔离 `vulcan.runtime.lua.exec` 请求，并把结果直接打印到标准输出。

推荐命令：

```powershell
.\target\debug\vulcan-agent-service.exe --internal-luaexec-request .\temp\internal_luaexec_request.json --runtime-root output
```

其中请求文件内容是一个 JSON 对象，常用字段如下：

- `task`
  - 可选的人类可读任务说明，会展示在结果头部
- `code`
  - 可选的内联 Lua 代码
- `file`
  - 可选的 Lua 文件路径
- `args`
  - 可选的 JSON 对象，会以 `args` 变量注入到 Lua 里
- `timeout_ms`
  - 可选超时时间，单位毫秒

约束规则：

- `code` 与 `file` 必须且只能提供一个
- 如果直接使用 `target\debug\vulcan-agent-service.exe`，建议显式加上 `--runtime-root output`
- 该入口是内部调试能力，不属于面向最终用户的公开 MCP 参数

最小示例：

请求文件：

```json
{"code":"return 1"}
```

执行命令：

```powershell
.\target\debug\vulcan-agent-service.exe --internal-luaexec-request .\temp\internal_luaexec_request.json --runtime-root output
```

带打印输出的示例：

请求文件：

```json
{"code":"print(\"hello from internal luaexec\")\nreturn { ok = true, value = 42 }"}
```

这个模式下：

- `print(...)` 会被收集到结果里的 `Printed Output`
- Lua 返回值会出现在 `Returned Values`
- 最终 stdout 输出的是一段 Markdown 结果文本，而不是 MCP 协议响应包

## 3. 推荐调试流程

### 3.1 先构建 debug 产物

```powershell
.\make.ps1 build
```

构建后会同步：

- `output/debug/vulcan-agent-service.exe`
- `output/lua_runtime/skills/`
- `output/lua_runtime/lua_packages/`
- `output/configs/`

`--call-tools` 推荐直接使用 `output/debug/vulcan-agent-service.exe`。

### 3.2 执行 tool 调试

以下示例假定当前工作目录就是仓库根目录。

例如调试 `vmcp-rg`：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\runtime\\lua_runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

例如调试 `vmcp-ast`：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-ast '{"path":".\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua","comment":false}'
```

### 3.3 调试技能包配置

`runtime-config` 是唯一的配置管理工具。它会读取完整 LuaEngine 已发现的技能包，因此配置调试必须使用有效的 runtime root；引擎或技能根加载失败时不会退回独立配置存储。

先查看包的声明和完整性，默认不披露保存值或有效值：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools runtime-config '{"action":"describe","skill_id":"example-skill"}'
```

记下 `describe` 返回的当前 `revision`。在用户确认或等价授权之后，通过一次批量事务设置类型化值；以下示例假设刚读取到的 revision 为 `12`：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools runtime-config '{"action":"set","skill_id":"example-skill","values":{"api_token":"secret","retry_count":3},"expected_revision":"12"}'
```

revision 属于整个 `skills` 或 `system-skills` 存储，不能因为目标技能尚未配置就假设它是 `0`。发生 `CONFIG_REVISION_CONFLICT` 时重新执行 `describe`，核对最新状态后再提交。

写入前可只做校验：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools runtime-config '{"action":"validate","skill_id":"example-skill","values":{"api_token":"secret","retry_count":3}}'
```

响应始终是 `{ok, action, result, error}`。`ok=false` 时，`error.code` 与 `error.message` 给出稳定失败信息；不要把敏感输入复制到日志或问题报告。

默认配置根为：

- Windows：`%USERPROFILE%\.vulcan\agent-service\config`
- Linux/macOS：`$HOME/.vulcan/agent-service/config`

普通技能文件是 `skills/config.json`，`ROOT` 系统技能文件是 `system-skills/config.json`。两份文档都必须使用 `format_version: 1` 的当前严格契约。

## 4. 参数传递说明

### 4.1 JSON 参数必须是合法对象

正确示例：

```powershell
'{"path":".\\src","recursive":true,"ext":"rs"}'
```

错误示例：

```powershell
'{path:"src"}'
```

错误原因：

- key 没有双引号
- 路径反斜杠没有正确转义

### 4.2 Windows 下推荐单引号包裹整个 JSON

PowerShell 下推荐：

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

这样可以减少双引号转义混乱。

## 5. 常见示例

### 5.1 调试 `vmcp-rg` 的声明命中

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

预期：

- 先通过 `rg` 找到 `struct ExecRequest`
- 再只输出命中的结构节点

### 5.2 调试 `vmcp-rg` 的函数体内容命中

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

预期：

- `rg` 命中函数体内部字符串
- 返回最近的函数结构
- 在结构下附带命中的具体行

### 5.3 调试 `vmcp-ast` 的结构输出

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-ast '{"path":".\\src\\main.rs","comment":false}'
```

预期：

- 返回结构视图结果
- 不启动 HTTP 或 gRPC 服务

## 6. 输出说明

`--call-tools` 会直接把 tool 的返回值打印到标准输出，通常为 JSON。

例如 `vmcp-rg` 可能返回：

```json
{
  "files_scanned": 1,
  "files_with_matches": 1,
  "items_found": 1,
  "rg_matches": 3,
  "files": [
    {
      "file": "<repo_root>\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua",
      "lines": 2087,
      "content": "local function validate_extension_argument(value) ... L744-803"
    }
  ]
}
```

重点字段：

- `files_scanned`：参与 AST 分析的文件数
- `files_with_matches`：最终有结构命中的文件数
- `rg_matches`：`rg` 的原始命中数
- `files[].content`：最终展示给调试者的结构内容

## 7. 常见问题

### 7.1 提示 `Unknown Lua skill tool`

原因：

- tool 名称写错
- `skill.json` 中没有注册该 tool
- 构建后 `output/lua_runtime/skills` 尚未同步到最新版本

处理：

```powershell
.\make.ps1 build
```

然后重新执行。

### 7.2 提示依赖缺失

如果 skill 声明了 `dependencies.yaml`，加载时会自动检查其版本化工具依赖目录：

- `output/lua_runtime/dependencies/tools/`

若缺失对应依赖，需先确保构建和依赖同步过程已完成。

需要额外区分版本化依赖工具与宿主控制器：

- `output/lua_runtime/dependencies/tools/`
  - 版本化的技能命令行工具目录，例如 `rg`、`ast-grep`
- `output/lua_runtime/bin/vldb-controller.exe`
  - 数据库控制器主程序
  - 不属于版本化依赖工具树

如果当前调试的是会访问 SQLite / LanceDB 的 skill，还需要额外确认：

- 已执行过 `make deps` 与 `make build`
- `output/lua_runtime/bin/vldb-controller.exe` 已存在
- 如果 `space_controller.auto_spawn=true`，则 `space_controller.endpoint` 必须是本地可拉起地址
- 如果连接远端 controller，则应设置 `auto_spawn=false`，并提前保证远端 controller 已启动
- 若手工替换了 controller 二进制，需保证其 release tag 与当前仓库锁定的 `vldb-controller-client` 一致

### 7.3 调试结果与运行中服务不一致

`--call-tools` 使用的是**本地当前构建产物**，不一定等同于你当前正在运行的 `output/bin` 进程。

建议区分：

- `output/debug/vulcan-agent-service.exe --call-tools ...`：本地功能调试
- `output/bin/vulcan-agent-service.exe`：正在运行的正式服务

### 7.4 为什么这个模式不启动 HTTP / gRPC？

因为它的设计目标就是缩短调试路径，只验证：

- 配置加载
- LuaEngine 初始化
- skill 加载
- tool 实际返回值

这样可以避免被传输层、端口占用或外部服务依赖干扰。

## 8. 调试建议

建议优先顺序：

1. 先用 `--call-tools` 验证 tool 逻辑本身是否正确
2. 再进入正式 MCP 服务模式验证协议和客户端联动

对于 AST / rg 类工具，优先使用：

- 小目录
- 明确扩展名
- 明确正则

这样更容易定位是：

- `rg` 没命中
- AST 没识别
- 还是映射规则不符合预期
