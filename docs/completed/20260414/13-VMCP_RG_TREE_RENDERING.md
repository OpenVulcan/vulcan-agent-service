# 任务目标

调整 `ast-grep` skill 中 `vmcp-rg` 的结果渲染方式，将当前“列表 + fenced code block”形态改为更稳定的树结构文本，便于模型直接理解“文件 -> 结构 -> 命中行”的层级关系。

# 执行步骤

1. 分析现有 `runtime/lua_skills/ast-grep/main_rg.lua` 中的命中归属与渲染逻辑，确认可以复用的结构标记与分页逻辑。
2. 设计新的树结构输出格式，满足以下目标：
   - 文件内按 AST 层级输出类、impl、函数等结构；
   - 结构节点使用统一的树形前缀与 `[Lx-y]` 行号范围；
   - 命中行作为子节点直接挂在对应结构下；
   - 移除 fenced code block 与当前的 `-` 列表风格。
3. 修改 `main_rg.lua` 的渲染函数，尽量保持现有命中判定、分页和缓存逻辑不变，仅调整最终内容拼装方式。
4. 使用本地调试入口验证 `vmcp-rg` 输出，确认声明命中、函数体命中都能形成清晰的树结构。
5. 若调试结果符合预期，则同步必要文件并补充执行变更总结，最后归档计划文件。

# 技术选型

- 继续复用 `vmcp-rg` 现有的 AST 命中归属逻辑，包括：
  - `annotate_tree_with_rg_hits`
  - `resolve_display_symbol`
  - `append_symbol_match_line`
- 重写树渲染器，基于“是否为兄弟节点最后一项”计算 `├` / `└` / `│` 前缀。
- 命中行统一渲染为 `Lx | text`，结构节点统一渲染为 `signature [Lx-y]`。

# 验收标准

1. `vmcp-rg` 的 `content` 字段改为树结构文本，不再包含 fenced code block。
2. 结构节点显示为 `signature [Lx-y]`。
3. 命中行以子节点形式挂载在对应结构下。
4. 同一文件内多根结构、多层嵌套函数/类型时，树形前缀清晰且不混乱。
5. 本地调试验证通过，输出形态接近目标示例。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `vmcp-rg` 的结果渲染从原先的“列表 + fenced code block”改为真正的树结构文本。
- 结构节点现在统一渲染为 `signature [Lx-y]`，命中行统一作为子节点渲染为 `Lx | text`。
- 进一步细化了命中规则：函数声明或函数体命中时展开完整函数源码；类型/结构声明命中时只显示结构头；非函数体命中时仅保留命中的具体行。
- 保留了现有的 AST 命中归属、分页与缓存逻辑，仅调整渲染与命中展现策略，降低对现有行为的扰动。

### 2. 📂文件变更清单

新增：

- `D:\\projects\\vulcan-mcp-client\\docs\\plan\\20260414-13-VMCP_RG_TREE_RENDERING.md`

修改：

- `D:\\projects\\vulcan-mcp-client\\runtime\\lua_skills\\ast-grep\\main_rg.lua`
- `D:\\projects\\vulcan-mcp-client\\output\\lua_skills\\ast-grep\\main_rg.lua`
- `D:\\projects\\vulcan-mcp-client\\docs\\lua_skills.md`

删除：

- 无

### 3. 💻关键代码调整详情

- 在 `runtime/lua_skills/ast-grep/main_rg.lua` 中删除了旧的 `append_match_block` 与基于 `-` 列表的渲染方式。
- 新增树结构渲染辅助函数：
  - `build_tree_prefix`
  - `append_tree_line`
  - `collect_render_children`
  - `append_symbol_tree`
- 新增源码片段渲染辅助函数：
  - `mark_symbol_expand_source`
  - `read_file_lines`
  - `build_source_excerpt_lines`
- 渲染规则调整为：
  - 结构节点：`signature [Lx-y]`
  - 命中行：`Lx | text`
  - 根节点与子节点统一使用 `├` / `└` / `│` 维护层级关系
- 命中策略调整为：
  - 函数/方法命中：展开完整函数源码片段
  - 类型/结构声明命中：只保留结构头
  - 非函数体命中：仅保留命中的具体源码行
- 在 `docs/lua_skills.md` 中同步补充了 `vmcp-rg` / 类似工具的树结构输出约定说明。

### 4. ⚠️遗留问题与注意事项

- 当前返回仍保留 `file` 与 `content` 的 JSON 外壳，文件路径不直接写进 `content` 内，这是为了兼容现有分页与结构化结果格式。
- 当前函数源码展开不会再应用“最多 12 行”的命中行限制，因此超长函数会主要依赖分页预算进行拆分。
- 本次只调整了 `vmcp-rg` 的渲染形态，没有改变 `vmcp-ast` 的输出格式。
