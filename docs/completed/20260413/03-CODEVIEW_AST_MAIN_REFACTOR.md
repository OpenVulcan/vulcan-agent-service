# 任务计划：重构 codeview_ast 的规则加载与结构提取主逻辑

## 任务目标

重构 `runtime/lua_skills/codeview_ast/main.lua` 以及配套规则组织方式，解决当前 `codeview_ast` 存在的两类核心问题：

1. `ast-grep` 规则文件过于分散，导致扫描单个文件时仍需反复装载大量无关规则，运行时开销高且维护困难。
2. Lua 主逻辑当前输出结构不稳定，无法以“文件”为节点，清晰返回文件内的类、接口、函数/方法、参数、备注信息与行号范围，无法满足 AI 先看结构后精读源码的使用目标。

本次改造完成后，`codeview_ast` 应能稳定返回“按文件聚合”的代码结构清单，并具备统一、可扩展的规则加载策略。

## 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/` 下现有 `main.lua`、`sgconfig.yml` 与 `rules/*.yml` 的组织方式，确认当前规则命名、变量提取和扫描调用链存在的问题。
2. 设计新的规则组织方案，优先将规则从“海量离散文件”收敛为“按语言聚合或按运行时最小集合装载”的结构，减少单次扫描的规则装载成本。
3. 重构 `main.lua`：
   - 重建文件遍历、语言识别、规则选择、命令执行、结果解析、结构聚合的完整流程；
   - 输出以“文件”为一级节点的数据结构；
   - 为每个结构节点补齐名称、签名、参数、备注、起止行号、原始文本片段等信息；
   - 处理类/接口与函数/方法的归属关系，保证结果可用于 AI 精准定位代码范围。
4. 补齐必要的规则字段约定，使 ast-grep 输出可被 Lua 侧稳定解析，而不是依赖当前不可靠的兜底字符串截取。
5. 结合仓库内样例文件或当前仓库代码对 skill 进行实测，确认输出结构满足预期。
6. 对照本计划逐项自检，补充执行变更总结，并在确认完成后将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 保持现有 Lua skill + ast-grep 技术路线，不额外引入新的 AST 引擎。
- 优先采用“按语言聚合规则 + 按目标文件语言定向装载”的方案，避免每次扫描装载整个规则目录。
- 在 Lua 侧建立统一的结果归一化层，将不同语言规则输出转换为统一结构，提升扩展性与可维护性。
- 在不破坏现有 skill 注册机制的前提下，允许对 `main.lua` 做较大幅度重构，但尽量不扩大到 Rust 引擎侧改造。

## 验收标准

- `codeview_ast` 返回结果必须以文件为一级节点，能够明确展示每个文件下的结构成员及其行号范围。
- 返回结果中至少包含：文件路径、结构类型、名称、签名、参数列表、备注信息、起始行、结束行。
- `main.lua` 不再对每个文件都通过 `sgconfig.yml` 全量装载整个 `rules/` 目录，而是采用更聚焦、更可控的规则加载方式。
- 规则文件组织与 Lua 解析逻辑一致，不能再依赖脆弱的随机变量命中或模糊字符串截取作为主路径。
- 代码修改符合仓库规范，新增或重构的代码包含必要的中英文双语注释。
- 计划文件最终包含完整的执行变更总结，并迁移到 `docs/completed/20260413/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 完整重构 `codeview_ast/main.lua`，将执行链从“逐文件 + `sgconfig.yml` 全量规则目录扫描”改为“按语言分组 + `scan --rule <language-bundle> --json=stream --include-metadata`”的新模式，修复了原实现错误使用 `scan -l`、错误读取 `item.meta.ruleId / item.meta.variables` 的根本问题。
- 将规则目录从原来的“按结构类型零散拆分”重组为“按语言聚合”的 bundle 方案，每个语言规则文件统一使用 ast-grep 官方支持的 YAML 规则格式，并通过 `metadata.symbol_kind / metadata.container / name_capture / params_capture` 向 Lua 侧提供稳定协议。
- 重新实现 Lua 侧结果归一化逻辑，支持输出“按文件聚合”的结构树，包含文件路径、结构类型、名称、签名、参数、备注、起止行号以及父子层级关系，类/结构体/协议等容器节点下的函数会自动归并为方法。
- 修复初始化脚本对 `SKILL_DIR` 的强依赖问题，使 `init.ps1` 与 `init.sh` 在离开宿主环境单独调试时也能正常回退到脚本目录。
- 在 `output/lua_skills/codeview_ast` 上完成规则烟雾测试、Lua harness 结构验证，以及对仓库真实 `src` 目录的扫描验证，确认当前实现可以稳定返回文件级结构结果。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`runtime/lua_skills/codeview_ast/init.ps1`
- 修改：`runtime/lua_skills/codeview_ast/init.sh`
- 修改：`runtime/lua_skills/codeview_ast/rules/`（删除旧散装规则，新增按语言聚合规则 bundle）
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/init.ps1`
- 修改：`output/lua_skills/codeview_ast/init.sh`
- 修改：`output/lua_skills/codeview_ast/rules/`（同步新的语言化规则 bundle，并清理旧规则）
- 修改：`docs/plan/20260413-03-CODEVIEW_AST_MAIN_REFACTOR.md`

