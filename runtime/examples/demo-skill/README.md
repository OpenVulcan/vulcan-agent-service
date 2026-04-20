# __demo 模板说明

这份目录是新的 LuaSkills Core 模板示例，用来演示迁移后的最小结构，而不是旧的 `skill.json + groups/prompts/templates/resources` 模型。

`__demo` 目录不会被自动加载，不是因为宿主单独判断 `__` 前缀，而是因为 LuaSkills 现在要求目录名必须匹配 `^[a-z]([a-z0-9-]*[a-z0-9])?$`。复制它时，建议把目录改成你自己的合法 skill 名称，再同步修改 `skill.yaml` 中的 `name`。

## 当前模板对应的新结构

- `skill.yaml`
  - skill 主清单
- `dependencies.yaml`
  - 依赖声明
- `runtime/`
  - 运行时 Lua 入口
- `help/`
  - 主 help 与子 help 示例
- `overflow_templates/`
  - 分页 / 截断提示模板示例
- `resources/`
  - skill 私有资源资产示例

## 命名规则

新的 canonical 命名方式为：

- `skill_id-entry_name`

当前模板中的两个入口若被复制到合法 skill 目录下，最终会被宿主识别为：

- `my-skill-template-tool`
- `my-skill-template-summary`

其中 `entry_name` 只保留局部名，不再在 manifest 里重复拼 skill 前缀；如果不同 skill 组合后出现同名，runtime 会自动追加 `-2`、`-3`。

## `skill.yaml` 重点字段

### `name`

- `name`
  - 当前 skill 的内部名称
- `skill_id`
  - 不再由 `skill.yaml` 声明
  - 统一直接取 skill 目录名

### `entries`

每个 entry 只声明局部名与运行时入口，例如：

- `name`
- `description`
- `lua_entry`
- `lua_module`
- `help`
- `parameters`

其中：

- `lua_entry` 现在应位于 `runtime/`
- `help` 指向一个 help topic 名称，而不是 prompt 名称

### `help`

新的 help 模型分成：

- `main`
  - skill 主帮助节点
- `topics`
  - 子帮助 / 工作流节点

它们不要求与具体 entry 一一对应。  
help 更像“能力导航”和“工作流说明”，而不是 MCP prompt。

### `overflow_templates`

这里放的是：

- 分页提示模板
- 截断提示模板

它们不是普通回复正文模板。  
运行时只返回完整内容、分割建议与模板建议；真正如何分页、截断、是否落盘，由上层宿主决定。

## 当前模板里的示例文件

### `runtime/demo_template_tool.lua`

- 演示一个普通 entry 如何直接返回字符串结果

### `runtime/demo_template_summary.lua`

- 演示字符串结果加多返回值超限协议

### `help/static_prompt.md`

- 作为主 help 节点的最简单静态帮助示例

### `help/dynamic_prompt.lua`

- 演示如何用 Lua 动态生成子 help / workflow 节点内容

### `overflow_templates/example.md`

- 静态超限模板示例

### `overflow_templates/example_generator.lua`

- 动态超限模板示例

### `resources/guide.md`

- skill 私有资源示例，供运行时读取或演示复制使用

## 返回值规则

当前模板对应的新约定是：

- 普通正文直接由 Lua 返回
- Lua 不负责宿主级最终分页文案拼装
- Lua 可以返回超限建议
- 实际分页、截断、落盘、模板套用由宿主决定

推荐写法：

```lua
return content
```

```lua
return content, vulcan.runtime.overflow_type.truncate
```

```lua
return content, vulcan.runtime.overflow_type.page, "overflow_page.md"
```

## 推荐复制流程

1. 复制 `__demo` 目录并改名
2. 修改 `skill.yaml` 中的 `name`、`entries`
3. 替换 `runtime/` 目录中的 Lua 实现
4. 调整 `help/` 与 `overflow_templates/`
5. 删除不需要的示例资源文件

## 当前模板的定位

这份模板主要用于演示：

- 新的 `skill.yaml` 结构
- `skill_id-entry_name` 命名规则
- help 取代 prompt 的方向
- `overflow_templates` 取代旧模板目录的方向
- 运行时与宿主的分页 / 截断边界

如果你要写新的官方 skill，建议优先从这份结构开始，而不是继续复制旧的 `skill.json` 模型。
