## 任务目标

本次任务是在上一轮 `vmcp-ast / vmcp-rg` 输出重构基础上继续收口两项规则：

1. 超限自动落盘的临时文件统一改为 Markdown 格式，而不是 JSON。
2. 当调用方未显式传入 `ext` 时，默认只扫描更适合代码理解的源代码语言，主动排除 `css/html/json/yaml` 等非核心代码格式。

## 执行步骤

1. 梳理 `vmcp-ast` 与 `vmcp-rg` 当前的超限落盘逻辑，改为写入完整 Markdown 结果，并同步更新返回字段与提示文案。
2. 梳理 `vmcp-ast` 当前语言注册表与默认扩展处理逻辑，定义“默认允许的代码语言集合”。
3. 在 `vmcp-ast` 中实现“未传 `ext` 时自动使用默认代码语言集合”的收口策略，并让 `vmcp-rg` 复用同一套默认扩展过滤逻辑。
4. 更新 `skill.json`、开发文档与运行时副本，确保提示词与实际行为一致。
5. 通过本地调试验证：
   - 超限场景落盘文件后缀为 `.md`
   - 提示消息给出完整 Markdown 文件路径
   - 未传 `ext` 时不会把 `css/html/json/yaml` 扫进去

## 技术选型

- 继续沿用 Lua 层结果渲染逻辑，超限时直接复用现有 Markdown 渲染函数写出完整结果。
- 默认扩展过滤采用“语言注册表 -> 代码语言白名单 -> 扩展集合”的方式统一生成，避免在多个工具中维护不同列表。
- 不做旧行为兼容，保持规则单一清晰。

## 验收标准

1. `vmcp-ast` 与 `vmcp-rg` 超限自动落盘文件为 `.md`，且提示路径与实际文件一致。
2. 不传 `ext` 时，默认不会扫描 `css/html/json/yaml` 这类文件。
3. `vmcp-rg` 通过复用同一套默认扩展逻辑，行为与 `vmcp-ast` 保持一致。
4. 相关说明文档与 `skill.json` 已同步更新。
5. 本地调用验证通过。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vmcp-ast` 与 `vmcp-rg` 的超限自动落盘文件统一改为 Markdown 格式，返回提示中的完整路径也同步指向 `.md` 文件。
- 为 `vmcp-ast` 引入统一的默认代码语言白名单；在未传 `ext` 时自动排除 `css/html/json/yaml` 等非核心代码格式。
- 让 `vmcp-rg` 直接复用 `vmcp-ast` 的扩展过滤逻辑，保持两个工具在默认扫描范围上的一致性。
- 同步更新 `skill.json` 与开发文档，确保提示词、参数说明、默认行为与实际实现一致。

### 2. 📂文件变更清单

#### 修改

- `runtime/lua_skills/ast-grep/main.lua`
- `runtime/lua_skills/ast-grep/main_rg.lua`
- `runtime/lua_skills/ast-grep/skill.json`
- `docs/lua_skills.md`

#### 新增

- `docs/plan/20260414-18-AST_RG_MARKDOWN_SPILL_AND_DEFAULT_LANG_FILTER.md`

### 3. 💻关键代码调整详情

- `runtime/lua_skills/ast-grep/main.lua`
  - 新增默认代码语言白名单，并在构建 `EXTENSION_MAP` 时同步生成默认扩展集合。
  - 调整 `validate_extension_argument`，未传 `ext` 或传空值时默认返回代码语言扩展集合。
  - 将大结果落盘从 `.json` 改为 `.md`，并复用 `build_ast_markdown` 输出完整内容。
- `runtime/lua_skills/ast-grep/main_rg.lua`
  - 保持结果大小判断仍基于内联 MCP JSON 编码大小，但实际落盘改为 `.md` 文件。
  - 与 `vmcp-ast` 对齐，返回字段中的完整文件路径现在指向 Markdown 文件。
- `runtime/lua_skills/ast-grep/skill.json`
  - 更新 `ext` 参数说明，明确未传时的默认语言范围。
  - 更新 `workdir` 与 prompt 文案，明确超限时写出的是完整 Markdown。
- `docs/lua_skills.md`
  - 补充“默认语言范围”说明。
  - 将大结果处理规则从“完整 JSON 落盘”更新为“完整 Markdown 落盘”。

### 4. ⚠️遗留问题与注意事项

- 当前大结果判定阈值仍基于“内联返回 JSON 编码大小是否超过 10000 字节”，这是为了匹配 MCP 客户端的实际截断风险；但落盘文件本身现在统一为 Markdown。
- 本轮已通过本地调试验证：
  - `vmcp-ast` 大结果会落盘为 `.md`
  - `vmcp-rg` 大结果会落盘为 `.md`
  - 未传 `ext` 时，临时测试目录中只有 `demo.rs` 被扫描，`json/html/css/yaml` 均被排除
