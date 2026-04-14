# 任务计划：补强 codeview_ast 对 TypeScript 导出常量结构的提取能力

## 任务目标

分析并修复 `codeview_ast` 在扫描 `D:\projects\VmmOpenCodePlugins\src\vmm-language` 这类 TypeScript 语言目录时，无法稳定导出 `export const ...` 结构的问题。

当前现象是：目录中只有少量可识别结构被导出，像 `export const VMM_LANGUAGE_CATALOG_DE = ...` 这种对 AI 很关键的顶层导出对象，未能输出行号范围与结构信息。这会直接削弱该 skill 对“先看结构、再决定是否读源码”的价值。

本次改造目标如下：

1. 至少能够稳定识别并导出 `export const VMM_LANGUAGE_CATALOG_DE` 这类顶层导出常量的行号范围；
2. 尽量兼容同类 `export const` / `const ... = ...` 的结构性定义，尤其是对象字面量、数组字面量、调用返回值这类常见模块导出形式；
3. 保持现有轻量输出风格，即仍以 `file`、`language`、`content` 形式返回，避免重新回退到过细的 JSON 节点树。

## 执行步骤

1. 审查 `D:\projects\VmmOpenCodePlugins\src\vmm-language` 中各语言文件的源码形态，确认当前哪些顶层定义未被识别。
2. 复核 `runtime/lua_skills/codeview_ast/rules/typescript.yml`、`javascript.yml` 与 `main.lua` 的归一化逻辑，定位遗漏原因是规则覆盖不足还是名称/头部提取失败。
3. 设计并实现对顶层导出常量、变量声明的提取增强，优先通过 ast-grep 规则补齐，不轻易引入脆弱字符串硬解析。
4. 如有必要，补充 Lua 侧名称、签名、行号摘要的归一化逻辑，保证新增结构能被正确渲染到 `content` 文本中。
5. 同步构建到 `output/`，并通过真实 `codeview_ast` 调用验证：
   - `D:\projects\VmmOpenCodePlugins\src\vmm-language\de.ts` 中应能看到 `export const VMM_LANGUAGE_CATALOG_DE ...` 的行号范围；
   - 同目录其他语言文件中的同类结构应尽量被稳定识别。
6. 对照本计划逐项自检，补充执行变更总结，并在确认完成后将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 保持 Lua + ast-grep 规则驱动路线，不改动整体 skill 架构。
- 优先通过 TypeScript / JavaScript 规则增强来补齐导出常量提取。
- 若规则层无法稳定给出名称或签名，再在 Lua 侧做最小必要的归一化兜底。

## 验收标准

- `codeview_ast` 能够导出 `export const VMM_LANGUAGE_CATALOG_DE` 的结构摘要与准确行号范围。
- `vmm-language` 目录下的语言文件不再只剩 `shared.ts` 有明显可见结构，其余 `*.ts` 文件中的顶层导出对象也应能被识别。
- 最终输出继续保持 `file`、`language`、`content` 的轻量格式。
- 代码修改包含必要的中英文双语注释，并完成构建同步与实际验证。

## 执行变更总结

### 1. 核心修复与调整概述

- 识别到 `codeview_ast` 当前对 TypeScript / JavaScript 的规则偏向“行为结构”，只覆盖类、接口、函数、方法，遗漏了语言目录、配置目录这类以 `export const` 对象为核心的“数据结构模块”。
- 已为 `TypeScript`、`JavaScript`、`TSX` 补充“顶层导出对象常量”规则，使 `export const VMM_LANGUAGE_CATALOG_DE = { ... }` 这类结构能够被识别并导出行号范围。
- 已在 Lua 归一化逻辑中把 `constant` 结构头部从 `export const NAME =` 收敛为更干净的展示文本，最终输出直接显示为 `export const NAME Lx-y`，更适合 AI 直读。
- 本次刻意先只补“`export const` + 对象字面量”这一类目标结构，没有把所有普通 `const` 都纳入，是为了避免与现有箭头函数/函数表达式规则重叠，导致重复导出。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/rules/typescript.yml`
- 修改：`runtime/lua_skills/codeview_ast/rules/javascript.yml`
- 修改：`runtime/lua_skills/codeview_ast/rules/tsx.yml`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/rules/typescript.yml`
- 修改：`output/lua_skills/codeview_ast/rules/javascript.yml`
- 修改：`output/lua_skills/codeview_ast/rules/tsx.yml`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`docs/plan/20260413-06-CODEVIEW_AST_TYPESCRIPT_EXPORT_CONSTANT_SUPPORT.md`

### 3. 💻关键代码调整详情

- 在 `typescript.yml` 中新增了两类规则：
  - `export const $NAME = { ... }`
  - `export const $NAME = { ... } satisfies $TYPE`
- 在 `javascript.yml` 中新增了 `export const $NAME = { ... }` 顶层对象导出规则。
- 在 `tsx.yml` 中同步补齐与 TypeScript 对应的顶层对象导出规则，避免 JSX/TSX 目录类模块继续漏识别。
- 在 `main.lua` 中扩展了 `constant` 结构的名称推断规则，并在 `normalize_symbol` 中将 `export const NAME =` 的尾部等号清理掉，使最终 `content` 更贴近人类阅读习惯。
- 实际验证结果：
  - 通过真实 MCP 调用扫描 `D:\projects\VmmOpenCodePlugins\src\vmm-language`，已成功输出：
    - `export const VMM_LANGUAGE_CATALOG_DE L17-79`
    - `export const VMM_LANGUAGE_CATALOG_EN L17-80`
    - `export const VMM_LANGUAGE_CATALOGS L25-33`
    - 以及其他各语言目录常量项
  - 验证统计为：`files_scanned = 9`、`files_with_symbols = 9`、`items_found = 12`

### 4. ⚠️遗留问题与注意事项

- 当前补强范围聚焦于“顶层导出对象常量”。如果后续还有大量 `export const X = defineConfig(...)`、`export default {...}`、`export const X = [...] as const` 这类形态，建议再单独扩展规则，而不是一次性放宽成“所有 const 都抓”，否则很容易把函数变量、普通值变量也混进结构视图。
- 目前 `shared.ts` 仍然主要输出类型定义，这是符合预期的；因为它本身就是类型层文件，不属于这次新增的“对象型常量模块”范围。
