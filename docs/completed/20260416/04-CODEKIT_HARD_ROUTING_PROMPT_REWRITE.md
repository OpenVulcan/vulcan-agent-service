# 任务目标

将 Vulcan CodeKit 的本地 skill 与公共 prompt 从“建议式工具选择”升级为“强约束工具路由”，显著提高 `codekit-ast-tree`、`codekit-rg`、`codekit-ast-detail` 在代码分析场景下的默认调用优先级，并补充返回形态预期说明，减少模型因不确定结果形态而降级到普通工具。

# 执行步骤

1. 审查当前本地 skill 与公共 prompt 中的 `Quick Decision Tree`、`Boundaries`、`Tool Notes` 等相关段落，识别仍然允许模型合法降级的表述。
2. 在保持整体结构基本稳定的前提下，引入更强约束的 `MANDATORY TOOL ROUTING` 段落，明确项目映射、结构化搜索、源码检查的默认强制工具。
3. 引入 `OUTPUT PREDICTION` 段落，明确 `codekit-ast-tree`、`codekit-rg`、`codekit-ast-detail` 及 overflow pointer 的典型输出形态。
4. 调整 `Boundaries`，将普通工具退化为严格例外路径，而非平行默认路径。
5. 同步更新本地 skill 与公共 prompt，确保两边正文主体一致。
6. 进行正文一致性和 skill 校验，并在完成后总结剩余不足与后续可优化点。
7. 在计划文件末尾追加执行变更总结，并归档至 `docs/completed/20260416/`。

# 技术选型

1. 保持现有“行为原则 + 决策树 + 主代理规则 + 工具说明 + 工作流 + 边界”框架，避免整体认知模型重建。
2. 新增“强路由”层，明确：
   - 代码库建图优先 `codekit-ast-tree`
   - 结构化搜索优先 `codekit-rg`
   - 源码检查优先 `codekit-ast-detail`
3. 对普通工具仅保留严格例外场景：
   - 纯配置文件
   - 纯字面量定位且不关心归属
   - 纯文件名或扩展名发现
   - 极小、非结构化、纯文本微调
4. 新增结果预期说明，降低模型对结构化工具输出形态的陌生感和不确定感。

# 验收标准

1. 本地 skill 与公共 prompt 主体正文保持一致。
2. 提示词明确体现“代码分析默认强制优先 CodeKit”。
3. 结果预期说明已加入，且覆盖 `ast-tree`、`rg`、`ast-detail`、overflow pointer。
4. 普通工具已被降为严格例外路径，而非与 CodeKit 并列的常规选择。
5. 本地 skill 校验通过，公共 prompt 未破坏现有 Lua 动态包装逻辑。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次将 Vulcan CodeKit 的提示词从“建议式默认优先”进一步收紧为“强约束默认路由”。核心变化有三点：

1. 增加 `Mandatory Tool Routing`，明确项目建图、结构搜索、源码检查的默认强制工具。
2. 增加 `Output Prediction`，主动告诉模型各工具的典型输出形态与 overflow 返回方式。
3. 将 `Boundaries` 收紧为严格例外规则，普通工具不再作为代码分析场景的平行常规路径。

同时，我对修改后的提示词做了复查，进一步消除了 `Quick Decision Tree` 之后与 `Mandatory Tool Routing` 重复的“默认优先”段落，使强约束规则更集中、更不分散。

## 2. 📂文件变更清单

### 修改文件

- `C:\Users\20000\.codex\skills\vulcan-codekit\SKILL.md`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\prompts\vulcan_codekit_skill.lua`

### 新增文件

- `D:\projects\vulcan-mcp-client\docs\plan\20260416-04-CODEKIT_HARD_ROUTING_PROMPT_REWRITE.md`（随后归档）

## 3. 💻关键代码调整详情

### 强约束路由新增

- 新增 `Mandatory Tool Routing` 段落，明确：
  - 对陌生代码库或非平凡源码目录，必须先 `codekit-ast-tree`
  - 对源码中的符号、函数、方法、类、日志字符串、正则锚点搜索，默认用 `codekit-rg`
  - 对源码文件检查，默认用 `codekit-ast-detail`
  - 普通工具仅在纯配置文件、纯文件发现、纯字面量定位、极小非结构文本修改等场景才允许回退

### 输出预期新增

- 新增 `Output Prediction` 段落，说明：
  - `codekit-ast-tree` 会返回带紧凑指标的 Markdown 树
  - `codekit-rg` 会返回命中行与所属函数/类上下文
  - `codekit-ast-detail` 会返回带嵌套关系的符号树
  - 超限时会返回 `raw_file` 与 host-safe `offset/limit` chunk 计划

### 冗余内容压缩

- 将 `Quick Decision Tree` 后原本的“Prefer CodeKit by default...”段落收敛为一句承接语，避免与后续 `Mandatory Tool Routing` 重复表达。

### 一致性与校验

- 使用 Python 脚本比对本地 skill 去掉 frontmatter 后的正文与公共 prompt 中 `base_prompt`，结果为 `BODY_MATCHED`
- 使用 `quick_validate.py` 校验本地 skill，结果为 `Skill is valid!`

## 4. ⚠️遗留问题与注意事项

### 当前已改善但仍需注意的不足

1. 当前提示词已明显更偏向 CodeKit，但仍属于“文本硬约束”，并非运行时强制执行；如果某些宿主在工具选择阶段有自己的启发式偏置，仍可能出现少量降级行为。
2. `Failure and Fallback` 仍保留了故障时回退普通工具的说明，这在工具异常时是必要的，但也意味着极少数模型可能在真正失败前就尝试走保守路径。
3. `Mandatory Tool Routing` 已经很强，但仍保留了“non-trivial / trivial / narrow fallback cases”这类判断词，后续若要进一步提高命中率，可以继续把“什么叫 trivial / non-trivial”描述得更工程化。
4. 当前提示词已经更适合推广期与能力展示期；如果将来进入稳定期，可能需要再平衡一次“强路由”与“调用成本”之间的取舍。
