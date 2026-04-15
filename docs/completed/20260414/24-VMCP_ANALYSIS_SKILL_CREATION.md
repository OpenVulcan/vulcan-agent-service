# 任务计划：创建 VMCP 分析与补丁工作流 Skill

## 任务目标

基于当前对 `vmcp_ast`、`vmcp_rg`、`vmcp_patch` 的实际使用经验，创建一个可复用的 Skill，用于指导后续会话中高效完成仓库级代码建图、局部链路钻取与函数级安全修改，并最终向用户提供该 Skill 的实际存放路径。

## 执行步骤

1. 阅读 `$skill-creator` 的规范，确认 Skill 的目录结构、命名规则、初始化方式与验证要求。
2. 确定 Skill 名称、默认创建路径、触发描述与核心工作流。
3. 使用 `init_skill.py` 在 `C:\Users\20000\.codex\skills` 下初始化 Skill 目录。
4. 编辑生成的 `SKILL.md`，写入面向 `vmcp_ast`、`vmcp_rg`、`vmcp_patch` 的工作流说明与输出规范。
5. 如有需要，检查并更新 `agents/openai.yaml`，确保与 Skill 内容一致。
6. 运行 `quick_validate.py` 校验 Skill 结构与元数据格式。
7. 对照计划完成自检，在文末补充执行变更总结后，将计划归档到 `docs/completed/20260414/`。

## 技术选型

- Skill 默认创建到 `C:\Users\20000\.codex\skills`，以便 Codex 自动发现。
- Skill 名称采用短横线命名，体现其围绕 VMCP 分析与补丁工作流的职责。
- 优先保持 Skill 轻量，仅创建必要的 `SKILL.md` 与 `agents/openai.yaml`，避免无效附属文档。

## 验收标准

- Skill 目录成功创建于可自动发现的位置。
- `SKILL.md` 能清楚描述触发条件、AST/RG/Patch 分工、标准工作流与注意事项。
- `quick_validate.py` 校验通过。
- 最终向用户返回 Skill 的实际地址。
- 计划文件补充完整执行变更总结并完成归档。

## 验证结果

1. 已使用 `init_skill.py` 在 `C:\Users\20000\.codex\skills\vmcp-ast-rg-patch` 成功初始化 Skill。
2. 已完成 `SKILL.md` 正式内容编写，覆盖触发条件、`vmcp_ast` / `vmcp_rg` / `vmcp_patch` 分工、标准工作流、决策规则与输出预期。
3. 已修正初始化生成的 `agents/openai.yaml` 编码与默认提示内容，确保 UI 元数据可正常读取。
4. 已运行 `quick_validate.py`，结果为 `Skill is valid!`，校验通过。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作基于 `skill-creator` 规范，创建了一个新的 Skill `vmcp-ast-rg-patch`。该 Skill 面向结构化代码分析和函数级安全补丁场景，明确规定先用 `vmcp_ast` 建图，再用 `vmcp_rg` 缩小上下文，最后在适用时使用 `vmcp_patch` 进行完整函数替换，并在修改后重新校验结构。

### 2. 📂文件变更清单

- 新增：`C:\Users\20000\.codex\skills\vmcp-ast-rg-patch\SKILL.md`
- 新增：`C:\Users\20000\.codex\skills\vmcp-ast-rg-patch\agents\openai.yaml`
- 新增：`docs/plan/20260414-24-VMCP_ANALYSIS_SKILL_CREATION.md`
- 后续归档目标：`docs/completed/20260414/24-VMCP_ANALYSIS_SKILL_CREATION.md`

### 3. 💻关键代码调整详情

- 通过 `init_skill.py` 初始化 Skill 目录，并生成基础模板文件。
- 将模板 `SKILL.md` 替换为正式版本，内容强调：
  - 新会话先用 `vmcp_ast` 建立代码地图
  - 用 `vmcp_rg` 进行函数/方法级上下文确认
  - 仅在完整函数替换场景下使用 `vmcp_patch`
  - 修改后必须再次执行 AST 或 RG 复核
- 重写 `agents/openai.yaml`，修复初始化时产生的编码异常，并提供清晰的展示名、短描述和默认提示语。

### 4. ⚠️遗留问题与注意事项

- 本次完成的是 Skill 创建与静态结构校验，尚未进行子代理前向测试。
- 如果后续实际使用中发现触发词不够敏感，或输出风格还需进一步收敛，可在真实任务驱动下继续迭代 `description` 与正文流程。
