# Lua Skill 开发指南

## 环境

- **引擎**: LuaJIT 2.1 (兼容 Lua 5.2)
- **模式**: `unsafe`（允许 C 模块加载，可使用 FFI）
- **入口**: 每个 tool 入口的 `lua_entry` 文件返回 `function(args)`，附属能力入口通过各自的 `file` 字段绑定静态文件或 `.lua` 生成器

## 调试文档

- 中文调试文档：`docs/skill_debugging_cn.md`
- English debugging guide: `docs/skill_debugging_en.md`
- `codekit-patch` 中文使用说明：`docs/vmcp_patch_usage_cn.md`

当前仓库已支持 `--call-tools <tool_name> [json_arguments]` 本地调试模式，可在不启动 HTTP / gRPC 服务的情况下直接初始化 Lua skill 并执行目标 tool。

## LuaSkills 0.5.7 技能包配置

配置归属于技能包，而不是单个 entry。同一包内的所有 entry 共享顶层声明、持久化命名空间、revision 和可选业务校验器。声明只能放在 `skill.yaml` 顶层；entry 内的 `config` 或 `config_validator` 会被 0.5.7 作为未知字段拒绝。

最小声明示例：

```yaml
config:
  - key: api_token
    type: string
    required: true
    sensitive: true
    description: Service access token
    format: password
    constraints:
      min_length: 1
      max_length: 4096

  - key: retry_count
    type: integer
    default: 3
    description: Maximum request retry count
    constraints:
      minimum: 0
      maximum: 10

config_validator: runtime/config-validator.lua
```

声明规则：

- 支持 `integer`、`string`、`float`、`enum`、`boolean` 五种类型。
- `description` 必填；`required` 与 `sensitive` 默认均为 `false`。
- 整数和浮点约束使用包含边界的 `minimum`、`maximum`；字符串使用 `min_length`、`max_length`。
- `enum` 必须使用 `options`，每项包含 `value`、`label`、`description`。
- `default` 是公开声明元数据，即使 `sensitive=true` 也会被 `describe` 返回，因此严禁把真实秘密写成默认值。
- `config_validator` 是可选的包级跨字段校验器；校验失败时整个批量写入回滚。

技能 Lua 只能访问当前包，不能指定或修改其他包。可用 API 如下：

| API | 作用 |
| --- | --- |
| `vulcan.config.get(key)` | 返回已保存值或声明默认值；未声明键会失败 |
| `vulcan.config.has(key)` | 判断已保存值或声明默认值是否存在 |
| `vulcan.config.set(key, value)` | 通过原子批量事务写入一个标量 |
| `vulcan.config.set(values)` | 原子写入非空键值表，任一项失败则全部不落盘 |
| `vulcan.config.delete(key)` | 删除已保存值，删除后可能重新显露声明默认值 |
| `vulcan.config.list()` | 列出全部已声明键的有效值 |
| `vulcan.config.describe()` | 返回声明、约束与状态，不返回值 |
| `vulcan.config.status()` | 返回 revision、存储范围、完整性、问题与孤儿键 |

推荐在真正需要配置的入口中显式检查完整性：

```lua
local status = vulcan.config.status()
if not status.complete then
    return [[This skill package configuration is incomplete.
Ask the AI to call runtime-config with action=describe and this package id.
Only call action=set after host or user authorization, and never echo secrets.]]
end

local api_token = vulcan.config.get("api_token")
return use_service(api_token)
```

宿主对外只暴露 `runtime-config`，支持 `describe`、`validate`、`list`、`get`、`set`、`delete`、`refresh`。该工具能够跨包读取或修改配置，因此调用方必须完成用户确认或等价授权；请求本身没有可信的“已授权”字段。写入应把当前可用值放在一次类型化 `values` 批次中，并在并发或陈旧界面场景携带 `expected_revision`。敏感值不得写入日志、帮助文本或错误消息。

服务默认使用用户级配置根：

- Windows：`%USERPROFILE%\.vulcan\agent-service\config`
- Linux/macOS：`$HOME/.vulcan/agent-service/config`

普通技能保存到 `<skill_config_root>/skills/config.json`，`ROOT` 系统技能保存到 `<skill_config_root>/system-skills/config.json`。两份文档都使用 `format_version: 1` 的当前严格契约。

## LuaSkills 托管字段契约

部分 LuaSkill 需要稳定的会话、任务或上下文身份，用于把多次工具调用绑定到同一份状态。为了避免每个对接方各自约定参数名和隐藏规则，LuaSkills 生态保留 `LUASKILL_SID` 作为通用托管身份字段。

`LUASKILL_SID` 是文档级契约，不是 LuaSkills 运行时内置特殊字段。LuaSkill 代码可以把它当普通字符串参数读取；是否隐藏、是否自动注入、如何生成稳定值，由暴露工具的一方决定。

通用规则：

- 新 LuaSkill 如果需要稳定会话/任务身份，优先使用普通参数 `LUASKILL_SID`。
- 对接方如果支持托管身份，应在向模型暴露工具时隐藏该字段，并在调用 LuaSkill 前自动注入。
- 对接方如果不支持托管身份，应保留该字段在工具 schema 中，让模型、用户、调用方或 LuaSkill 自身 fallback 逻辑处理。
- LuaSkill 不能假设所有对接方都支持托管身份；需要在 help 中说明托管模式和非托管模式的差异。
- 托管身份字段只用于稳定上下文归属，不应用作鉴权 token、数据库密钥或其它安全凭证。
- 历史业务字段不属于通用标准；需要统一接入托管身份时，应迁移到 `LUASKILL_SID`。

