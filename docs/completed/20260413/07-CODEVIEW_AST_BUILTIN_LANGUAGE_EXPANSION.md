# 任务计划：扩展 codeview_ast 到 ast-grep 官方内建语言全集

## 任务目标

将 `codeview_ast` 的语言支持范围从当前的有限子集扩展到 `ast-grep` 官方当前内建支持的语言全集，并补齐这些语言的最小可用结构提取能力。

当前问题有两层：

1. 我们的 `LANGUAGE_REGISTRY` 只覆盖了官方支持语言中的一部分，导致像 `css`、`html`、`json`、`yaml`、`hcl`、`nix`、`solidity` 等语言即便 `ast-grep` 能解析，skill 也完全不会扫描。
2. 即便部分语言被纳入扫描，如果规则只覆盖函数/类而不覆盖对象、块、配置键、合约等“数据/声明型结构”，AI 看到的结构仍然是不完整的。

本次目标是：

1. 将 `ast-grep` 官方当前内建支持但本地尚未接入的语言全部纳入 skill；
2. 为新增语言补齐一套“最小可用”的结构规则；
3. 保持最终输出仍为 `file`、`language`、`content` 的轻量格式；
4. 在不引入大量重复噪声的前提下，优先提取对 AI 判断文件职责最有帮助的顶层结构。

## 执行步骤

1. 依据 ast-grep 官方语言清单与本地 `LANGUAGE_REGISTRY` 做差集，明确当前缺失的语言、别名与扩展名映射。
2. 扩展 `runtime/lua_skills/codeview_ast/main.lua` 的语言注册表、扩展名映射与注释风格配置，使新增语言可以被扫描与归一化。
3. 为新增语言补充规则文件，优先保证以下类型的结构能被导出：
   - 配置/数据语言：顶层对象、键、块、映射、选择器；
   - 合约/声明语言：contract、interface、library、function、struct、enum 等；
   - 标记语言：可读的顶层标签或区块结构。
4. 视实际测试结果，调整归一化逻辑，避免新增语言出现明显重复结构、空名称或过度冗长的头部摘要。
5. 运行构建同步到 `output/`，并通过真实 MCP 调用对新增语言样例做验证，确认它们能输出可读结构。
6. 对照本计划逐项自检，补充执行变更总结，并在确认完成后将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 以 ast-grep 官方内建语言清单为准，优先覆盖其当前内建支持面。
- 保持 “一语言一 rule bundle” 的组织方式，继续走规则驱动路线。
- 对新增语言采用“最小可用规则集”策略：先覆盖最关键的顶层结构，再逐步增强，而不是一开始追求语言级完备。

## 验收标准

- `LANGUAGE_REGISTRY` 覆盖 ast-grep 官方当前内建支持的语言全集。
- 新增语言至少具备一套可实际导出结构摘要的规则文件，不再是“注册了语言但无结构输出”。
- 扫描这些语言的样例文件时，能返回有意义的 `content` 文本。
- 输出依旧保持轻量格式，不回退到详细 JSON 节点树。
- 代码与新增规则包含必要的中英文双语注释，并完成构建同步与实际验证。

## 执行变更总结

### 1. 核心修复与调整概述

- 依据 ast-grep 官方语言参考页的当前内建语言清单，确认本地 `codeview_ast` 缺失了 `css`、`hcl`、`html`、`json`、`nix`、`solidity`、`yaml` 七类语言支持，并已全部补入本地 skill。
- 扩展了 `LANGUAGE_REGISTRY`、别名、扩展名与注释风格配置，使 skill 对官方内建语言全集具备扫描入口，而不再只停留在原来的 19 种语言。
- 为新增语言分别补齐最小可用规则 bundle，使它们至少能够导出顶层结构，而不是“语言能识别但无任何结构输出”：
  - CSS：选择器与 `@media`
  - HCL：block 与 attribute
  - HTML：element
  - JSON：pair
  - Nix：binding
  - Solidity：contract / interface / library / struct / enum / function
  - YAML：mapping pair
- 在 Lua 归一化逻辑中扩展了 `property`、`tag`、`attribute`、`block` 等结构名推断规则，并补齐更多扩展名映射，使新增语言与原有语言都能以更自然的结构摘要形式输出。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 新增：`runtime/lua_skills/codeview_ast/rules/css.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/hcl.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/html.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/json.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/nix.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/solidity.yml`
- 新增：`runtime/lua_skills/codeview_ast/rules/yaml.yml`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 新增：`output/lua_skills/codeview_ast/rules/css.yml`
- 新增：`output/lua_skills/codeview_ast/rules/hcl.yml`
- 新增：`output/lua_skills/codeview_ast/rules/html.yml`
- 新增：`output/lua_skills/codeview_ast/rules/json.yml`
- 新增：`output/lua_skills/codeview_ast/rules/nix.yml`
- 新增：`output/lua_skills/codeview_ast/rules/solidity.yml`
- 新增：`output/lua_skills/codeview_ast/rules/yaml.yml`
- 修改：`docs/plan/20260413-07-CODEVIEW_AST_BUILTIN_LANGUAGE_EXPANSION.md`

### 3. 💻关键代码调整详情

- `main.lua` 中新增或调整了以下内容：
  - 注册 `css`、`hcl`、`html`、`json`、`nix`、`solidity`、`yaml` 语言；
  - 为现有语言补充更多常见扩展名，如 `tf`、`tfvars`、`htm`、`xhtml`、`pyi`、`sc`、`sbt`、`sol`、`yml` 等；
  - 扩展结构名推断，支持 `property`、`tag`、`attribute`、`block` 类型。
- 新增规则文件采用“最小可用规则集”策略：
  - 先保证每种语言至少有一类对 AI 有辨识价值的顶层结构；
  - 不一开始就追求语言级完备，避免大量低价值噪声结构进入输出。
- 实测验证分两层完成：
  - 样例层：分别用 `ast-grep scan --rule` 验证 7 个新增 rule bundle 均能命中；
  - 真实调用层：启动 `output/debug/vulcan-mcp.exe`，通过 MCP 调用 `codeview_ast` 扫描临时多语言样例目录，结果为：
    - `files_scanned = 7`
    - `files_with_symbols = 7`
    - `items_found = 26`
    - 各语言均返回了可读的 `content` 摘要，例如：
      - CSS：`.button L1-3`
      - HCL：`resource "aws_s3_bucket" "example" L1-3`
      - JSON：`"scripts" L3-5`
      - Solidity：`contract Demo L2-10`

### 4. ⚠️遗留问题与注意事项

- 虽然现在已经覆盖 ast-grep 官方当前内建语言全集，但新增语言规则仍属于“最小可用版”，不是语义完备版。例如：
  - HTML 目前会把嵌套标签完整展开，复杂页面上可能偏啰嗦；
  - JSON / YAML 当前主要按键值对视角输出，还没有更强的顶层对象/数组摘要策略；
  - Nix 目前侧重 binding，不含更复杂的 `let/in`、`with`、module option 结构。
- 后续如果要继续增强，建议按“高价值配置语言优先”的顺序继续细化：
  1. JSON / YAML / HCL / Nix 的顶层容器和配置块摘要
  2. HTML 的结构降噪
  3. Solidity 的 event / modifier / error / state variable
