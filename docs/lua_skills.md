# Lua Skill 开发指南

## 环境

- **引擎**: LuaJIT 2.1 (兼容 Lua 5.2)
- **模式**: `unsafe`（允许 C 模块加载，可使用 FFI）
- **入口**: 每个 skill 的 `main.lua` 返回 `function(args)` 作为入口

## skill.json 关键约定

### 内部模板目录

- `lua_skills` 下凡是以 `__` 开头的目录，宿主都会跳过自动加载
- 这类目录适合存放内部模板、演示 skill、复制样板
- 推荐保留一个 `__demo` 目录，方便一键复制后改名投入使用

### 初始化脚本

- 仅支持：`init_scripts.ps1` / `init_scripts.sh`
- 宿主会根据当前系统选择对应脚本
- 不再支持旧版单字段 `init_script`

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
- `skill.json`：最小完整配置
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
local result = vulcan.call("codeview_ts", { dir = "src/", recursive = true })
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

## Skill 初始化脚本

在 `skill.json` 中设置 `"init_scripts"`，skill 加载前会按当前系统选择脚本执行。

```json
{
  "name": "codeview_ast",
  "init_scripts": {
    "ps1": "init.ps1",
    "sh": "init.sh"
  },
  ...
}
```

- Windows 使用 `powershell.exe -File` 执行，Unix 使用 `sh` 执行
- 环境变量：
  - `SKILL_DIR` — skill 目录路径
  - `TOOLS_DIR` — `<exe_parent>/tools/` 目录，用于存放通用工具（如 unzip、7z）
- 脚本 stdout/stderr 通过 `[LuaSkill:init]` 日志输出
- 脚本返回非零 → 跳过该 skill

典型用途：从 GitHub 下载最新依赖二进制。

## Skill 调试模式

在 `skill.json` 中设置 `"debug": true`，每次调用时自动从磁盘重新加载 `main.lua`。

```json
{
  "name": "codeview_ts",
  "debug": true,
  ...
}
```

源码变化时输出日志：
```
[LuaSkill] Hot reload codeview_ts: D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ts\main.lua
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
  "tool_name": "my_skill",
  "description": "Tool description for tools/list",
  "lua_entry": "main.lua",
  "lua_module": "my_skill",
  "init_scripts": {
    "ps1": "init.ps1",
    "sh": "init.sh"
  },
  "debug": false,
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
}
```
