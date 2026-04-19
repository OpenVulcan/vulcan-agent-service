# __demo 模板说明

这份目录是当前 `lua_skills` 的复制模板，用来给开发者快速起一个新的 skill。

因为 `skill.json` 必须保持标准 JSON，文件本身不能写注释，所以这份 `README.md` 专门承担“贴着模板解释字段”的职责。复制 `__demo` 时，建议把这份文件一起看完，再开始改 `skill.json` 与 `tools/` 目录里的 Lua 文件。

## 当前模板已同步的最新能力

这份模板现在已经对齐了下面这些新规则：

- `groups` 分组结构
- 多 `tool` 入口
- `resources`
- `resource_templates`
- `prompts`
- `prompt.arguments[].completions`
- 顶层 `lancedb` 配置对象
- tool 入口统一放在 `tools/` 目录
- 工具返回值统一改为字符串，不再返回 Lua table
- 工具可选使用多返回值超限协议
- `skill.json` 不再使用 `return_type`

## 当前目录结构建议

复制模板后，建议继续沿用下面这类结构：

- `tools/`
  - 存放工具入口 Lua 文件
- `resources/`
  - 存放静态资源或资源生成器
- `templates/`
  - 存放资源模板或模板生成器
- `prompts/`
  - 存放静态或动态提示词文件

说明：

- tool 的 `lua_entry` 必须位于 `tools/` 下
- prompt 的 `file` 建议位于 `prompts/` 下
- resource 的 `file` 建议位于 `resources/` 下
- resource template 的 `file` 建议位于 `templates/` 下

## `skill.json` 顶层字段说明

### `name`

skill 名称。

注意：

- 真正复制出去后，这里应该改成你的 skill 名称
- `__demo` 本身因为目录名前缀是 `__`，宿主会跳过自动加载

### `debug`

是否开启调试热加载。

- `true`：每次调用都重新读取 Lua 文件
- `false`：按正常加载流程使用

开发期建议保留 `true`，稳定后再改成 `false`。

### `lancedb`

当前推荐的新写法：

```json
"lancedb": {
  "enable": false,
  "log_level": "info",
  "slow_log_enabled": false,
  "slow_log_threshold_ms": 800
}
```

字段说明：

- `enable`
  - 是否为当前 skill 开启宿主管理的 LanceDB
- `log_level`
  - 支持：`off` / `info` / `warning`
- `slow_log_enabled`
  - 是否开启慢操作日志
- `slow_log_threshold_ms`
  - 慢操作阈值（毫秒）

规则说明：

- 一个 skill 只能绑定一个 LanceDB 库
- 数据库目录固定为 `__lancedb/<skill_dir_name>`
- skill 目录名就是数据库名
- Lua 不负责创建/删除数据库，只负责表和数据操作

如果当前 skill 不需要 LanceDB，就保持：

```json
"lancedb": {
  "enable": false
}
```

## `groups` 结构说明

一个 skill 可以有多个分组。

当前模板分成两组：

- `tooling`
  - 放工具入口
- `references`
  - 放资源、模板、提示词示例

如果你的 skill 很简单，也可以只保留一个 group。

## `tools` 结构说明

每个 tool 主要包含：

- `name`
- `description`
- `lua_entry`
- `lua_module`
- `parameters`
- `prompt`

### `lua_entry`

Lua 文件路径，相对于当前 skill 目录，且工具入口必须位于 `tools/` 目录。

例如：

- `tools/demo_template_tool.lua`
- `tools/demo_template_summary.lua`

### `lua_module`

同一个 skill 中每个 tool 的模块名都应该唯一。

即使多个 tool 共用一个 Lua 文件，也建议使用不同的 `lua_module`，避免宿主侧注册混淆。

### 当前工具返回规则

普通工具现在必须直接返回字符串。

当前推荐：

- 返回 Markdown 字符串
- 不再返回 Lua table 给宿主
- 如需让宿主接管超限处理，可使用多返回值协议

可选写法示例：

```lua
return content
```

```lua
return content, vulcan.overflow_type.truncate
```

```lua
return content, vulcan.overflow_type.page
```

```lua
return content, vulcan.overflow_type.page, "overflow_page.md"
```

含义说明：

