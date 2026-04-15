> 迁移说明（2026-04-15）：
> 本文记录的是历史版本设计，文中的 `ignore` 参数语义已经失效。
> 当前运行时统一改为 `noignore`：
> - 默认不传时，仍启用 `.gitignore`、`.ignore` 与内建忽略规则。
> - 仅当显式传入 `noignore=true` 时，才关闭忽略规则并扫描原本被过滤的路径。

# 任务目标

增强 `codeview_ast` 的目录扫描治理能力，避免在项目根目录扫描时将 `target`、`output`、`node_modules` 等构建产物或依赖目录误纳入结果。同时补足目录级忽略规则与文件数量上限控制，并将 ast-grep 调用优化为“按语言优先整批提交”的模式。

# 执行步骤

1. 审查当前 `codeview_ast` 的目录遍历、文件收集、批量扫描与参数校验流程，定位可复用与需重构的部分。
2. 增加默认忽略目录集合，并引入新的 `ignore` 布尔参数，默认启用忽略逻辑，显式传 `false` 时关闭默认忽略与 `.gitignore/.ignore` 规则。
3. 在目录遍历阶段支持读取当前目录下的 `.gitignore` 与 `.ignore` 文件，并让这些规则只作用于对应目录子树。
4. 将最终文件清单的数量限制为 200；当超出上限时，返回结构化错误，提示用户缩小路径范围或细化文件列表。
5. 将 ast-grep 扫描策略从固定小批次改为“按语言优先整批提交，必要时再回退分批”，减少重复进程启动开销。
6. 更新 `skill.json` 的参数描述与提示文案。
7. 通过构建与真实 MCP 回归验证以下场景：
   - 默认忽略开启时，项目根目录扫描不会包含 `target/output/node_modules` 等目录内容。
   - `ignore = false` 时，默认忽略与 `.gitignore/.ignore` 规则都会被禁用。
   - 子目录级 `.gitignore/.ignore` 规则只影响对应目录子树。
   - 文件数量超过 200 时返回预期错误。
   - 按语言整批扫描仍能正常返回结构结果。

# 技术选型

- 继续在 Lua 层实现目录遍历与忽略规则判断，避免扩大 Rust 侧接口变更范围。
- 默认忽略目录采用常量表快速判定，降低大目录扫描时的额外开销。
- `.gitignore/.ignore` 解析先覆盖当前场景最常见、最关键的规则：空行、注释、普通路径、目录路径、通配符与 `!` 反选。
- ast-grep 调用保留回退策略：优先整批提交同语言文件列表，若单次调用失败且文件数较多，再自动按批拆分，兼顾效率与稳定性。

# 验收标准

1. 默认情况下，`target`、`node_modules`、`output` 等常见目录不会被扫描进结果。
2. `ignore = false` 时，忽略目录常量表和 `.gitignore/.ignore` 都不生效。
3. 存在子目录级 `.gitignore/.ignore` 时，仅该目录及其子树受规则影响。
4. 最终文件清单超过 200 时，技能直接返回结构化错误，不继续执行 ast-grep。
5. 构建通过，且真实 MCP 调用验证通过。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 为 `codeview_ast` 增加默认忽略目录集合，默认屏蔽 `target`、`node_modules`、`output`、`vendor`、`build` 等高噪声目录。
- 新增 `ignore` 布尔参数，默认值为 `true`；当传入 `false` 时，同时关闭默认忽略目录与 `.gitignore/.ignore` 规则识别。
- 目录遍历链路新增子目录级 `.gitignore/.ignore` 解析与作用域继承，确保规则只对其所在目录子树生效，不污染兄弟目录。
- 新增文件数量上限控制：匹配文件数超过 200 时，直接返回结构化错误并提示缩小范围。
- ast-grep 调用策略改为“按语言分组、每批最多 50 个文件”，以规避 Windows `CreateProcess` 32,767 字符命令行上限；若某批失败，再自动回退到更小批次。

## 2. 📂文件变更清单

- 新增：`docs/plan/20260413-13-CODEVIEW_AST_IGNORE_RULES_AND_FILE_LIMIT.md`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`output/lua_skills/codeview_ast/main.lua`
- 修改：`output/lua_skills/codeview_ast/skill.json`

## 3. 💻关键代码调整详情

- 在 `main.lua` 中新增 `DEFAULT_IGNORES`、`IGNORE_RULE_CACHE`、`MAX_AST_GREP_BATCH_FILES = 50`、`MAX_MATCHED_FILES = 200` 等运行时治理常量。
- 在 `main.lua` 中新增忽略规则相关工具函数，包括路径标准化、作用域相对路径计算、简化版 gitignore 通配符匹配、单行规则解析、目录级忽略规则缓存读取，以及目录项忽略判断。
- 将 `collect_files_for_path(...)` 重构为支持 `ignore_enabled` 控制的目录遍历流程；目录模式下会叠加当前目录 `.gitignore/.ignore` 规则并作用于对应子树。
- 将 `collect_files(...)` 重构为“多路径聚合 + 去重 + 文件上限控制”链路，当匹配数量超过 200 时返回 `too_many_matched_files` 结构化错误。
- 新增 `validate_ignore_argument(...)` 并在技能入口中接入 `ignore` 参数；最终返回结构中同步包含 `ignore` 字段。
- 将 `run_language_scan(...)` 重构为按语言批处理：常规批次上限为 50 个文件，异常时回退到更小批次重新执行。
- 更新 `skill.json` 参数描述与 prompt，明确默认忽略行为、`ignore=false`、200 文件上限与目录规则生效方式。

## 4. ⚠️遗留问题与注意事项

- 当前 `.gitignore/.ignore` 解析聚焦本轮最需要的规则能力：空行、注释、普通路径、目录路径、`*`、`**`、`?` 与 `!` 反选。更复杂的转义与极端边界规则未在本轮进一步扩展。
- 默认忽略目录是硬过滤策略；如果用户明确需要扫描这些目录，应显式传入 `ignore = false` 或直接把目标路径指向该目录。
- 文件数量上限是在“最终去重后的待扫描文件列表”维度生效，超过上限后不会继续执行 ast-grep。
- 实测验证结果：
  - 构建：`scripts/build.ps1` 成功。
  - 临时夹具目录验证：
    - 默认忽略开启时，返回 4 个 Rust 文件，仅保留 `src/kept.rs`、`pkg/kept.rs`、`nested/keep.rs`、`other/generated/visible.rs`。
    - `ignore = false` 时，返回 10 个 Rust 文件，`target/output/vendor/build/pkg/generated/nested/skip.rs` 均重新可见。
    - 201 个 TypeScript 文件场景下，返回 `too_many_matched_files`，并带出 `matched_files = 201`、`limit = 200`。
    - 55 个 TypeScript 文件场景下，扫描成功，返回 `files_scanned = 55`、`files_with_symbols = 55`、`items_found = 55`，验证 50 文件批次切分后流程可用。
  - 当前仓库根目录验证：
    - 以 `path = D:\\projects\\vulcan-mcp-client`、`recursive = true`、`ext = rs` 调用时，返回 `files_scanned = 11`、`files_with_symbols = 11`，且 `target_matches = 0`、`output_matches = 0`。
