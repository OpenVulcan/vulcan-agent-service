## 任务目标

为 `ast-grep` 的 Lua skill 新增一个 `vmcp-rg` 工具，支持基于目录、可选扩展名和 `rg` 正则先做文本检索，再结合现有 AST 结构分析，仅输出与命中行直接相关的类/函数等结构信息，弥补 `vmcp-ast` 当前不擅长“由文本命中反推局部结构”的能力。

## 执行步骤

1. 梳理 `runtime/lua_skills/ast-grep` 现有 `vmcp-ast` 实现，确认可复用的扫描、符号归一化、结构树构建与分页逻辑。
2. 为 `vmcp-rg` 设计参数校验与 `rg` 调用流程，支持：
   - 目录参数
   - 可空扩展名参数
   - `rg` 正则参数
3. 实现 `rg` 命中结果与 AST 结构树的映射逻辑：
   - 命中类声明时输出类结构
   - 命中函数声明时输出函数结构
   - 命中函数体内部内容时输出所在函数结构及相关命中内容
4. 更新 `skill.json`，注册 `vmcp-rg` 工具及参数说明。
5. 同步 `runtime` 与 `output` 副本，执行最小验证。
6. 完成验证后补充执行变更总结，并将计划文档归档到 `docs/completed/20260414/`。

## 技术选型

- 优先复用现有 `vmcp-ast` 的 AST 扫描与结构树能力，避免复制整套 ast-grep 解析实现。
- 新增 `vmcp-rg` 时先调用共享 `__tools/bin` 下的 `rg` 可执行文件进行文本检索，再用 AST 结构树对命中行做归属判断。
- 输出以“文件 -> 相关结构 -> 命中文本片段/结构内容”为主，不返回无关结构，尽量控制上下文体积。

## 验收标准

1. `ast-grep` skill 中新增 `vmcp-rg` 工具。
2. `vmcp-rg` 支持 `dir + ext(可空) + rg_pattern` 参数输入。
3. 能先使用 `rg` 检索，再只输出命中行相关的类/函数等 AST 结构。
4. 当命中类名、函数名、函数体内容时，输出结构符合预期且区分清晰。
5. `runtime` 与 `output` 中相关文件保持一致。
6. 计划文档完成变更总结并归档。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 为 `ast-grep` skill 新增了 `vmcp-rg` 工具，支持先用 `rg` 做文本检索，再仅对命中文件执行 AST 分析，并把命中行映射回最相关的结构节点。
- 新工具已支持两类核心场景：
  - 命中声明行时，输出对应结构本身；
  - 命中函数体内部内容时，回退到最近的函数/方法结构，并只附带命中的 `rg` 行内容。
- 为便于本地调试，又补充了主程序 `--call-tools` 调试模式，可直接初始化 Lua skill 并执行目标 tool，而不启动 HTTP/gRPC 服务。

### 2. 📂文件变更清单

新增：
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\ast-grep\main_rg.lua`
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-11-VMCP_RG_AST_CONTEXT_FILTER.md`

修改：
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\ast-grep\skill.json`
- `D:\projects\vulcan-mcp-client\src\main.rs`
- `D:\projects\vulcan-mcp-client\output\lua_skills\ast-grep\main_rg.lua`
- `D:\projects\vulcan-mcp-client\output\lua_skills\ast-grep\skill.json`

删除：
- 无

### 3. 💻关键代码调整详情

1. `runtime/lua_skills/ast-grep/main_rg.lua`
   - 复用现有 `vmcp-ast` 内部 helper（文件收集、ast-grep 扫描、结构归一化、建树）；
   - 新增 `rg --json` 调用与解析逻辑；
   - 新增“命中行 -> 最相关结构”映射逻辑；
   - 新增按文件分页返回与 cache_id 支持。

2. `runtime/lua_skills/ast-grep/skill.json`
   - 注册 `vmcp-rg` tool；
   - 增加 `dir`、`ext`、`rg_pattern` 以及分页相关参数说明；
   - 补充工具用途与使用协议说明。

3. `src/main.rs`
   - 增加 `--call-tools <tool_name> [json_arguments]` 调试模式；
   - 调试模式下直接初始化 LuaEngine 并调用目标 tool，不启动 HTTP/gRPC 服务；
   - 保留默认服务模式不变。

4. 验证结果
   - 执行 `.\make.ps1 build` 成功，`output/debug`、`output/lua_skills` 已同步；
   - 通过 `output/debug/vulcan-mcp.exe --call-tools vmcp-rg ...` 验证了：
     - 声明命中可返回声明结构；
     - 函数体内容命中可回退到函数结构并附带命中行。

### 4. ⚠️遗留问题与注意事项

1. `output/bin/vulcan-mcp.exe` 当前被运行中的进程占用，因此本次未能完成 `release` 二进制覆盖；但 `debug` 调试链与 `output/lua_skills` 已完成同步。
2. `--call-tools` 当前定位为本地调试入口，直接针对 Lua skill tool 使用，适合验证 skill 加载、依赖初始化和实际返回值。
3. `vmcp-rg` 对于没有 AST 结构规则的文本命中，不会伪造结构节点，只会返回空结构结果。
