# 任务计划：优化 vmcp-ast 的备注提取与摘要输出

## 任务目标

调整 `vmcp-ast` 在 `comment=true` 场景下的备注提取逻辑，使其更适合真实代码库中的中英文双语注释与大段区域说明。目标包括：

- 排除 `// -----------`、`// ========` 等分隔线或区域注释噪音
- 排除 `@param`、`@returns`、`参数 / Parameters`、`返回 / Returns` 这类说明段落中的结构标签噪音
- 将多行有效备注合并为一行
- 每个函数/方法的备注仅保留核心摘要的前最多 50 字节内容

## 执行步骤

1. 定位 `vmcp-ast` 注释提取与注释展示链路，确认 `extract_leading_comment`、`extract_docstring` 和 `symbol.comment` 的赋值流程。
2. 设计统一的备注摘要后处理逻辑，对原始注释文本做清洗、过滤、合并和截断。
3. 实现以下规则：
   - 过滤纯分隔符样式注释行
   - 过滤参数/返回值标签行
   - 合并有效备注为单行
   - 以 UTF-8 安全方式截断到最多 50 字节
4. 将摘要逻辑接入 `vmcp-ast` 的注释输出路径。
5. 视情况更新工具定义说明，使 `comment` 参数含义与新行为一致。
6. 使用临时文件做最小验证，确认中英文备注、分隔线备注和多行备注都能正确压缩成简洁摘要。
7. 记录执行变更总结并归档到 `docs/completed/20260415/`。

## 技术选型

- 不直接改变 AST 扫描与符号识别逻辑，只在注释提取后的归一化阶段做摘要处理。
- 采用启发式规则过滤噪音行，以兼容不同语言的双语注释风格。
- 采用 UTF-8 安全截断，避免中文多字节字符被截断到半个字符。

## 验收标准

- 分隔线样式注释不会进入 `note:` 输出。
- 多行中英文备注会被压缩为单行核心摘要。
- 单条备注最多输出 50 字节的有效内容。
- `comment=true` 时输出仍然稳定可读，不影响函数/方法结构扫描。

## 验证结果

1. 已在 `runtime/lua_skills/ast-grep/main.lua` 中新增统一的备注摘要后处理逻辑，覆盖：
   - 分隔线装饰过滤
   - 参数/返回值标签过滤
   - 多行备注合并为单行
   - UTF-8 安全的 50 字节截断
2. 已修正块注释路径，避免在进入摘要阶段前过早压成单行，导致 `* ======` 这类分隔线和正文混合。
3. 已同步更新 `runtime/lua_skills/ast-grep/skill.json` 和 `docs/lua_skills.md`，明确 `comment=true` 输出的是压缩后的备注摘要，而不是完整注释原文。
4. 使用临时 TypeScript 文件通过 `--call-tools vmcp-ast` 完成实际验证，结果如下：
   - `summarizeInput` 的 `note:` 输出为 `中文：这是一个很长的函数备注信息`
   - `normalizeInput` 的 `note:` 输出为 `中文：第二个函数用于验证块注释中`
   - 分隔线注释与 `@param` / `@returns` 元信息均未进入最终输出
5. 验证期间生成的临时文件、联接目录和依赖产物已清理完成。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作优化了 `vmcp-ast` 在 `comment=true` 场景下的备注提取方式。新的逻辑不再直接输出完整注释块，而是基于启发式规则提炼出“单行核心摘要”，这对存在大量中英文双语注释、参数说明段落和装饰性分隔线的代码库更友好，能显著降低备注噪音，同时保留对函数意图的快速感知能力。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`
- 新增：`docs/plan/20260415-03-AST_COMMENT_NOTE_SUMMARIZATION.md`
- 后续归档目标：`docs/completed/20260415/03-AST_COMMENT_NOTE_SUMMARIZATION.md`

### 3. 💻关键代码调整详情

- 新增 `MAX_COMMENT_SUMMARY_BYTES = 50`，统一限制备注摘要最大长度。
- 新增注释清洗与摘要辅助函数：
  - `strip_comment_decorations`
  - `is_separator_comment_line`
  - `is_comment_metadata_line`
  - `utf8_truncate_by_bytes`
  - `summarize_comment_text`
- 在 `normalize_symbol` 中对 `extract_leading_comment` / `extract_docstring` 的结果统一调用 `summarize_comment_text`，确保所有语言路径共享相同摘要规则。
- 调整 `extract_leading_comment` 的块注释分支，保留逐行内容进入摘要阶段，从而更准确过滤块注释中的装饰行。

### 4. ⚠️遗留问题与注意事项

- 当前实现基于启发式规则，已经足够准确地处理绝大多数常见中英文注释风格，但对极端自定义格式的注释仍可能存在误判空间。
- 50 字节上限是偏保守的压缩策略，优点是不会让备注噪音淹没结构图；如果后续有更强的摘要需求，可以再改成可配置参数。
