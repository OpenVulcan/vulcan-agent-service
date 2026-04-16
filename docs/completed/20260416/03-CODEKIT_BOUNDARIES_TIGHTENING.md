# 任务目标

收紧 Vulcan CodeKit 本地 skill 与公共 prompt 中的工具降级边界，避免模型在代码分析场景中过早退回普通文本工具，确保代码文件默认优先使用 CodeKit 结构化工具，仅在纯配置、纯字面量或纯文件发现等低结构价值场景降级。

# 执行步骤

1. 检查当前本地 skill 与公共 prompt 的正文，确认现有 `Boundaries`、`Quick Decision Tree`、`Tool Notes` 中与降级相关的表述。
2. 调整本地 skill 正文，保持整体结构不变，重点收紧 `Boundaries` 与相关工具说明，使代码分析默认优先 CodeKit。
3. 调整公共 prompt 正文，确保与本地 skill 主体保持一致，仅保留公共 prompt 自身必要的动态包装逻辑。
4. 进行一致性检查，确认本地 skill 与公共 prompt 的正文主体一致，且 Lua prompt 语法未被破坏。
5. 在计划文件中补充执行变更总结，确认完成后迁移到 `docs/completed/20260416/`。

# 技术选型

1. 维持现有 “行为原则 + 决策树 + 工作流 + 边界” 的提示词结构，不重做整体框架。
2. 将 `Boundaries` 从“轻量优先”改为“结构优先，纯文本例外”。
3. 保留普通工具回退通道，但仅限：
   - JSON/YAML/TOML/env 等纯配置文件
   - 只关心字面量本身、不关心归属上下文的字符串定位
   - 纯文件名/扩展名发现
   - 极小、已知、非代码文件的直接阅读
4. 对代码文件场景明确强调：
   - `codekit-rg` 优先于普通 grep
   - `codekit-ast-detail` 优先于普通 read_file

# 验收标准

1. 本地 skill 与公共 prompt 主体正文一致。
2. `Boundaries` 不再鼓励对普通代码分析场景过早降级。
3. 公共 prompt Lua 文件仍可正常返回完整文本，未破坏原有动态包装逻辑。
4. 修改后的提示词能清楚表达：代码分析默认优先 CodeKit，普通工具仅作为低结构价值场景的例外回退。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次调整未重写 Vulcan CodeKit 的整体提示词结构，而是在保留现有“行为原则 + 决策树 + 工具说明 + 工作流 + 边界”框架的前提下，重点收紧了 `Boundaries` 和相关工具说明。核心变化是将默认心智从“轻量优先”调整为“结构优先”，明确代码分析场景默认优先使用 CodeKit，仅在纯配置文件、纯字面量搜索、纯文件发现或极小非代码文件直读等低结构价值场景才回退到普通工具。

## 2. 📂文件变更清单

### 修改文件

- `C:\Users\20000\.codex\skills\vulcan-codekit\SKILL.md`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\prompts\vulcan_codekit_skill.lua`

### 新增文件

- `D:\projects\vulcan-mcp-client\docs\plan\20260416-03-CODEKIT_BOUNDARIES_TIGHTENING.md`（随后归档）

## 3. 💻关键代码调整详情

### 本地 skill 正文调整

- 将“如果只是轻量搜索或小文件读取就优先普通工具”的表述改为“代码分析默认优先 CodeKit”。
- 将降级条件收紧为：
  - JSON/YAML/TOML/env 等纯配置文件
  - 不关心归属关系的纯字面量搜索
  - 极小且已知的非代码文件直读
  - 纯文件名或扩展名发现
  - 不依赖 AST 归属上下文的行级微调
- 在 `codekit-ast-detail` 与 `codekit-rg` 的 `Remember` 段中补充了优先于 `read_file` / `grep` 的适用说明。

### 公共 prompt 正文同步

- 对 `vulcan_codekit_skill.lua` 中的 `base_prompt` 做了与本地 skill 同步的同等调整。
- 修复了 prompt 主体末尾换行差异，确保公共 prompt 与本地 skill 主体文本完全一致。

### 校验动作

- 通过 Python 脚本对比本地 skill 去除 frontmatter 后的正文与公共 prompt 中 `base_prompt` 内容，结果为 `BODY_MATCHED`。
- 执行 `quick_validate.py` 校验本地 skill，结果为 `Skill is valid!`。

## 4. ⚠️遗留问题与注意事项

- 当前仅收紧了 `Boundaries` 与少量工具说明，没有修改 `Quick Decision Tree` 的主结构，因此后续仍可继续针对“默认优先 CodeKit”的倾向做更细力度优化。
- 本次没有改动 `skill.json` 或 Lua skill 运行时注册逻辑，因此现有 prompt 装配方式与动态包装逻辑保持不变。
