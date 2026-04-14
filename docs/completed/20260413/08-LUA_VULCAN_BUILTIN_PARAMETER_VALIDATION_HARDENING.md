# 任务计划：加固 Lua `vulcan` 内置函数的参数校验机制

## 任务目标

修复并加固 Lua `vulcan` 内置函数在参数过滤方面的薄弱点，重点解决当前 `vulcan.fs_list` 等文件系统函数对错误参数过滤不严谨、最终把底层操作系统错误直接暴露给 Lua skill 的问题。

当前已观测到的直接问题是：

```text
Lua skill 'codeview_ast' error: runtime error: fs_list: 目录名称无效。 (os error 267)
```

这说明：

1. 进入 `vulcan.fs_list` 的参数在值级别没有被提前拦截；
2. 一些非预期参数会一路流到 `std::fs::read_dir` 才报底层错误，导致错误语义不清晰；
3. 其他内置函数如 `fs_read`、`fs_write`、`fs_exists`、`fs_is_dir`、`path_join`、`json_decode` 等也存在类似“类型转换后直接执行、缺少统一值校验”的隐患。

本次目标如下：

1. 为 `vulcan` 内置函数建立统一的参数校验辅助逻辑；
2. 对路径类参数、字符串类参数、可变参数列表做更严格的值级过滤；
3. 输出更明确、更靠近调用方语义的错误信息，而不是让 OS 错误直接充当主要报错；
4. 顺手修补 `codeview_ast` 入口参数的关键类型校验，避免典型脏值先在 Lua 层被 `tostring` 放大。

## 执行步骤

1. 审查 `src/lua_engine.rs` 中 `vulcan` 模块的所有内置函数注册逻辑，梳理当前参数获取方式、隐式类型转换方式和错误路径。
2. 设计统一的参数校验辅助函数，覆盖：
   - 必填字符串参数；
   - 路径字符串参数；
   - 变长字符串参数列表；
   - JSON 解码输入字符串；
   - `vulcan.call` 这类复合参数。
3. 将 `fs_list`、`fs_read`、`fs_write`、`fs_exists`、`fs_is_dir`、`path_join`、`json_decode`、`call` 等内置函数切换到更严格的参数校验路径。
4. 在 `runtime/lua_skills/codeview_ast/main.lua` 中补充 `dir`、`lang` 等关键参数的基本类型校验，避免非字符串先被 `tostring` 污染。
5. 运行构建同步，并通过实际 Lua 调用验证以下场景：
   - 非字符串路径参数能被明确拒绝；
   - 明显无效的路径文本能被提前拦截；
   - `path_join` 不再静默吞掉非字符串参数；
   - 正常路径参数不受影响。
6. 对照本计划逐项自检，补充执行变更总结，并在确认完成后将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 优先在 Rust 侧 `vulcan` 内置函数入口做统一校验，因为这是所有 Lua skill 共用的系统边界。
- 对路径参数采用“类型严格 + 值级过滤 + 更清晰错误信息”的策略，不依赖底层文件系统错误做主校验。
- 对 `codeview_ast` 额外补入口校验，作为高频调用场景下的直接防线。

## 验收标准

- `vulcan.fs_list` 等内置函数对错误参数能够给出明确、稳定的参数错误，而不是直接透传 OS 错误。
- `path_join` 不再静默忽略非字符串参数。
- `json_decode`、`call` 等其他内置函数的参数过滤机制也得到同步加固。
- `codeview_ast` 对明显错误的 `dir/lang` 参数能提前返回合理错误。
- 完成构建同步与实际验证，并保留必要的中英文双语注释。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 在 Rust 侧 `src/lua_engine.rs` 中新增统一的 Lua 参数校验辅助函数，集中处理字符串、路径、table 参数的类型与值级校验。
- 收紧 `vulcan.fs_list`、`fs_read`、`fs_write`、`fs_exists`、`fs_is_dir`、`path_join`、`json_decode`、`call`、`log` 的参数入口，移除原先依赖隐式转换或底层系统报错的宽松行为。
- 在 Lua skill `runtime/lua_skills/codeview_ast/main.lua` 中补充 `dir`、`lang`、`recursive` 的入口类型校验，确保非法参数在 skill 层就返回结构化错误对象。
- 通过真实 MCP 调用完成回归验证，确认 `fs_list` 不再透传 Windows `os error 267`，而是明确提示参数类型错误。

### 2. 📂文件变更清单

- 修改：`src/lua_engine.rs`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`docs/plan/20260413-08-LUA_VULCAN_BUILTIN_PARAMETER_VALIDATION_HARDENING.md`

### 3. 💻关键代码调整详情

- `src/lua_engine.rs`
  - 新增 `lua_value_type_name`、`require_string_arg`、`validate_path_text`、`require_path_arg`、`require_table_arg` 等统一校验函数。
  - 为 Windows 路径增加保守语法校验，拒绝明显非法的路径文本与 `table: 0x...` 这类 `tostring` 污染值。
  - `path_join` 改为逐段严格校验后使用 `PathBuf::push` 拼接，不再静默吞掉非法参数。
  - `call` 改为强制要求 `name` 为非空字符串、`args` 为 table。
- `runtime/lua_skills/codeview_ast/main.lua`
  - 新增 `validate_directory_argument`、`validate_language_argument`、`validate_recursive_argument`。
  - Skill 入口改为“先校验、后执行”，当 `dir/lang/recursive` 非法时直接返回结构化错误对象，不再继续下探到 `collect_files` / `vulcan.fs_list`。
- 实测结果
  - `runlua: return vulcan.fs_list({})` 返回明确错误：`fs_list: dir must be a string, got table`。
  - `runlua: return vulcan.path_join("a", {})` 返回明确错误：`path_join: part[2] must be a string, got table`。
  - `runlua: return vulcan.json_decode("")` 返回明确错误：`json_decode: text must not be empty`。
  - `runlua: return vulcan.call("codeview_ast", "bad")` 返回明确错误：`call: args must be a table, got string`。
  - `codeview_ast` 传入非法 `dir` 时返回：
    - `error = "invalid_dir_argument"`
    - `message = "dir must be a non-empty string"`
  - `codeview_ast` 以 `dir = "src"` 执行正常，返回 `files_scanned = 10`、`files_with_symbols = 10`、`items_found = 252`。

### 4. ⚠️遗留问题与注意事项

- 当前 `runlua` 工具在 Lua 运行时报错时，文本内容已经正确返回，但响应中的 `isError` 标记未在本轮内额外调整；这属于工具结果封装层的既有行为，不影响本次参数校验修复本身。
- 路径校验采用“保守拒绝”策略，优先保证稳定性与错误可读性；如果后续要支持非常规 Windows 路径写法，需要在已有校验函数上谨慎扩展。
