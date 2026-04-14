# 任务计划：为 Lua `vulcan` 模块增加 `exec` 运行时桥接能力

## 任务目标

根据当前需求，为 Lua skill 提供更完整的宿主进程执行能力，新增 `vulcan.exec`，并保留 `io` 不做封禁。与此同时，补充 `vulcan.cwd`，避免 skill 为获取当前工作目录再通过 `io.popen("pwd"/"cd")` 启动 shell。

本次目标不是先做严格收敛，而是先把可用能力铺齐，方便后续在真实 skill 演进过程中识别哪些执行形态必须支持、哪些能力最终需要收口。

目标包括：

1. 在 Rust 侧 `vulcan` 模块中新增 `vulcan.exec`；
2. 支持两类调用形态：
   - 字符串命令：走 shell 执行，满足“先提供一切能力”的需求；
   - 结构化表参数：支持 `program / args / cwd / env / stdin / timeout_ms / shell` 等字段；
3. 返回结构化结果，包括成功状态、退出码、`stdout`、`stderr`、超时标记等；
4. 新增 `vulcan.cwd`，直接返回当前工作目录；
5. 将 `codeview_ast` 中现有 `io.popen` 调用改为优先使用 `vulcan.exec` / `vulcan.cwd`，作为首个真实落地场景；
6. 保留 `io` 可用，不做兼容性破坏。

## 执行步骤

1. 审查 `src/lua_engine.rs` 当前 `vulcan` 内置函数注册方式，设计 `exec` 的 Lua 入参到 Rust 执行配置的转换方案。
2. 实现结构化执行配置解析，覆盖：
   - 字符串命令；
   - 表参数模式；
   - 参数数组；
   - 环境变量表；
   - 工作目录；
   - 标准输入；
   - 超时控制。
3. 在 Rust 侧用 `std::process::Command` 构建统一执行逻辑，并输出结构化结果表。
4. 新增 `vulcan.cwd`。
5. 修改 `runtime/lua_skills/codeview_ast/main.lua`：
   - 获取当前目录改走 `vulcan.cwd()`；
   - 扫描命令执行改走 `vulcan.exec(...)`；
   - 保留错误诊断兼容性。
6. 如有必要，同步更新 `runlua` 工具说明，让调试时能直接发现 `vulcan.exec` 新能力。
7. 执行构建与真实 MCP 回归，至少验证：
   - `vulcan.cwd()` 返回当前目录；
   - `vulcan.exec("echo ...")` 能执行；
   - `vulcan.exec({ program = ..., args = {...} })` 能执行；
   - `codeview_ast` 使用新桥接后仍能正常扫描；
   - 超时与参数错误能返回明确结果。
8. 对照计划逐项自检，补充执行变更总结，并在完成后迁移到 `docs/completed/20260413/`。

## 技术选型

- 执行底座采用 Rust 标准库 `std::process::Command`，避免 Lua 侧持续依赖 shell 字符串拼接。
- 同时保留 shell 字符串模式，作为“全能力开放阶段”的兼容入口。
- 超时控制通过 Rust 线程 + 通道等待结果的方式实现，保证 Lua skill 能拿到确定性的超时反馈。
- `codeview_ast` 先作为第一批迁移对象，验证 `vulcan.exec` 是否足以覆盖当前真实需求。

## 验收标准

- Lua 中可调用 `vulcan.exec(...)`，并获得结构化执行结果。
- 同时支持 shell 字符串模式与结构化表模式。
- `vulcan.cwd()` 可稳定返回当前工作目录。
- `codeview_ast` 不再依赖 `io.popen` 完成目录获取和 ast-grep 执行。
- 构建通过，真实 MCP 回归通过。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 在 Rust 侧 `vulcan` 模块新增 `vulcan.exec` 与 `vulcan.cwd`，为 Lua skill 提供统一的宿主进程执行桥接。
- `vulcan.exec` 同时支持：
  - 直接传字符串命令，走 shell 模式；
  - 传结构化 table，使用 `program / args / cwd / env / stdin / timeout_ms` 进行受控执行。
- 返回值统一为结构化结果表，包含 `ok`、`success`、`code`、`stdout`、`stderr`、`timed_out`、`error`。
- `codeview_ast` 已完成首批迁移：获取当前目录优先使用 `vulcan.cwd()`，执行 `ast-grep` 优先使用 `vulcan.exec(...)`，同时保留 `io.popen` 作为兼容回退路径，符合“先开放能力、不关闭 io”的当前策略。

### 2. 📂文件变更清单

- 修改：`src/lua_engine.rs`
- 修改：`src/server.rs`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`docs/plan/20260413-09-LUA_VULCAN_EXEC_RUNTIME_BRIDGE.md`

### 3. 💻关键代码调整详情

- `src/lua_engine.rs`
  - 新增 `ExecMode`、`ExecRequest`、`ExecResult` 以及配套解析与执行函数；
  - 新增对 `exec` table 参数的字段解析，支持：
    - `command`
    - `program`
    - `args`
    - `cwd`
    - `env`
    - `stdin`
    - `timeout_ms`
    - `shell`
  - 超时控制采用轮询 `try_wait + kill` 的方式实现，可在超时后主动终止子进程；
  - `vulcan.cwd()` 直接返回宿主当前工作目录；
  - `vulcan.exec()` 统一返回结构化结果，不再要求 Lua skill 自己拼 shell 并读管道。
- `runtime/lua_skills/codeview_ast/main.lua`
  - `get_current_working_directory()` 优先改为 `pcall(vulcan.cwd)`；
  - `run_scan_batch()` 优先改为 `pcall(vulcan.exec, { ... })`；
  - `ast-grep` 结构化执行模式下改用绝对程序路径，避免 `cwd` 变化导致 `Command::new` 找不到二进制；
  - 保留 `io.popen` 作为 fallback，避免对旧运行时产生硬破坏。
- `src/server.rs`
  - `runlua` 工具说明已补充 `vulcan.cwd()`、`vulcan.exec(spec)`、`vulcan.fs_is_dir(path)` 等新能力，方便调试与发现。

### 4. ⚠️遗留问题与注意事项

- 当前阶段按需求“先提供一切能力”，因此 `vulcan.exec` 仍支持 shell 字符串模式；后续如果要收敛权限边界，可以再基于真实 skill 使用情况决定是否限制到结构化模式。
- `codeview_ast` 虽然已优先改走 `vulcan.exec`，但仍保留 `io.popen` fallback；这不是遗漏，而是当前阶段的兼容策略。
- 真实 MCP 回归已验证以下结果：
  - `vulcan.cwd()` 返回 `D:\projects\vulcan-mcp-client`
  - `vulcan.exec("echo shell_exec_ok")` 返回 `code = 0`、`stdout = shell_exec_ok`
  - `vulcan.exec({ program = "cmd.exe", args = { "/C", "echo", "program_exec_ok" } })` 返回 `code = 0`
  - 超时样例返回 `timed_out = true`
  - `codeview_ast` 扫描 `src` 时恢复正常，结果为 `files_scanned = 10`、`files_with_symbols = 10`、`items_found = 268`