### 对 LuaSkill 开发者的要求

当某个 LuaSkill 依赖 `LUASKILL_SID` 时，建议提供一个 create、start、open 或 bootstrap 类入口，用于建立或恢复状态。

该入口的行为建议：

- 如果调用方显式传入 `LUASKILL_SID`，直接复用该 ID，不再生成新 ID。
- 如果调用方没有传入 `LUASKILL_SID`，且该 LuaSkill 支持非托管 fallback，可以生成新的稳定 ID。
- 生成新 ID 后，工具结果必须显式输出该 ID，并说明后续调用必须继续传入。
- 生成新 ID 后，只能建议用户保存到 `AGENTS.md`、`CLAUDE.md` 或项目规则文件，不能在未获用户明确同意时自动写入。
- LuaSkill 只需要对非托管生成的公开 ID 负责回显；托管模式下原始身份的隐藏、脱敏和结果提示由对接方负责。

非 create 类入口的行为建议：

- 如果缺少必需的 `LUASKILL_SID`，应返回明确错误，提示先调用 create/start/bootstrap 入口，或传入已保存的 ID。
- 非托管模式下，每次成功调用结果中建议携带可恢复公开 ID；托管模式下，对接方可将可见结果改写为 `managed` 状态说明。
- 帮助文档中应说明：非托管模式下模型需要显式传递该字段，托管模式下模型不需要也不应该要求用户提供该字段。

### 对接方处理规则

对接方生成 tool schema 或 help 时，应扫描工具参数中是否存在托管字段。

支持托管身份的对接方：

- 从对模型可见的 `properties` 中移除 `LUASKILL_SID`，并从 `required` 中移除。
- 调用工具前把稳定身份值补回参数对象。
- help 或工具描述应追加托管模式说明，告诉模型不要询问、打印或保存原始托管 ID。
- 如果工具结果包含被注入的原始托管 ID，应在返回给模型前脱敏或改写为托管状态说明。
- 注入值应在同一宿主会话或同一项目任务范围内稳定，不能每次调用随机变化。

不支持托管身份的对接方：

- 保持原始参数 schema 不变。
- 保持 LuaSkill 原始 help 不变。
- 如果 create/start/bootstrap 入口支持缺省生成 ID，可以允许模型不传该字段并由 LuaSkill 生成。
- 对生成出的公开 ID，应引导模型显式回显并询问用户是否保存到项目规则文件。

`codekit-rg` 这类“文本命中回映结构”的工具，建议输出为扁平结构文本，即：

- 返回值建议直接使用 Markdown 纯文本，而不是 JSON 字段包装后的结果对象
- 输出头部应先给出单行扫描摘要，例如扫描文件数、命中文件数、结构命中数、rg 原始命中数与错误数
- 逐文件输出时建议使用 `[文件绝对路径 Lines:总行数 Symbols:符号数]` 作为头部，并直接在下一段输出正文
- 命中结构统一显示为 `@ signature [Lx-y]`；若属于嵌套结构，可使用 `@ 外层结构 :: 内层结构` 表达层级
- 命中行统一显示为 `Lx: text`，并对具体代码文本做首尾空白清理
- 默认不要展开命中函数的完整源码，只显示命中行与结构上下文
- 命中类型/结构声明时，应只显示相关结构链与命中行，不额外展开无关子树
- 不再支持 `export_md_path`；若结果过大，应自动写入 `vulcan.runtime.temp_dir/mcp/cache/`，并返回 raw file 指针与安全分块读取计划，不再返回残缺正文

`codekit-markdown-menu` 这类“文档目录筛选”工具，建议遵循以下规则：

- 仅扫描 `.md` 文件，不尝试解析正文、表格或复杂 Markdown 语义
- 返回值建议直接使用 Markdown 纯文本，而不是 JSON 包裹后的 `content` 字段
- 输出头部应先给出单行扫描统计节点，例如扫描文件数、包含标题的文件数、标题总数与错误数量
- 统计之后再输出 `# FILE MENU` 与按目录分组的文件名列表，然后再给出逐文件的标题目录详情
- 逐文件详情建议使用 `[文件绝对路径 Lines:总行数]` 作为头部，并直接按 `Lx: #/##/### 标题文本` 输出标题行
- 每个文件仅展示 `#`、`##`、`###` 标题和行号，不展示正文内容，也不要再额外包裹 `## [n]`、引用块或代码块
- 支持目录、文件、多路径组合，并对重复 Markdown 文件去重
- 允许目录与文件在一次请求中混用，便于截断后按文件菜单做精确重取
- 对文档根目录做首轮筛选时建议 `recursive=false`，确认相关文档范围后再缩小路径或开启递归
- 不做缓存、不写入 `workdir`、不导出 Markdown 文件；如果客户端侧发生截断，应根据 `# FILE MENU` 判断需要的文件或子目录，并重新调用本工具缩小范围

`codekit-ast-tree` 这类“单目录 AST 导航”工具，建议遵循以下规则：

