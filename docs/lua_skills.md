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

`codekit-rg` 这类“文本命中回映结构”的工具，建议输出为扁平结构文本，即：

- 返回值建议直接使用 Markdown 纯文本，而不是 JSON 字段包装后的结果对象
- 输出头部应先给出单行扫描摘要，例如扫描文件数、命中文件数、结构命中数、rg 原始命中数与错误数
- 逐文件输出时建议使用 `[文件绝对路径 Lines:总行数 Symbols:符号数]` 作为头部，并直接在下一段输出正文
- 命中结构统一显示为 `@ signature [Lx-y]`；若属于嵌套结构，可使用 `@ 外层结构 :: 内层结构` 表达层级
- 命中行统一显示为 `Lx: text`，并对具体代码文本做首尾空白清理
- 默认不要展开命中函数的完整源码，只显示命中行与结构上下文
- 命中类型/结构声明时，应只显示相关结构链与命中行，不额外展开无关子树
- 不再支持 `export_md_path`；若结果过大，应自动写入 `vulcan.temp_dir/mcp/cache/`，并返回 raw file 指针与安全分块读取计划，不再返回残缺正文

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
- 当输出文本超过与 `codekit-ast-detail` 相同的客户端安全预算时，应将完整结果写入 `vulcan.temp_dir/mcp/cache/`，并返回 raw file 指针与安全分块读取计划，不再拼接残缺正文

`codekit-ast-detail`、`codekit-ast-tree` 与 `codekit-rg` 当前推荐统一采用以下“大结果处理规则”：

- 不再暴露 `cache_id`、`page`、`truncate_chars`、`cache_ttl_sec` 这类工具级缓存/分页参数
- 统一按客户端预算的安全阈值决定是否内联返回，而不是分别维护固定字节阈值
- 字符预算规则建议收敛到独立公共 Lua 文件中，避免不同工具各自维护一套客户端长度映射
- 完整 Markdown 统一写入 `vulcan.temp_dir/mcp/cache/`，不再写入工作目录，避免缓存文件干扰模型对仓库状态的判断
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
- selector 优先采用宽松的结构路径，例如 `with_vmm`、`McpServer/with_vmm`、`impl McpServer/with_vmm`
- 唯一命中时直接替换；多命中时返回更完整的候选结构路径，让调用方重试
- `replacement` 必须是完整函数源码，且必须从声明行开始传入
- 不做 `body/auto` 兼容推断；不符合规则时应直接返回结构化错误

## skill.json 关键约定

### 内部模板目录

- `lua_skills` 下凡是以 `__` 开头的目录，宿主都会跳过自动加载
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
- 宿主会自动使用 `__lancedb/<skill_dir_name>` 作为数据库目录
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

### `vulcan.log(level, msg)`

打印带级别的日志到 stderr。

```lua
vulcan.log("info", "scanning directory: " .. dir)
vulcan.log("warn", "file not found")
vulcan.log("error", "parse failed")
```

### `print(...)`

Lua 全局标准输出函数。常规 skill 环境下会进入宿主日志链路；`luaexec` 隔离执行环境下会被捕获到最终返回结果中。

```lua
print("found:", #files, "files")
print(fn_name, line_num, kind)
```

### `vulcan.fs_list(dir) -> table`

列出目录下所有文件/子目录名。

```lua
local entries = vulcan.fs_list("src/")
for _, name in ipairs(entries) do
    print(name)
end
```

### `vulcan.fs_read(path) -> string`

读取文件全部内容（文本模式）。

```lua
local content = vulcan.fs_read("config.yaml")
```

### `vulcan.fs_write(path, content)`

写入文件（覆盖模式）。

```lua
vulcan.fs_write("output.json", vulcan.json_encode(data))
```

### `vulcan.fs_exists(path) -> boolean`

判断文件或目录是否存在。

```lua
if vulcan.fs_exists("cache.json") then
    local cache = vulcan.json_decode(vulcan.fs_read("cache.json"))
end
```

### `vulcan.fs_is_dir(path) -> boolean`

判断是否为目录。

```lua
if vulcan.fs_is_dir(path) then
    vulcan.log("info", "skipping directory: " .. path)
end
```

### `vulcan.osinfo() -> table`

返回当前平台信息。

```lua
local info = vulcan.osinfo()
print(info.os, info.arch)
-- windows  x86_64
```

### `vulcan.path_join(...) -> string`

拼接路径，自动使用平台分隔符。

```lua
local full = vulcan.path_join("src", "utils", "helper.lua")
-- Windows: src\utils\helper.lua
-- Unix:    src/utils/helper.lua
```

