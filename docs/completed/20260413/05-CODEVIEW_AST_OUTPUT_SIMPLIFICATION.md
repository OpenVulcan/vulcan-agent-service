# 任务计划：简化 codeview_ast 的结构输出格式

## 任务目标

调整 `runtime/lua_skills/codeview_ast/main.lua` 的最终返回结构，解决当前 `codeview_ast` 输出过于全面、层级过深、阅读成本过高的问题。

本次改造的目标不是削弱 AST 提取能力，而是将现有“细粒度节点明细”收敛为更适合 AI 直接阅读的轻量结构视图。最终结果应按文件分组，每组只保留：

1. `file`：文件标识；
2. `language`：语言标识；
3. `content`：直接可读的纯文本结构摘要。

其中 `content` 应采用最简单的列表形式表达，例如：

```text
impl LuaEngine L1-10
- pub fn new() -> Result<Self, Box<dyn std::error::Error>> L12-18
```

确保 AI 不需要再解析复杂 JSON 节点，即可快速理解文件结构。

## 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 当前的返回结构与构树逻辑，识别哪些字段仅服务于详细 JSON 输出、哪些字段仍然对生成轻量文本有价值。
2. 设计新的对外结果模型，统一改为“文件级三元组（file、language、content）”形式，并明确文本缩进、行号格式、父子结构展示规则。
3. 在不破坏现有 ast-grep 扫描、规则加载、符号归一化逻辑的前提下，新增轻量格式化层，将内部结构树转换为文本内容。
4. 视需要同步更新 `skill.json` 的工具描述，确保技能说明与实际输出一致。
5. 运行构建同步脚本，将 `runtime` 变更同步到 `output`，并结合样例目录或仓库真实代码进行验证，确认输出确实简洁、稳定、可读。
6. 对照本计划逐项自检，补充执行变更总结，并在确认完成后将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 保留现有 Lua + ast-grep 扫描与规则协议，不重新设计底层提取链路。
- 以“内部保留结构化符号、外部输出轻量文本内容”的方式实现收敛，兼顾稳定性与可读性。
- 输出格式优先服务 AI 阅读体验，避免为了保留冗余字段而牺牲简洁性。

## 验收标准

- 返回结果按文件分组，每组至少包含 `file`、`language`、`content` 三个核心字段。
- `content` 为纯文本结构摘要，能够直接展示容器、函数/方法与行号范围，不需要依赖 JSON 深层字段解析。
- 输出中不再暴露当前那种过细的 `symbols` 明细树作为主结果。
- 新的输出格式在真实代码目录上可运行，并能稳定返回可读结果。
- 代码修改符合仓库规范，新增或修改的关键函数包含必要的中英文双语注释。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `codeview_ast` 的对外结果从“文件 + 统计 + symbols 明细树”收敛为“`file` + `language` + `content`”的轻量结构视图，显著降低 AI 阅读成本。
- 在 `main.lua` 中新增结构文本渲染层，将内部符号树直接格式化为人类可读的纯文本摘要，输出形态改为类似 `impl Foo L1-10`、`- pub fn bar() L12-18` 的列表式结构。
- 同步调整 `skill.json` 描述，使技能说明与新输出格式保持一致，明确该 skill 现在返回轻量结构摘要而非详细节点树。
- 在验证过程中补修了一个影响真实可用性的相对路径问题：`ast-grep` 在 skill `bin` 目录执行时，原先会导致相对扫描路径失效；现已改为“扫描文件绝对路径 + 在 bin 目录调用 exe 名称”的稳定方案。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`
- 修改：`docs/plan/20260413-05-CODEVIEW_AST_OUTPUT_SIMPLIFICATION.md`

### 3. 💻关键代码调整详情

- `main.lua` 新增了面向最终输出的轻量格式化函数：
  - 行号范围格式化；
  - 单节点结构摘要格式化；
  - 结构树递归展开为纯文本 outline；
  - 单文件 `content` 文本生成。
- 文件收集逻辑新增扫描路径规范化能力：
  - 保留用于展示的 `display_file`；
  - 实际传给 `ast-grep` 与文件读取的路径统一解析为稳定的绝对路径；
  - 避免相对路径在切换工作目录后失效。
- ast-grep 命令执行逻辑调整为：
  - 在 skill `bin` 目录下执行 `ast-grep.exe` 名称，规避 Windows `io.popen` 直接以带引号的绝对可执行路径开头时的命令解析问题；
  - 配合绝对扫描路径，保证规则文件与源文件都能被正确定位。
- 最终返回结果中，文件节点现在只保留：
  - `file`
  - `language`
  - `content`
  不再把 `summary`、`symbol_count`、`line_count`、`symbols` 明细树作为主输出暴露给调用方。

### 4. ⚠️遗留问题与注意事项

- 当前 `content` 已经足够适合 AI 直读，但对于特别长的函数签名，单行文本仍可能较长；这是为了尽量保留原始声明信息而做出的折中。
- 顶层返回仍保留了 `files_scanned`、`files_with_symbols`、`items_found`、`errors` 等汇总信息，便于调用方快速判断扫描结果是否完整；如果后续希望进一步极简，还可以继续裁剪这些汇总字段。
- 本次已通过真实 MCP 调用验证新格式，测试条件为：临时端口启动 `output/debug/vulcan-mcp.exe`，调用 `codeview_ast` 扫描仓库 `src` 目录，结果为 `files_scanned = 10`、`files_with_symbols = 10`、`items_found = 245`，并已返回新的 `file / language / content` 结构。