- 参数使用 `paths`，且当前协议只允许传入一个目录路径
- 多目录输入、文件路径输入都会被明确拒绝
- 返回值直接使用 Markdown 纯文本，而不是 JSON 包裹后的 `content` 字段
- 输出头部先给出扫描摘要，再按子目录分组输出文件级单行 AST 摘要
- 递归扫描是隐式开启的，不再单独暴露 `recursive`
- 支持 `ext` 作为文件扩展名过滤，也支持把 `rust`、`typescript` 这类语言名自动归一化为扩展名集合
- 默认仍启用忽略规则；仅当显式传入 `noignore=true` 时，才关闭 `.gitignore`、`.ignore` 与内建黑名单过滤
- 不支持 `comment` 与 `export_md_path`
- 适合作为仓库级或子系统级的首轮文件筛选入口；真正需要细节时，再转向 `codekit-ast-detail` 或 `codekit-rg`
- 当输出文本超过与 `codekit-ast-detail` 相同的客户端安全预算时，应将完整结果写入 `vulcan.runtime.temp_dir/mcp/cache/`，并返回 raw file 指针与安全分块读取计划，不再拼接残缺正文

`codekit-ast-detail`、`codekit-ast-tree` 与 `codekit-rg` 当前推荐统一采用以下“大结果处理规则”：

- 不再暴露 `cache_id`、`page`、`truncate_chars`、`cache_ttl_sec` 这类工具级缓存/分页参数
- 统一按客户端预算的安全阈值决定是否内联返回，而不是分别维护固定字节阈值
- 字符预算规则建议收敛到独立公共 Lua 文件中，避免不同工具各自维护一套客户端长度映射
- 完整 Markdown 统一写入 `vulcan.runtime.temp_dir/mcp/cache/`，不再写入工作目录，避免缓存文件干扰模型对仓库状态的判断
- 当结果发生落盘时，返回值应改为 raw file 指针块，至少包含原始文件路径、总行数，以及可直接用于宿主 `Read(offset, limit)` 的 chunk 参数（`offset` 为 0-based 起始行，`limit` 为读取行数），同时保留 `start_line/end_line` 作为可读锚点
- `codekit-ast-detail` 不再暴露 `export_md_path`，仅保留内联返回与超限 pointer 两种行为
- `codekit-rg` 也不再暴露 `export_md_path`，仅保留内联返回与超限 pointer 两种行为

`codekit-ast-detail` 在 `comment=true` 场景下，备注提取建议统一如下：

- 备注应输出为压缩后的单行摘要，而不是完整注释块原文
- 需要过滤 `// -----------`、`// ========` 这类分隔线或区域装饰注释
- 需要过滤 `@param`、`@returns`、`参数 / Parameters`、`返回 / Returns` 这类结构化标签说明
- 多行中英文备注应合并为单行，并只保留前部核心有效信息
- 单条备注建议限制在较短字节数内，避免注释挤占结构输出空间

如需批量验证常见备注格式与多语言备注格式，可直接运行：

```powershell
python scripts/verify_vmcp_ast_comment_notes.py
```

默认语言范围建议也统一如下：

- `ext` 的语义应明确为“文件扩展名过滤”，不是编程语言枚举本身
- 当未显式传入 `ext` 时，优先扫描源代码语言，例如 `c/cpp/csharp/go/java/js/ts/tsx/kotlin/lua/php/python/ruby/rust/swift` 等
- 当调用方传入 `rust`、`typescript`、`python` 这类完整语言名时，可在内部自动归一化为对应扩展名集合
- 当调用方传入 `rs`、`ts`、`js` 这类明确扩展名时，应按精确扩展名处理，不要意外放大过滤范围
- 默认排除 `css/html/json/yaml` 这类样式、标记或数据配置格式
- 若确实需要覆盖配置类文件，再显式传入 `ext`
- `noignore` 的语义应统一为“关闭忽略规则”；默认不传时仍启用 `.gitignore`、`.ignore` 和内建黑名单
- 仅当调用方显式传入 `noignore=true` 时，才关闭忽略规则并扫描原本会被过滤的目录树

`codekit-patch` 这类“结构重定位替换”的工具，建议遵循以下规则：

- 只允许 patch function / method 这类完整代码节点
- selector 优先采用宽松的结构路径，例如 `vmm_backend_status_message`、`HostRuntime/vmm_backend_status_message`、`impl HostRuntime/vmm_backend_status_message`
- 唯一命中时直接替换；多命中时返回更完整的候选结构路径，让调用方重试
- `replacement` 必须是完整函数源码，且必须从声明行开始传入
- 不做 `body/auto` 兼容推断；不符合规则时应直接返回结构化错误

## skill.json 关键约定

### 内部模板目录

- `runtime/lua_runtime/skills` 下只有符合技能命名规则的目录才会被宿主自动加载
- 这类目录适合存放内部模板、演示 skill、复制样板
- 推荐保留一个 `__demo` 目录，方便一键复制后改名投入使用

### 依赖声明文件

- 固定文件名：`dependencies.yaml`
- 宿主在加载 skill 前会自动检查该文件是否存在
- 若存在，会由 Rust 统一完成依赖下载，不再执行脚本初始化
- 若不存在，则表示当前 skill 无需外部工具依赖初始化

`dependencies.yaml` 的目标是声明：

- GitHub 仓库地址
- 最新 tag 解析地址
- 当前系统对应的资源文件名
- 本地落库名称（已存在则直接跳过）
- 压缩包内部需要提取的文件路径

当前至少推荐显式覆盖这几类系统目标：

- `windows` + `x86_64`
- `linux` + `x86_64`
- `linux` + `aarch64`
- `macos` + `aarch64`

### 分组与多入口

`skill.json` 顶层使用 `groups` 数组组织入口：

- 每个 group 都可以独立声明多个 `tools`
- 每个 group 都可以独立声明多个 `prompts`
- 每个 group 都可以独立声明多个 `resources`
- 每个 group 都可以独立声明多个 `resource_templates`
- 不同入口可以各自拥有不同名称、参数、Lua 入口文件与描述

