# 任务目标

为 `ast-grep` skill 新增 `vmcp-patch` 工具，使其能够基于结构路径 selector 在代码发生行号漂移后仍重新定位目标函数，并完成整函数替换或仅函数体替换，弥补当前只能依赖行号范围进行代码替换的问题。

# 执行步骤

1. 梳理现有 `vmcp-ast` / `vmcp-rg` 的 AST 归一化与结构树构建能力，确认哪些 helper 可直接复用。
2. 设计 `vmcp-patch` 参数协议，至少支持：
   - `file`
   - `selector`
   - `replacement`
   - `mode`（`auto` / `full` / `body`）
3. 实现函数级节点重定位能力：
   - 仅允许 patch function / method 这类可完整替换的结构节点
   - 支持宽松 selector，如 `with_vmm`、`McpServer/with_vmm`、`impl McpServer/with_vmm`
   - 唯一命中则替换，多个命中则返回候选树结构
4. 实现替换策略：
   - `full`：替换完整函数
   - `body`：保留外层声明，仅替换函数体
   - `auto`：自动判断 replacement 是完整函数还是函数体
5. 将工具注册到 `skill.json`，同步 `runtime` / `output` 文件。
6. 使用仓库内临时测试文件构造较复杂结构，验证：
   - 唯一命中替换成功
   - 歧义 selector 返回多个候选
   - `full` 与 `body` 两种替换都可工作
7. 完成验证后补充执行变更总结，并将计划文件归档到 `docs/completed/20260414/`。

# 技术选型

- 复用 `vmcp-ast` 的 AST helper：文件收集、ast-grep 扫描、symbol 归一化与结构树构建。
- `vmcp-patch` 仅支持函数级节点替换，不允许直接 patch 类名、结构名、字段名等局部符号。
- selector 采用“路径后缀匹配 + 节点别名匹配”策略：
  - 例如 `with_vmm`
  - `McpServer/with_vmm`
  - `impl McpServer/with_vmm`
- 当匹配结果超过 1 个时，返回更完整的树路径候选，要求调用方重试。
- `body` 替换优先支持 Rust/Go/JS/TS/Java/C#/Kotlin/Swift/PHP/Solidity 等花括号函数体；必要时为 Lua/Python 提供基础支持。

# 验收标准

1. `ast-grep` skill 中新增 `vmcp-patch` 工具。
2. `vmcp-patch` 支持宽松 selector，并能在行号变化后重新定位函数。
3. 仅函数节点允许被替换，类型/结构节点不可直接 patch。
4. `replacement` 同时支持完整函数与仅函数体两种输入。
5. 歧义 selector 会返回候选树信息，而不是错误修改代码。
6. 使用临时测试文件完成实际验证，覆盖唯一命中、歧义命中、完整函数替换、函数体替换四类场景。

## 执行变更总结

### 1. 核心修复与调整概述

- 为 `ast-grep` skill 新增了 `vmcp-patch` 工具，支持在不依赖旧行号的前提下，通过宽松结构 selector 重新定位函数并完成代码替换。
- `vmcp-patch` 仅允许 patch function / method 节点，不允许直接 patch 类型名、字段名等局部标识符。
- 工具支持两类 replacement：
  - 完整函数替换（`mode=full`）
  - 仅函数体替换（`mode=body`）
  - `mode=auto` 会自动判断 replacement 更像完整函数还是函数体
- 当 selector 命中多个函数时，工具不会误改，而是返回候选树路径，要求调用方补充更具体的 selector。

### 2. 📂文件变更清单

新增：

- `D:\\projects\\vulcan-mcp-client\\runtime\\lua_skills\\ast-grep\\main_patch.lua`
- `D:\\projects\\vulcan-mcp-client\\docs\\plan\\20260414-14-VMCP_PATCH_FUNCTION_REPLACEMENT.md`

修改：

- `D:\\projects\\vulcan-mcp-client\\runtime\\lua_skills\\ast-grep\\skill.json`
- `D:\\projects\\vulcan-mcp-client\\output\\lua_skills\\ast-grep\\main_patch.lua`
- `D:\\projects\\vulcan-mcp-client\\output\\lua_skills\\ast-grep\\skill.json`
- `D:\\projects\\vulcan-mcp-client\\docs\\lua_skills.md`

删除：

- `D:\\projects\\vulcan-mcp-client\\tmp\\vmcp_patch_test.rs`（测试后已清理）

### 3. 💻关键代码调整详情

- `main_patch.lua`
  - 复用 `vmcp-ast` 内部 helper，重新扫描目标文件 AST 并构造函数级结构树；
  - 实现“路径后缀 + 节点别名”匹配策略，支持：
    - `with_vmm`
    - `McpServer/with_vmm`
    - `impl McpServer/with_vmm`
  - 命中多个函数时返回 `ambiguous_selector` 与候选结构路径；
  - 支持 `auto/full/body` 三种替换模式；
  - `body` 模式当前优先支持花括号函数体语言（已覆盖 Rust 场景）。
- `skill.json`
  - 新注册 `vmcp-patch` tool；
  - 增加 `file`、`selector`、`replacement`、`mode` 参数说明；
  - 补充工具用途与使用协议说明。
- `docs/lua_skills.md`
  - 增补 `vmcp-patch` 的 selector 与替换规则说明，确保技能说明与实际行为一致。

### 4. ⚠️遗留问题与注意事项

- `body` 替换当前主要面向花括号函数语言；对于非花括号语言，如果只提供函数体，当前会返回不支持错误。
- `full` 替换允许调用方提供完整函数源码，因此如果 replacement 自身逻辑或语法有问题，工具不会自动做语言级纠错。
- 当前候选路径返回的是“规范结构路径”，足够指导 AI 二次重试，但还没有单独的 `node_id` 机制。
- 本次验证使用了仓库内临时 Rust 文件，已在测试完成后删除，未污染业务代码。