- 只返回 `content`
  - 由宿主按默认截断策略处理
- `truncate`
  - 超长时按截断模式处理
- `page`
  - 超长时按分页目录模式处理
- 第三个返回值
  - 指定模板名，宿主会优先查 skill 本地模板，再查公共模板

### 已取消的旧规则

这些旧规则不应该再在新 skill 中继续使用：

- `return_type`
- tool 直接返回 Lua table 给宿主
- tool 自己拼宿主级分页/截断最终文案

如果你看到历史 skill 里还留着这些写法，复制新模板时不要继续沿用。

## `parameters` 结构说明

每个参数支持：

- `name`
- `type`
- `description`
- `required`

`type` 目前仍然使用 JSON Schema 风格字符串，例如：

- `string`
- `number`
- `boolean`
- `object`

## `prompts.arguments[].completions`

当前模板已经补上了 `completions` 示例。

例如：

```json
{
  "name": "goal",
  "description": "Goal that the copied skill should solve.",
  "required": false,
  "completions": [
    "Summarize the current structure",
    "Locate the relevant implementation",
    "Prepare the next editing step"
  ]
}
```

这类候选项会被宿主读取，用于 prompt 参数补全。

## `vulcan-runtime` 相关新能力

当前仓库已经内置 `vulcan-runtime` skill，主要提供：

- `vulcan-lua-help`
- `vulcan-lua-exec`
- `vulcan-lua-file`

如果你复制模板后需要：

- 临时执行一段 Lua 代码
- 执行一个现成 Lua 文件
- 先查看当前支持的扩展库与宿主 API

可以直接调用这些工具，而不需要在自己的 skill 中重复造一套执行器。

建议理解：

- `vulcan-lua-help`
  - 查看帮助与支持库清单
- `vulcan-lua-exec`
  - 执行临时 Lua 代码
- `vulcan-lua-file`
  - 执行一个 Lua 文件，并自动切换工作目录

## `vulcan-curl` 相关新能力

当前仓库还内置了 `vulcan-curl` skill，主要提供：

- `vulcan-curl-get`
- `vulcan-curl-post`
- `vulcan-curl`

适用场景：

- 简单 GET 请求
- 简单 POST 请求
- 简单 PUT 更新请求
- 简单 DELETE 删除请求
- 直接发 HTTP / HTTPS 请求
- 调 OpenAI、GitHub 或通用 REST API
- 上传简单表单
- 保存响应到文件

设计目标：

- 保持 AI 对 Linux `curl` 参数风格的熟悉心智
- 但底层不依赖系统 `curl` 可执行文件
- 统一走 `lua-curl`
- 避免 PowerShell / pwsh / sh 的转义差异

当前推荐理解：

- `vulcan-curl-get` / `vulcan-curl-post` 是面向 AI 的快捷工具
- 复杂需求统一回退到基础 `vulcan-curl`
- `vulcan-curl-post` 现在支持简单 `form + files` 组合上传
- 两个快捷工具都支持 `bearer` / `basic` 认证快捷输入
- 两个快捷工具都支持 `follow_location`、`download_to`、`save_headers_to`
- 传入的是 curl 风格参数数组
- 不需要手写 shell 命令
- 如果环境存在 TLS 中间代理，仍可能需要显式传：
  - `-k`
  - 或 `--cacert`

## 推荐复制流程

建议按下面的顺序复制：

1. 复制 `__demo` 目录并重命名
2. 修改 `skill.json` 中的：
   - `name`
   - group 名称
   - tool 名称
   - prompt/resource/template 名称
3. 再替换 `tools/demo_template_tool.lua` / `tools/demo_template_summary.lua`
4. 最后删掉不需要的演示文件

## 什么时候再考虑 YAML

当前宿主仍以 `skill.json` 为标准入口。

之所以暂时不直接改成 YAML，是因为：

- JSON 解析链更稳定
- 现有 Rust 加载逻辑已经全面围绕 JSON 跑通
- 迁移 YAML 会影响兼容、校验与加载链

所以当前更推荐的方式是：

- `skill.json` 保持机器可读
- `README.md` 承担模板说明与“备注”职责

如果后续真的要支持 YAML，更稳的方向也应该是“兼容增加”，而不是直接替换掉 `skill.json`。