推荐分层理解：

- `skill`：目录级封装，负责依赖声明、调试模式与文件归属
- `group`：逻辑分组，便于把相关入口组织在一起
- `tool/resource/prompt/template`：真正暴露给 MCP 的具体入口

### `lancedb` / `lancedb_enable`

如需让某个 skill 获得宿主管理的专属 LanceDB 实例，推荐在 `skill.json` 顶层声明：

```json
{
  "lancedb": {
    "enable": true,
    "log_level": "info",
    "slow_log_enabled": true,
    "slow_log_threshold_ms": 800
  }
}
```

当前仍兼容旧写法：

```json
{
  "lancedb_enable": true
}
```

当前规则固定如下：

- 每个 skill 最多只绑定一个 LanceDB 库
- 库名固定等于 **skill 目录名**
- 宿主会自动使用 `output/lua_runtime/databases/lancedb/<skill_dir_name>` 作为数据库目录
- 若目录不存在，宿主会自动创建
- Lua 不负责创建/删除数据库，只负责在该固定库内创建表、写入、检索和删表
- 未开启 `lancedb_enable` 的 skill 不会获得可用的 `vulcan.lancedb` 上下文
- `log_level` 当前支持：
  - `off`
  - `info`
  - `warning`
- `slow_log_enabled` 控制是否输出慢操作日志
- `slow_log_threshold_ms` 控制慢操作阈值（毫秒）

### 附属能力提供器

`prompts`、`resources`、`resource_templates` 统一只保留 `file` 字段：

- `file` 扩展名为 `.lua`：宿主按生成器执行
- `file` 扩展名不是 `.lua`：宿主按静态文件读取

约定建议：

- 普通文本、Markdown、JSON 等用于稳定静态内容
- `.lua` 文件用于依赖参数、上下文或条件分支的动态内容

### `__demo` 模板建议

推荐模板目录至少包含：

- `tools/demo_template_tool.lua`：工具入口示例
- `tools/demo_template_summary.lua`：第二个工具入口示例（演示多 tool 拆分）
- `skill.json`：最小完整配置
- `dependencies.yaml`：外部依赖声明模板
- `resources/`：静态与动态资源示例
- `templates/`：静态与动态资源模板示例
- `prompts/`：静态与动态提示词示例

### generator 返回约定

#### Prompt generator

推荐返回以下任一形式：

- 完整 `PromptGetResult`
- `{ "messages": [...] }`
- `{ "text": "...", "role": "user" }`
- 纯字符串（宿主会包装成单条 `user` 消息）

#### Resource / ResourceTemplate generator

推荐返回以下任一形式：

- 完整 `ResourceReadResult`
- `{ "contents": [...] }`
- `{ "text": "...", "mimeType": "text/plain" }`
- 纯字符串（宿主会包装成单个文本资源）

## `vulcan` 模块

Rust 侧注册到 Lua 全局的扩展模块。

### `vulcan.runtime.log(level, msg)`

打印带级别的日志到 stderr。

```lua
vulcan.runtime.log("info", "scanning directory: " .. dir)
vulcan.runtime.log("warn", "file not found")
vulcan.runtime.log("error", "parse failed")
```

### `print(...)`

Lua 全局标准输出函数。常规 skill 环境下会进入宿主日志链路；`luaexec` 隔离执行环境下会被捕获到最终返回结果中。

```lua
print("found:", #files, "files")
print(fn_name, line_num, kind)
```

### `vulcan.fs.list(dir) -> table`

列出目录下所有文件/子目录名。

```lua
local entries = vulcan.fs.list("src/")
for _, name in ipairs(entries) do
    print(name)
end
```

### `vulcan.fs.read(path) -> string`

读取文件全部内容（文本模式）。

```lua
local content = vulcan.fs.read("config.yaml")
```

### `vulcan.fs.write(path, content)`

写入文件（覆盖模式）。

```lua
vulcan.fs.write("output.json", vulcan.json.encode(data))
```

### `vulcan.fs.exists(path) -> boolean`

判断文件或目录是否存在。

```lua
if vulcan.fs.exists("cache.json") then
    local cache = vulcan.json.decode(vulcan.fs.read("cache.json"))
end
```

### `vulcan.fs.is_dir(path) -> boolean`

判断是否为目录。

```lua
if vulcan.fs.is_dir(path) then
    vulcan.runtime.log("info", "skipping directory: " .. path)
end
```

### `vulcan.os.info() -> table`

返回当前平台信息。

```lua
local info = vulcan.os.info()
print(info.os, info.arch)
-- windows  x86_64
```

### `vulcan.path.join(...) -> string`

拼接路径，自动使用平台分隔符。

```lua
local full = vulcan.path.join("src", "utils", "helper.lua")
-- Windows: src\utils\helper.lua
-- Unix:    src/utils/helper.lua
```

### `vulcan.json.encode(table) -> string`

Lua table 转 JSON 字符串。

```lua
local json = vulcan.json.encode({ name = "test", count = 42 })
-- => '{"name":"test","count":42}'
```

### `vulcan.json.decode(string) -> table`

JSON 字符串转 Lua table。

```lua
local t = vulcan.json.decode('{"name":"test","count":42}')
print(t.name)  -- test
```

### `vulcan.call(skill_name, args) -> any`

在 Lua 内调用其他已加载 skill。

