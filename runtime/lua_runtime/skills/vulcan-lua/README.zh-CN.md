# vulcan-lua

面向 Vulcan Agent 的 AI 原生 Lua 执行桥。

`vulcan-lua` 提供一个受控 Lua 运行时桥接能力，用于执行短小、明确、有边界的 Lua 任务。Agent 可以运行内联 Lua 代码，也可以执行一个已有 `.lua` 脚本，并通过结构化 `args` 传参，获得稳定的输入校验和 Markdown 输出。

## 什么时候使用

当 Agent 需要借助宿主运行时上下文执行一个有边界的 Lua 任务时使用 `vulcan-lua`：

- 执行短小的一次性 Lua 数据处理逻辑。
- 运行已有 `.lua` 脚本作为可复用任务。
- 通过结构化 `args` 传参，避免拼接平台相关 shell 引号。
- 查看 Vulcan 暴露的当前客户端上下文与结果限制。
- 快速原型或调试小型 LuaSkill 辅助逻辑。

如果任务主要是文件编辑、代码结构导航、验证、HTTP 请求或通用命令执行，应优先使用对应的 file、CodeKit、TestKit、curl 或 shell 工具。

## 工具

### `vulcan-lua-run`

用于运行一个 Lua 任务。`code` 与 `file` 必须且只能提供一个。

内联模式：

```yaml
task: summarize numbers
code: |
  local total = 0
  for _, value in ipairs(args.values) do
    total = total + value
  end
  return { total = total, count = #args.values }
args:
  values: [1, 2, 3]
```

文件模式：

```yaml
task: run local helper
file: scripts/helper.lua
args:
  input: example
timeout_ms: 60000
```

`file` 模式会把执行工作目录切换到脚本所在目录，并注入：

- `vulcan.context.entry_file`
- `vulcan.context.entry_dir`

## 运行时行为

工具会先校验输入。输入不合法时返回 `Runtime Input Error` 报告，不会进入 Lua 执行阶段。

关键行为：

- `code` 与 `file` 互斥，且必须提供一个。
- `args` 会在脚本中作为局部变量 `args` 暴露。
- `print(...)` 输出会被捕获到 Markdown 结果中。
- 返回 table 会被渲染为格式化 JSON 文本。
- 多返回值会按顺序展示。
- 结果包含 `Current Client Context` 区块，用于展示调用方元数据和当前结果/读取限制。

运行时边界：

- 执行环境内会禁用 `vulcan.runtime.lua.exec`、`vulcan.runtime.log` 和 `vulcan.cache.*`。
- 工具不能通过当前执行桥递归调用自身。
- `vulcan.call(name, args)` 仍可用于组合其他工具，但不建议作为通用自动化主路径。

## 验证

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

发布包会生成在 `dist/` 下：

- `vulcan-lua-v<version>-skill.zip`
- `vulcan-lua-v<version>-checksums.txt`

## 说明

- 仓库根目录就是 skill 根目录。
- 安装后的 skill id 来自包根目录名：`vulcan-lua`。
- 运行时代码不捆绑外部命令行工具。
- 输出面向 AI Agent 设计：有边界、语义明确，并足够稳定，适合作为小型 Lua 执行原语。
