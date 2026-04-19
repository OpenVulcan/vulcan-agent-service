# Vulcan Runtime Help

这是 `vulcan-runtime` 的主帮助节点。

当前推荐工作流：

- `vulcan-help-list`
  - 先列出当前宿主已注册的 help 树与可用 flow
- `vulcan-help-detail`
  - 在 MCP 宿主里先读取 `skill=vulcan-runtime` 且 `flow=main`、`lua-exec` 或 `lua-file` 的帮助节点
- `vulcan-runtime-lua-exec`
  - 执行短小的内联 Lua 代码
- `vulcan-runtime-lua-file`
  - 执行现有 Lua 文件，并自动切换工作目录

当前正式可用的 `vulcan.runtime.*` system tools / 字段：

- `vulcan.runtime.cwd()`
  - 返回当前运行时工作目录
- `vulcan.runtime.log(level, message)`
  - 向宿主日志层发送运行时日志事件
- `vulcan.runtime.temp_dir`
  - 宿主管理的临时目录
- `vulcan.runtime.resources_dir`
  - 宿主管理的运行时资源目录
- `vulcan.runtime.overflow_type.truncate`
- `vulcan.runtime.overflow_type.page`
- `vulcan.runtime.lua.exec(input)`
  - 受控的运行时 Lua 执行桥，仅在普通 skill VM 内可用

另外这些根命名空间也属于当前正式能力面：

- `vulcan.fs.*`
- `vulcan.path.join(...)`
- `vulcan.process.exec(...)`
- `vulcan.os.info()`
- `vulcan.json.encode(...) / decode(...)`
- `vulcan.cache.put/get/delete`
- `vulcan.call(...)`

注意：

- `vulcan.runtime.internal` 仅属于运行时内部状态表，不应作为公开工作流能力依赖。
- 在 `vulcan-runtime-lua-exec` / `vulcan-runtime-lua-file` 的隔离执行环境里，`vulcan.runtime.lua.exec`、`vulcan.runtime.log` 和 `vulcan.cache.*` 会被主动禁用。
- 当前 help 本身属于 system 帮助树，不再作为公开 skill tool 暴露。