```lua
local result = vulcan.call("codekit-ast-detail", { paths = "src/main.rs" })
print(result)
```

### `vulcan.lancedb`

仅当当前 skill 在 `skill.json` 中显式声明 `lancedb_enable: true` 时，宿主才会注入 `vulcan.lancedb`。

当前最小能力面包括：

- `vulcan.lancedb.status()`
- `vulcan.lancedb.info()`
- `vulcan.lancedb.create_table(input)`
- `vulcan.lancedb.vector_upsert(input)`
- `vulcan.lancedb.vector_search(input)`
- `vulcan.lancedb.delete(input)`
- `vulcan.lancedb.drop_table(input)`

其中：

- `create_table/delete/drop_table` 直接接收 Lua table，宿主会转成对应 JSON 输入
- `vector_upsert` 支持：
  - `rows = {...}`：宿主自动按 JSON Rows 编码
  - `data = "..."`：按原始 bytes 传递
- `vector_search` 默认按 `json` 输出格式返回，并把结果行挂到 `data_json`
- `status()` 在未启用 LanceDB 时也能稳定返回 `{ enabled = false, initialized = false, ... }`
- `info()` 在未启用 LanceDB 时也会返回相同结构，方便 Lua 侧先做状态判断
- 若当前 skill 未启用 LanceDB，真正的写操作接口会返回“当前 skill 未启用 lancedb”错误

### `vulcan.runtime.temp_dir -> string`

返回宿主提供的 MCP 临时目录绝对路径。当前固定为 `<application_root>/lua_runtime/temp`，例如调试构建为 `output/lua_runtime/temp`。

```lua
local spill_root = vulcan.path.join(vulcan.runtime.temp_dir, "mcp", "cache")
print("temp spill root:", spill_root)
```

## LuaJIT 标准库

LuaJIT 完整标准库均已加载，以下是常用模块概览。

### `string` — 字符串处理

```lua
local s = string.gsub("hello world", "world", "lua")
local m = string.match("func(123)", "func%((%d+)%)")
local f = string.format("count: %d, name: %s", 5, "test")
```

### `table` — 表操作

```lua
local t = { 3, 1, 2 }
table.sort(t)
table.insert(t, 4)
local s = table.concat(t, ", ")
```

### `math` — 数学函数

```lua
local n = math.floor(3.7)
local r = math.random(1, 100)
local m = math.max(a, b)
```

### `bit` — 位运算（LuaJIT 特有）

```lua
local a = bit.band(0xFF, 0x0F)
local b = bit.lshift(1, 8)
local c = bit.bor(x, y)
```

### `os` — 操作系统

```lua
local t = os.time()
local d = os.date("%Y-%m-%d %H:%M:%S")
local env = os.getenv("PATH")
```

### `io` — 文件 I/O

```lua
-- 读取
local f = io.open("file.txt", "r")
local content = f:read("*a")
f:close()

-- 写入
local f = io.open("file.txt", "w")
f:write("hello\n")
f:close()
```

### `debug` — 调试

```lua
local tb = debug.traceback()
local info = debug.getinfo(1)  -- 当前函数信息
```

### `jit` — LuaJIT JIT 控制

```lua
jit.off()       -- 关闭 JIT 编译
jit.on()        -- 开启
jit.flush()     -- 清空已编译代码
```

### `ffi` — LuaJIT C FFI

可直接调用 C 函数，无需写胶水代码。

```lua
local ffi = require "ffi"
ffi.cdef[[
    int printf(const char *fmt, ...);
]]
ffi.C.printf("Hello from C!\n")
```

### `package` — 模块加载

```lua
require "module_name"
package.path  -- Lua 模块搜索路径
package.cpath -- C 模块搜索路径
```

## 第三方 C 模块

通过 luarocks 安装的 C 扩展库，`require` 即可使用。

### `cjson` — JSON 处理

比 `vulcan.json.encode/decode` 更快的 JSON 库。

```lua
local json = require "cjson"
local s = json.encode({ a = 1, b = "hello" })
local t = json.decode(s)
```

### `lfs` — LuaFileSystem

文件系统操作（目录遍历、文件属性）。

```lua
local lfs = require "lfs"
for entry in lfs.dir("/tmp") do
    local attr = lfs.attributes("/tmp/" .. entry)
    if attr.mode == "directory" then
        print("dir:", entry)
    end
end
```

### `socket` + `ssl` — 网络通信

HTTP/TCP 客户端，支持 SSL。

```lua
local http = require "socket.http"
local body, code = http.request("https://api.example.com/data")
```

### `lua-curl` — libcurl 绑定

提供基于 libcurl 的网络请求能力，适合需要直接使用 curl 语义的场景。

```lua
local curl = require "lcurl.safe"
local easy = curl.easy {
  url = "https://example.com",
}
easy:perform()
easy:close()
```

### `openssl` — 加密库

```lua
local openssl = require "openssl"
local digest = openssl.digest("sha256", "hello world")
```

### `lyaml` — YAML 解析

```lua
local lyaml = require "lyaml"
local t = lyaml.load("key: value\nlist:\n  - a\n  - b")
local s = lyaml.dump({ key = "value" })
```

### `rex_pcre2` — PCRE2 正则

```lua
local rex = require "rex_pcre2"
local m = rex.match("hello 123", "(%d+)")
```

### `zlib` — 压缩

```lua
local zlib = require "zlib"
local compressed = zlib.deflate()(raw_data, "finish")
```

### `toml` — TOML 解析

```lua
local toml = require "toml"
local t = toml.parse("key = \"value\"\n[section]\nnum = 42")
```

