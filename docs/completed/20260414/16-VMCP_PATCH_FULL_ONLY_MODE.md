# 任务目标

将 `vmcp-patch` 收敛为仅支持整函数替换的安全工具，不再接受 `mode` 参数，也不再暴露 `body` 模式；同时保留并强化现有的通用 AST 校验、错误节点检测与失败回滚能力，并同步更新技能提示词与使用说明。

# 执行步骤

1. 梳理 `vmcp-patch` 当前实现，确认 `mode/body/auto` 相关逻辑、参数暴露点与文档位置。
2. 修改 `runtime/lua_skills/ast-grep/main_patch.lua`：
   - 去除 `mode` 参数校验与分支；
   - 仅保留整函数替换流程；
   - 保留写前临时文件、写后 AST 校验、ERROR 节点检测、失败回滚；
   - 对替换后的目标函数继续做结构身份校验，避免 AI 误改目标身份。
3. 修改 `runtime/lua_skills/ast-grep/skill.json`，移除 `mode` 参数，并在 prompt 中明确只允许传完整函数源码。
4. 更新 `docs/vmcp_patch_usage_cn.md` 与必要的技能说明，使使用方式、失败提示与工具行为保持一致。
5. 同步运行时副本到 `output/lua_skills/ast-grep/`。
6. 使用临时测试文件验证：
   - 正常整函数替换成功；
   - 歧义 selector 返回候选；
   - 传入多余 `}` / `end` 等结构损坏内容时被识别并回滚。
7. 验证通过后补充执行变更总结，并将计划文件归档到 `docs/completed/20260414/`。

# 技术选型

- 保持 `vmcp-patch` 为函数级 AST 重定位工具，不扩展为局部文本 patch。
- 仅支持整函数替换，降低使用歧义与跨语言函数体边界处理复杂度。
- 保留通用 `ERROR` 节点扫描与 AST 重定位校验，避免重新引入语言专属校验器。
- 使用临时测试文件做验证，避免污染真实代码。

# 验收标准

1. `vmcp-patch` 对外不再暴露 `mode` 参数。
2. `vmcp-patch` 仅接受完整函数源码替换。
3. 多余 `}` / `end` 等结构性错误可被识别，并触发失败回滚。
4. 使用说明与技能 prompt 已同步为“仅 full 模式”的最新规则。
5. 临时测试覆盖成功、歧义、失败回滚三类关键场景。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 将 `vmcp-patch` 从多模式工具收敛为**只支持完整函数替换**的单一规则工具。
- 移除了 `mode/body/auto` 的外部行为与参数暴露，不再做兼容推断。
- 保留通用 AST 结构校验、`ERROR` 节点扫描和失败回滚，确保多写 `}` / `end` 等结构错误可被拒绝并恢复原文件。
- 同步更新工具 prompt、技能说明与中文使用文档，明确“必须传完整函数源码”的硬规则。

## 2. 📂 文件变更清单

### 修改

- `runtime/lua_skills/ast-grep/main_patch.lua`
- `runtime/lua_skills/ast-grep/skill.json`
- `docs/lua_skills.md`

### 新增

- `docs/vmcp_patch_usage_cn.md`

### 同步

- `output/lua_skills/ast-grep/main_patch.lua`
- `output/lua_skills/ast-grep/skill.json`

## 3. 💻 关键代码调整详情

- 删除 `vmcp-patch` 的 `mode` 参数校验与 `body/auto` 分支逻辑，入口只接受 `file + selector + replacement`。
- 增加 `validate_mode_absence`，若调用方仍传 `mode`，直接返回 `mode_not_supported`。
- 增加 `validate_full_replacement_shape`，要求 replacement 必须从目标函数声明行开始，否则返回 `replacement_must_be_full_function`。
- 保留 `build_full_replacement_lines` 整函数重缩进方案，并继续复用 AST 重定位、临时文件替换、`ERROR` 节点扫描和回滚流程。
- 更新 `skill.json` 中 `vmcp-patch` 的参数说明与 prompt，明确禁止 body-only 输入。

## 4. ⚠️ 遗留问题与注意事项

- 当前策略是“严格优先于灵活”，因此某些多行声明风格如果不从首行声明开始传入，也会被拒绝；这是当前设计选择，不视为兼容缺陷。
- `vmcp-patch` 仍然只支持 `function / method` 这类完整代码节点，不扩展到字段、类型名、局部变量等低粒度修改。
- 临时测试文件仅用于本次验证，任务结束前应清理，避免污染工作区。
