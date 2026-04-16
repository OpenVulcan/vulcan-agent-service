# __demo 模板说明

这份目录是当前 `lua_skills` 的复制模板，用来给开发者快速起一个新的 skill。

因为 `skill.json` 必须保持标准 JSON，文件本身不能写注释，所以这份 `README.md` 专门承担“贴着模板解释字段”的职责。复制 `__demo` 时，建议把这份文件一起看完，再开始改 `skill.json`。

## 当前模板已同步的最新能力

这份模板现在已经包含下面这些最新约定：

- `groups` 分组结构
- 多 `tool` 入口
- `resources`
- `resource_templates`
- `prompts`
- `prompt.arguments[].completions`
- 顶层 `lancedb` 配置对象

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
- `return_type`
- `prompt`

### `lua_entry`

Lua 文件路径，相对于当前 skill 目录。

例如：

- `main.lua`
- `main_summary.lua`

### `lua_module`

同一个 skill 中每个 tool 的模块名都应该唯一。

即使多个 tool 共用一个 Lua 文件，也建议使用不同的 `lua_module`，避免宿主侧注册混淆。

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

## 推荐复制流程

建议按下面的顺序复制：

1. 复制 `__demo` 目录并重命名
2. 修改 `skill.json` 中的：
   - `name`
   - group 名称
   - tool 名称
   - prompt/resource/template 名称
3. 再替换 `main.lua` / `main_summary.lua`
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