## Skill 依赖下载

如果 skill 目录下存在 `dependencies.yaml`，宿主会在加载前自动处理依赖下载。

```yaml
dependencies:
  - name: "ast-grep"
    install_as: "ast-grep.exe"
    github:
      repo: "https://github.com/ast-grep/ast-grep"
      tag_api: "https://api.github.com/repos/ast-grep/ast-grep/releases/latest"
    targets:
      - os: "windows"
        arch: "x86_64"
        asset_name: "app-x86_64-pc-windows-msvc.zip"
        install_as: "ast-grep.exe"
        archive_path: "ast-grep.exe"
        executable: false
      - os: "linux"
        arch: "x86_64"
        asset_name: "app-x86_64-unknown-linux-gnu.zip"
        install_as: "ast-grep"
        archive_path: "ast-grep"
        executable: true
      - os: "linux"
        arch: "aarch64"
        asset_name: "app-aarch64-unknown-linux-gnu.zip"
        install_as: "ast-grep"
        archive_path: "ast-grep"
        executable: true
      - os: "macos"
        arch: "aarch64"
        asset_name: "app-aarch64-apple-darwin.zip"
        install_as: "ast-grep"
        archive_path: "ast-grep"
        executable: true
```

运行规则：

- 宿主提供可执行文件目录固定为 LuaSkills 运行根下的 `bin/`（正式构建为 `output/lua_runtime/bin/`），技能的版本化命令行工具进入 `output/lua_runtime/dependencies/tools/`
- `vldb-controller(.exe)` 固定放在 `output/lua_runtime/bin/`，不属于版本化依赖工具树
- 当前 `vulcan-agent-service` 产品固定采用 controller-only 数据库访问模型，skill 应假设数据库能力由宿主通过 controller 统一提供
- 会先检查 `install_as` 对应文件是否已存在，存在则直接跳过
- 支持 `asset_name`、`install_as`、`archive_path` 中使用 `{tag}` 与 `{version}` 占位符
- 当前支持直接文件、`.zip`、`.tar.gz` / `.tgz` 安装
- 下载过程由 Rust 统一显示进度条

## Skill 调试模式

在 `skill.json` 中设置 `"debug": true`，每次调用时自动从磁盘重新加载对应 tool 的 `lua_entry` 文件。

```json
{
  "name": "vulcan-codekit",
  "debug": true,
  ...
}
```

源码变化时输出日志：
```
[LuaSkill] Hot reload codekit-ast-detail: <repo_root>\output\lua_runtime\skills\vulcan-codekit\runtime\codekit-ast-detail.lua
```

## Skill 模板

```lua
-- my_skill/tools/my_skill.lua

return function(args)
    local dir = args.dir or "."
    local recursive = args.recursive or false

    -- 处理逻辑
    local results = {}
    local content = table.concat({
        "# My Skill Result",
        "",
        "- dir: `" .. dir .. "`",
        "- recursive: `" .. tostring(recursive) .. "`",
        "- count: `" .. tostring(#results) .. "`",
    }, "\n")

    return content
end
```

如需让宿主接管超限处理，可改为：

```lua
return content, vulcan.runtime.overflow_type.truncate
```

或：

```lua
return content, vulcan.runtime.overflow_type.page, "overflow_page.md"
```

```json
// my_skill/skill.json
{
  "name": "my_skill",
  "debug": false,
  "groups": [
    {
      "name": "core",
      "description": "Primary entries for the skill",
      "tools": [
        {
          "name": "my_skill",
          "description": "Tool description for tools/list",
          "lua_entry": "tools/my_skill.lua",
          "lua_module": "my_skill",
          "parameters": [
            {
              "name": "dir",
              "type": "string",
              "description": "Target directory",
              "required": true
            }
          ],
          "prompt": "AI usage hint for the tool"
        },
        {
          "name": "my_skill_summary",
          "description": "Second tool entry example in the same skill",
          "lua_entry": "tools/my_skill_summary.lua",
          "lua_module": "my_skill_summary",
          "parameters": [],
          "prompt": "Optional hint for another grouped tool"
        }
      ],
      "resources": [
        {
          "uri": "skill://my-skill/guide",
          "name": "My Skill Guide",
          "description": "Static resource example",
          "mime_type": "text/markdown",
          "file": "resources/guide.md"
        }
      ],
      "resource_templates": [
        {
          "uri_template": "skill://my-skill/example/{topic}",
          "name": "My Skill Example",
          "description": "Template example",
          "mime_type": "text/markdown",
          "file": "templates/example.md"
        }
      ],
      "prompts": [
        {
          "name": "my_skill_first_pass",
          "description": "Prompt example",
          "file": "prompts/first_pass.md",
          "role": "user",
          "arguments": [
            {
              "name": "target",
              "description": "Target path",
              "required": false
            }
          ]
        }
      ]
    }
  ]
}
```

```yaml
# my_skill/dependencies.yaml
dependencies: []
```

### 设计建议

- 一个 skill 可以只有 `prompts/resources/templates`，不一定必须声明 `tools`
- 一个 skill 也可以声明多个 tool 入口，共享同一个目录与依赖声明文件
- 如果多个 tool 复用同一个 Lua 文件，请确保它们的 `lua_module` 唯一
- 建议始终保留 `__demo` 目录作为“多 group、多入口”的复制模板

## `vulcan-lua` 工具说明

`vulcan-lua` 是当前仓库内置的运行时执行 skill，当前提供一个统一执行工具：

