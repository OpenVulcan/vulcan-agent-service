# Lua Skill 开发指南

## 环境

- **引擎**: LuaJIT 2.1 (兼容 Lua 5.2)
- **模式**: `unsafe`（允许 C 模块加载，可使用 FFI）
- **入口**: 每个 tool 入口的 `lua_entry` 文件返回 `function(args)`，附属能力入口通过各自的 `file` 字段绑定静态文件或 `.lua` 生成器

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

### 附属能力提供器

`prompts`、`resources`、`resource_templates` 统一只保留 `file` 字段：

- `file` 扩展名为 `.lua`：宿主按生成器执行
- `file` 扩展名不是 `.lua`：宿主按静态文件读取

约定建议：

- 普通文本、Markdown、JSON 等用于稳定静态内容
- `.lua` 文件用于依赖参数、上下文或条件分支的动态内容

### `__demo` 模板建议

推荐模板目录至少包含：

- `main.lua`：工具入口示例
- `main_summary.lua`：第二个工具入口示例（演示多 tool 拆分）
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

### `vulcan.print(...)`

类似 Lua `print()`，支持多参数、自动类型转换，输出到 stderr。

```lua
vulcan.print("found:", #files, "files")
vulcan.print(fn_name, line_num, kind)
```

### `vulcan.fs_list(dir) -> table`

列出目录下所有文件/子目录名。

```lua
local entries = vulcan.fs_list("src/")
for _, name in ipairs(entries) do
    vulcan.print(name)
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
vulcan.print(info.os, info.arch)
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
vulcan.print(t.name)  -- test
```

### `vulcan.call(skill_name, args) -> any`

在 Lua 内调用其他已加载 skill。

```lua
local result = vulcan.call("vmcp-ast", { path = "src/", recursive = true })
vulcan.print("found", result.items_found, "items")
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
        vulcan.print("dir:", entry)
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
  "name": "ast-grep",
  "debug": true,
  ...
}
```

源码变化时输出日志：
```
[LuaSkill] Hot reload ast_grep: D:\projects\vulcan-mcp-client\output\lua_skills\ast-grep\main.lua
```

## Skill 模板

```lua
-- my_skill/main.lua

return function(args)
    local dir = args.dir or "."
    local recursive = args.recursive or false

    -- 处理逻辑
    local results = {}

    return {
        success = true,
        count = #results,
        items = results,
    }
end
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
          "lua_entry": "main.lua",
          "lua_module": "my_skill",
          "parameters": [
            {
              "name": "dir",
              "type": "string",
              "description": "Target directory",
              "required": true
            }
          ],
          "return_type": "table",
          "prompt": "AI usage hint for the tool"
        },
        {
          "name": "my_skill_summary",
          "description": "Second tool entry example in the same skill",
          "lua_entry": "main.lua",
          "lua_module": "my_skill_summary",
          "parameters": [],
          "return_type": "table",
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
