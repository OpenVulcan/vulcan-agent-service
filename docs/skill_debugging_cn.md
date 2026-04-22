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
.\output\debug\vulcan-mcp.exe --call-tools <tool_name> '<json_arguments>'
```

说明：

- `--call-tools`：进入本地 tool 调试模式
- `<tool_name>`：要调用的 tool 名称，例如 `vmcp-ast`、`vmcp-rg`
- `<json_arguments>`：传给 tool 的 JSON 参数对象，必须是一个合法 JSON 字符串

如果 tool 无需参数，也可以省略第三段参数：

```powershell
.\output\debug\vulcan-mcp.exe --call-tools current_time
```

## 3. 推荐调试流程

### 3.1 先构建 debug 产物

```powershell
.\make.ps1 build
```

构建后会同步：

- `output/debug/vulcan-mcp.exe`
- `output/skills/`
- `output/lua_packages/`
- `output/configs/`

`--call-tools` 推荐直接使用 `output/debug/vulcan-mcp.exe`。

### 3.2 执行 tool 调试

例如调试 `vmcp-rg`：

```powershell
.\output\debug\vulcan-mcp.exe --call-tools vmcp-rg '{"dir":"D:\\projects\\vulcan-mcp-client\\runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

例如调试 `vmcp-ast`：

```powershell
.\output\debug\vulcan-mcp.exe --call-tools vmcp-ast '{"path":"D:\\projects\\vulcan-mcp-client\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua","comment":false}'
```

## 4. 参数传递说明

### 4.1 JSON 参数必须是合法对象

正确示例：

```powershell
'{"path":"D:\\projects\\vulcan-mcp-client\\src","recursive":true,"ext":"rs"}'
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
.\output\debug\vulcan-mcp.exe --call-tools vmcp-rg '{"dir":"D:\\projects\\vulcan-mcp-client\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

这样可以减少双引号转义混乱。

## 5. 常见示例

### 5.1 调试 `vmcp-rg` 的声明命中

```powershell
.\output\debug\vulcan-mcp.exe --call-tools vmcp-rg '{"dir":"D:\\projects\\vulcan-mcp-client\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

预期：

- 先通过 `rg` 找到 `struct ExecRequest`
- 再只输出命中的结构节点

### 5.2 调试 `vmcp-rg` 的函数体内容命中

```powershell
.\output\debug\vulcan-mcp.exe --call-tools vmcp-rg '{"dir":"D:\\projects\\vulcan-mcp-client\\runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

预期：

- `rg` 命中函数体内部字符串
- 返回最近的函数结构
- 在结构下附带命中的具体行

### 5.3 调试 `vmcp-ast` 的结构输出

```powershell
.\output\debug\vulcan-mcp.exe --call-tools vmcp-ast '{"path":"D:\\projects\\vulcan-mcp-client\\src\\main.rs","comment":false}'
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
      "file": "D:\\projects\\vulcan-mcp-client\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua",
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
- 构建后 `output/skills` 尚未同步到最新版本

处理：

```powershell
.\make.ps1 build
```

然后重新执行。

### 7.2 提示依赖缺失

如果 skill 声明了 `dependencies.yaml`，加载时会自动检查共享工具目录：

- `output/bin/tools/`

若缺失对应依赖，需先确保构建和依赖同步过程已完成。

需要额外区分两类宿主产物：

- `output/bin/tools/`
  - 共享命令行工具目录，例如 `rg`、`ast-grep`
- `output/bin/vldb-controller.exe`
  - 数据库控制器主程序
  - 不属于 `bin/tools`

如果当前调试的是会访问 SQLite / LanceDB 的 skill，还需要额外确认：

- 已执行过 `make deps host` 与 `make build`
- `output/bin/vldb-controller.exe` 已存在
- 如果 `space_controller.auto_spawn=true`，则 `space_controller.endpoint` 必须是本地可拉起地址
- 如果连接远端 controller，则应设置 `auto_spawn=false`，并提前保证远端 controller 已启动
- 若手工替换了 controller 二进制，需保证其 release tag 与当前仓库锁定的 `vldb-controller-client` 一致

### 7.3 调试结果与运行中服务不一致

`--call-tools` 使用的是**本地当前构建产物**，不一定等同于你当前正在运行的 `output/bin` 进程。

建议区分：

- `output/debug/vulcan-mcp.exe --call-tools ...`：本地功能调试
- `output/bin/vulcan-mcp.exe`：正在运行的正式服务

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