- `vulcan-lua-run`

包级帮助说明仍然保留在：

- `vulcan-help-detail`
  - `skill=vulcan-lua`
  - `flow=main`

它们都遵循当前工具返回规则：

- tool 必须返回字符串
- `vulcan-lua` 的执行结果固定返回 Markdown 字符串
- `print(...)` 会被捕获到返回结果中
- `return table` 会转成格式化 JSON 文本
- 多返回值会按顺序逐项展示
- 长输出只允许截断，不分页

### `vulcan-lua-run`

用于统一执行一段临时 Lua 代码，或执行一个已有 Lua 文件。

输入结构：

```json
{
  "task": "可选任务说明",
  "code": "可选，内联 Lua 源码",
  "file": "可选，Lua 文件路径",
  "args": {},
  "timeout_ms": 60000
}
```

字段语义：

- `task`
  - 可选
  - 默认不传
  - 仅用于结果头部展示
- `code`
  - 可选
  - 默认不传
  - 用于短小的内联 Lua 执行
- `file`
  - 可选
  - 默认不传
  - 用于执行一个现有 `.lua` 文件
  - 运行时会自动切换 `cwd` 到该文件所在目录
- `args`
  - 可选
  - 默认 `{}` 
  - 会在执行环境中以局部变量 `args` 暴露
- `timeout_ms`
  - 可选
  - 默认 `60000`

额外约束：

- `code` 与 `file` 必须且只能传一个
- 传 `code` 时，适合一次性、短小、临时执行
- 传 `file` 时，适合已有脚本、需要相对路径、或多步骤逻辑
- `task` 如果传入，必须是字符串
- `args` 如果传入，必须是对象 table
- `timeout_ms` 如果传入，必须是大于 `0` 的数字
- 纯空白字符串会按“未传入”处理
- 输入不合法时，不会进入执行阶段，而是直接返回 `Runtime Input Error`

适用场景建议：

- 短循环
- 临时文件生成
- 一次性数据转换
- 快速网络探测
- 命令编排
- 已沉淀成独立脚本的多步骤逻辑

运行时行为：

- 使用 `file` 模式时，执行期间自动把 `cwd` 切换到目标文件目录
- 同时注入：
  - `vulcan.context.entry_file`
  - `vulcan.context.entry_dir`
- 返回结果末尾会固定追加一个 `Current Client Context` 区块
- 该区块展示的是发起当前 `vulcan-lua-run` 调用的外层真实客户端上下文
- 该区块会包含：
  - `client_kind`
  - `client_name`
  - `tool_result_bytes_limit`
  - `tool_result_line_limit`
  - `file_read_bytes_limit`
  - `file_read_line_limit`

### `vulcan-lua` 的额外边界

- 执行环境内 `vulcan.luaexec` 会被移除，因此不允许递归再次进入执行器
- 执行环境内 `vulcan.log` 与 `vulcan.cache_*` 不注册
- 执行环境内允许 `vulcan.call(name, args)` 调用其它工具，但它主要是兼容与组合能力，不推荐作为常规主路径
- 内部工具调用会以受限模拟客户端 `luaexec_call` 执行，目前默认预算是：
  - `tool_result.bytes = 10000`
  - `tool_result.lines = -1`
- 因此脚本内部如果主动读取 `vulcan.context.request`，看到的可能是内部 `luaexec_call`
- 需要判断真实调用方时，应以 `vulcan-lua-run` 返回里的 `Current Client Context` 区块为准
- 宿主对字节预算会再应用安全比例，因此 Lua 实际拿到的最终 `bytes` 可能小于配置原值
- 同时仍禁止：
  - 调用当前发起 `luaexec` 的工具自身
- 在 `vulcan-lua-run` 中再次调用当前执行工具

## `vulcan-curl` 工具说明

`vulcan-curl` 是当前仓库内置的 HTTP 调用 skill，底层直接使用 `lua-curl`，不依赖系统 `curl` 可执行文件。

它的目标是：

- 尽量保持 Linux `curl` 的使用心智
- 避免 Windows / Linux / macOS 下不同 shell 的转义差异
- 让 AI 直接通过结构化参数数组复用熟悉的 `curl` 参数风格
- 同时提供更适合 AI 的极简 GET / POST 快捷工具

### `vulcan-curl-get`

这是给 AI 使用的极简 GET 工具，适合：

- 只提供 `url`
- 只带简单 `params`
- 只带简单 `headers`
- 不关心复杂 TLS、代理、重试、上传和 curl 高级参数

输入结构：

```json
{
  "url": "https://httpbin.org/get",
  "params": {
    "q": "hello",
    "page": 1
  },
  "headers": {
    "Accept": "application/json"
  },
  "timeout_ms": 30000
}
```

说明：

- `params` 支持对象形式
- 若调用侧需要显式传递原始查询片段数组，可使用 `params_list`
- 对象形式会自动 URL 编码并拼到查询串
- `headers` 为对象型头映射，`header_lines` 为原始头字符串数组
- `bearer` 可快捷注入 `Authorization: Bearer ...`
- `basic` 为对象型基础认证，`basic_text` 可直接传 `user:pass`
- `follow_location` 可用于常见 30x 跳转跟随
- `download_to` 可将响应体直接保存到文件
- `save_headers_to` 可将响应头保存到文件
- 默认不返回请求详情和响应头；需要时通过 `flags` 显式传入逗号分隔字符串，例如 `"flags":"response-header"` 或 `"flags":"request-header,response-header"`
- 逗号两侧允许空格，例如 `"flags":"request-header , response-header"`；未知项会被忽略
- 若需要复杂 curl 参数、代理、证书、输出文件、重试等，请回退使用基础 `vulcan-curl`