### 3. 💻关键代码调整详情

- 在 `main.lua` 中新增并重构以下核心能力：
  - 语言注册表：统一管理扩展名、规则文件、注释风格与语言别名。
  - ast-grep 执行层：统一按语言分组批量扫描，并用 `--json=stream` 逐行解析 JSON 结果。
  - capture 归一化层：优先读取规则 `metadata` 与 `metaVariables`，不足时再回退到声明头文本推断名称与参数。
  - 结构建树层：依据文件内行号范围建立父子关系，并在类/结构体/协议/impl 等容器内将函数归类为方法。
  - 注释提取层：支持行注释、块注释与 Python docstring 的近邻备注提取。
- 新规则协议统一采用 ast-grep 官方支持的能力组合：
  - `scan --rule <rule-file>`：按语言定向加载规则，不再依赖全目录配置扫描。
  - `metadata`：显式声明结构类型与容器属性，避免 Lua 通过 `ruleId` 猜测类型。
  - `pattern + selector`：用于 TypeScript/JavaScript/Kotlin/Swift/Go 等需要精确捕获名称或参数的节点。
  - `kind`：用于稳定且简单的结构节点匹配，尽量减少脆弱规则。
- 新规则 bundle 已覆盖并通过烟雾测试的语言包括：
  - Bash
  - C
  - Cpp
  - CSharp
  - Elixir
  - Go
  - Haskell
  - Java
  - JavaScript
  - Kotlin
  - Lua
  - PHP
  - Python
  - Ruby
  - Rust
  - Scala
  - Swift
  - TypeScript
  - Tsx
- 验证结果：
  - 19 个语言规则 bundle 在 `output/lua_skills/codeview_ast/bin/ast-grep.exe` 下的烟雾测试全部返回 `ExitCode 0`。
  - 混合语言样例目录通过 Lua harness 返回了按文件聚合、带类-方法层级、带注释与行号的结构树。
  - 对仓库真实 `src` 目录的验证结果为：`files_scanned = 10`、`files_with_symbols = 10`、`items_found = 245`。

### 4. ⚠️遗留问题与注意事项

- 当前 `output/` 目录仍然是测试运行目录，后续如构建流程继续从 `runtime/` 同步到 `output/`，需要保持这套“源目录改造 + output 验证”路径不变，避免直接在 output 下手改后被覆盖。
- 备注提取目前采用“近邻注释 + Python docstring”的通用启发式策略，已经能覆盖常见注释场景，但对少数语言的复杂块注释或特殊文档语法仍可能存在漏提取空间。
- 本次规则集聚焦 ast-grep 官方当前支持且本轮已验证可用的主流语言；原来那些未被官方当前能力稳定支持、或旧规则本身不可靠的语言文件已经移除，不建议再恢复旧散装规则方案。
