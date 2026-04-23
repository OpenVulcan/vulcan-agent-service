# Vulcan Lua Help

这是 `vulcan-lua` 的包级帮助节点。

当前公开工具：

- `vulcan-lua-run`

当前推荐工作流：

1. 先读取 `skill=vulcan-lua` 且 `flow=main` 的帮助节点。
2. 准备 `{ task?, code?, file?, args?, timeout_ms? }`。
3. 在 `code` 与 `file` 中二选一。
4. 保持脚本只完成一件事情，并返回一个最终结果。

`vulcan-lua-run` 参数默认值：

- `task`
  - 可选。
  - 默认：不传。
  - 只用于结果头部展示。
- `code`
  - 可选。
  - 默认：不传。
  - 用于短小、一次性的内联 Lua 执行。
- `file`
  - 可选。
  - 默认：不传。
  - 用于执行一个已有 `.lua` 文件。
  - 执行期间会自动把 `cwd` 切换到目标文件目录。
- `args`
  - 可选。
  - 默认：`{}`。
  - 会在执行环境中以局部变量 `args` 暴露。
- `timeout_ms`
  - 可选。
  - 默认：`60000`。
  - 表示执行超时，单位毫秒。

输入校验规则：

- 输入必须是一个 table 对象。
- `code` 与 `file` 必须且只能提供一个。
- `task` 如果提供，必须是字符串。
- `code` 如果提供，必须是字符串。
- `file` 如果提供，必须是字符串。
- `args` 如果提供，必须是对象 table。
- `timeout_ms` 如果提供，必须是大于 `0` 的数字。
- 纯空白字符串会按“未传入”处理。
- 输入不合法时，不会进入执行阶段，而是直接返回 `Runtime Input Error`。

适用场景建议：

- 传 `code`
  - 短循环
  - 临时文件生成
  - 一次性数据转换
  - 快速网络探测
  - 小型命令编排
- 传 `file`
  - 已沉淀成独立脚本的多步骤逻辑
  - 需要文件相对路径能力的脚本
  - 单独文件更清晰、可复用的运行逻辑

运行时行为：

- `print(...)` 会被捕获到返回结果中。
- 返回 table 会被转换成格式化 JSON 文本。
- 多返回值会按顺序逐项展示。
- 工具最终总是返回一个 Markdown 字符串。
- 返回结果末尾会固定追加一个 `Current Client Context` 区块。
- 这个区块展示的是外层真实调用方上下文，不是 `luaexec_call` 这样的内部模拟请求身份。
- 该区块会展示：
  - `client_kind`
  - `client_name`
  - `tool_result_bytes_limit`
  - `tool_result_line_limit`
  - `file_read_bytes_limit`
  - `file_read_line_limit`
- 使用 `file` 模式时，会同时注入：
  - `vulcan.context.entry_file`
  - `vulcan.context.entry_dir`

当前边界：

- 执行环境内 `vulcan.runtime.lua.exec`、`vulcan.runtime.log` 和 `vulcan.cache.*` 会被主动禁用。
- 不允许在 `vulcan-lua-run` 中再次调用当前执行工具自身。
- `vulcan.call(name, args)` 仍可用于组合其它工具，但不建议作为常规主路径。