### `vulcan-curl-post`

这是给 AI 使用的极简 POST 工具，适合：

- 简单 JSON 请求
- 简单表单请求
- 简单原始 body 请求

输入结构：

```json
{
  "url": "https://httpbin.org/post",
  "json": {
    "hello": "world"
  },
  "headers": {
    "Accept": "application/json"
  },
  "timeout_ms": 30000
}
```

说明：

- `json`、`body` 与 `form/files` 三种负载族只能三选一
- `form` 为对象型表单映射，`form_lines` 为 curl 风格表单数组
- `files` 为对象型文件映射，`file_lines` 为 curl 风格文件数组
- `form` 与 `files` 可以组合成常见 multipart 请求
- 查询参数若需要显式数组输入，可使用 `params_list`
- `headers` 为对象型头映射，`header_lines` 为原始头字符串数组
- `bearer` 可快捷注入 `Authorization: Bearer ...`
- `basic` 为对象型基础认证，`basic_text` 可直接传 `user:pass`
- `follow_location` 可用于常见 30x 跳转跟随
- `download_to` 可将响应体直接保存到文件
- `save_headers_to` 可将响应头保存到文件
- 默认不返回请求详情和响应头；需要时通过 `flags` 显式传入逗号分隔字符串，例如 `"flags":"response-header"` 或 `"flags":"request-header,response-header"`
- 逗号两侧允许空格，例如 `"flags":"request-header , response-header"`；未知项会被忽略
- 若需要文件上传、复杂 TLS、代理、重试、更多 curl 参数，请回退使用基础 `vulcan-curl`

文件上传示例：

```json
{
  "url": "https://httpbin.org/post",
  "form": {
    "name": "alice"
  },
  "files": {
    "upload": "D:/projects/demo/report.txt"
  }
}
```

### `vulcan-curl`

当前 skill 的基础工具为：

- `vulcan-curl`

输入结构：

```json
{
  "args": ["-X", "POST", "https://example.com/api", "--json", "{\"hello\":\"world\"}"],
  "cwd": "D:/workspace",
  "timeout_ms": 60000
}
```

字段语义：

- `args`
  - 必填
  - curl 风格参数数组
  - 推荐**不要**包含前导 `curl`
  - 如果传了前导 `curl`，工具也会自动剥离
- `cwd`
  - 可选
  - 用于解析相对文件路径
  - 会影响：
    - `-o/--output`
    - `-F @file`
    - `-F <file`
    - `--cacert`
    - `--cert`
    - `--key`
    - `--cookie-jar`
- `timeout_ms`
  - 可选
  - 当未显式提供 `--max-time` 时，作为默认请求超时
  - 默认 `60000`
- `flags`
  - 可选
  - 默认为空，不返回请求详情和响应头
  - 传递格式为逗号分隔字符串，例如 `"flags":"response-header"` 或 `"flags":"request-header,response-header"`
  - 逗号两侧允许空格，例如 `"flags":"request-header , response-header"`
  - `request-header` 用于返回请求详情
  - `response-header` 用于返回响应头
  - 未知项会被忽略

### 当前第一版已支持的常见 curl 参数

- `-X`, `--request`
- `-H`, `--header`
- `-d`, `--data`, `--data-raw`, `--data-binary`
- `--data-urlencode`
- `--json`
- `-F`, `--form`
- `-u`, `--user`
- `-A`, `--user-agent`
- `-e`, `--referer`
- `-L`, `--location`
- `-I`, `--head`
- `-G`, `--get`
- `-k`, `--insecure`
- `-o`, `--output`
- `-D`, `--dump-header`
- `-m`, `--max-time`
- `--connect-timeout`
- `--retry`
- `--retry-delay`
- `--retry-max-time`
- `--proxy`
- `--proxy-user`
- `-b`, `--cookie`
- `-c`, `--cookie-jar`
- `--cacert`
- `--capath`
- `-E`, `--cert`, `--key`
- `--compressed`
- `-f`, `--fail`
- `--fail-with-body`
- `-i`, `--include`
- `--http1.1`
- `--http2`

当前不在第一版支持范围内的参数，会直接返回明确错误，而不是静默忽略。

### 当前输出规则

- 工具固定返回 Markdown 字符串
- 成功时返回：
  - 请求方法
  - 请求 URL
  - 最终 URL
  - 状态码
  - 尝试次数
  - 响应头
  - 响应体（若未使用 `-o`）
- 如果使用 `-o`，则：
  - 响应内容写入文件
  - 工具只返回输出文件路径和响应头摘要

### 现实边界说明

- 该工具底层走的是 `lua-curl`，不是系统 `curl`
- 因此它解决的是：
  - shell 差异
  - 引号转义
  - PowerShell / pwsh / sh 差异
- 它不解决目标网络环境本身的证书或代理问题
- Windows 下在未显式传入 `-k`、`--cacert`、`--capath` 时，会优先尝试使用系统原生 CA 存储
- 如果当前网络环境存在 TLS 中间代理或证书校验问题，仍然可能需要：
  - `-k`
  - 或显式提供 `--cacert`

### 适用场景建议

- 调 OpenAI / GitHub / 通用 REST API
- 发送 JSON 请求
- 上传简单表单
- 保存返回内容到文件
- 让 AI 继续沿用 curl 的参数心智，但不再直接拼 shell 命令
