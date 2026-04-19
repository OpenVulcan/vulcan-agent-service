# __demo Resource Guide

这是 `__demo` skill 的资源示例文件。

新的模板结构里，`resources/` 表示 skill 私有资产目录，不再等同于 MCP `resources/list` 的协议对象。

当前目录示例用途：

- `runtime/`
  - 存放真实 Lua 入口
- `help/`
  - 存放主 help 与子 help / workflow help
- `overflow_templates/`
  - 存放分页与截断提示模板
- `resources/`
  - 存放 skill 自己需要读取的静态或动态资源

复制模板时，建议把这里的内容替换成真正会被你的 skill 读取或引用的资产，而不是继续保留旧的 prompt/resource-template 示例。