### `vulcan.json_encode(table) -> string`

Lua table 转 JSON 字符串。

```lua
local json = vulcan.json_encode({ name = "test", count = 42 })
-- => '{"name":"test","count":42}'
```

### `vulcan.json_decode(string) -> table`

JSON 字符串转 Lua table。

```lua
local t = vulcan.json_decode('{"name":"test","count":42}')
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

### `vulcan.temp_dir -> string`

返回宿主提供的 MCP 临时目录绝对路径。当前规则为“程序目录的上级目录下的 `temp` 目录”，例如调试构建常见为 `output/temp`。

```lua
local spill_root = vulcan.path_join(vulcan.temp_dir, "mcp", "cache")
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

比 `vulcan.json_encode/decode` 更快的 JSON 库。

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

- 下载目标目录固定为 `lua_skills/__tools/bin/`
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
[LuaSkill] Hot reload codekit_ast_detail: D:\projects\vulcan-mcp-client\output\lua_skills\vulcan-codekit\tools\codekit-ast-detail.lua
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
return content, vulcan.overflow_type.truncate
```

或：

```lua
return content, vulcan.overflow_type.page, "overflow_page.md"
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

## `vulcan-runtime` 工具说明

`vulcan-runtime` 是当前仓库内置的运行时执行 skill，主要提供三个工具：

- `vulcan-lua-help`
- `vulcan-lua-exec`
- `vulcan-lua-file`

它们都遵循当前工具返回规则：

- tool 必须返回字符串
- `vulcan-runtime` 的执行结果固定返回 Markdown 字符串
- `print(...)` 会被捕获到返回结果中
- `return table` 会转成格式化 JSON 文本
- 多返回值会按顺序逐项展示
- 长输出只允许截断，不分页

### `vulcan-lua-help`

无参数帮助工具，用来查看当前运行时能力边界。

它负责说明：

- 什么时候该用 `vulcan-lua-exec`
- 什么时候该用 `vulcan-lua-file`
- 当前支持的 `vulcan.*` API
- 当前禁用能力
- 输出规则
- 超时规则
- 当前构建实际带上的 Lua 扩展库清单

推荐约束：

- 在调用 `vulcan-lua-exec` 或 `vulcan-lua-file` 前，先调用一次 `vulcan-lua-help`
- 帮助工具不需要重复讲解 Lua 标准库本身；重点应放在宿主扩展能力和工具边界

### `vulcan-lua-exec`

用于执行一段临时 Lua 代码。

输入结构：

```json
{
  "task": "可选任务说明",
  "code": "必填，Lua 源码",
  "args": {},
  "timeout_ms": 60000
}
```

字段语义：

- `task`
  - 可选
  - 仅用于结果头部展示
- `code`
  - 必填
  - 真正执行的 Lua 代码
- `args`
  - 可选
  - 会在执行环境中以局部变量 `args` 暴露
- `timeout_ms`
  - 可选
  - 默认 `60000`

适用场景建议：

- 短循环
- 临时文件生成
- 一次性数据转换
- 快速网络探测
- 命令编排

### `vulcan-lua-file`

用于执行一个已有 Lua 文件。

输入结构：

```json
{
  "task": "可选任务说明",
  "file": "必填，Lua 文件路径",
  "args": {},
  "timeout_ms": 60000
}
```

字段语义：

- `file`
  - 必填
  - 指向要执行的 Lua 文件
- 其余字段与 `vulcan-lua-exec` 一致

运行时行为：

- 执行期间自动把 `cwd` 切换到目标文件目录
- 同时注入：
  - `vulcan.entry_file`
  - `vulcan.entry_dir`

适用场景建议：

- 逻辑已经沉淀成独立脚本
- 需要文件相对路径能力
- 多步骤逻辑写成独立文件更清晰

### `vulcan-runtime` 的额外边界

- 执行环境内 `vulcan.luaexec` 会被移除，因此不允许递归再次进入执行器
- 执行环境内 `vulcan.log` 与 `vulcan.cache_*` 不注册
- 执行环境内允许 `vulcan.call(name, args)` 调用其它工具，但它主要是兼容与组合能力，不推荐作为常规主路径
- 内部工具调用会以受限模拟客户端 `luaexec_call` 执行，目前默认预算是：
  - `tool_result.bytes = 10000`
  - `tool_result.lines = -1`
- 宿主对字节预算会再应用安全比例，因此 Lua 实际拿到的最终 `bytes` 可能小于配置原值
- 同时仍禁止：
  - 调用当前发起 `luaexec` 的工具自身
  - 在 `vulcan-lua-exec` / `vulcan-lua-file` 中再次调用这两个执行工具
