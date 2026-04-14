# 任务目标

修复 `vmcp-patch` 当前在写入安全性与替换边界上的主要风险，并补充一份高质量使用说明，确保该工具在实际 AI 调用链中既安全又易用。

# 执行步骤

1. 梳理 `vmcp-patch` 当前实现，重点确认以下风险点：
   - 写盘后无 AST 自检
   - 写盘过程缺少更安全的临时文件替换流程
   - `body` 模式的函数体边界提取是否存在误判
2. 检查 Lua 层与 Rust 宿主目前可用的文件系统能力，选择最稳妥的安全写入方案。
3. 修改 `runtime/lua_skills/ast-grep/main_patch.lua`，至少实现：
   - 写前临时文件落盘
   - 写后 AST 自检
   - 自检失败回滚
   - 更安全的最终文件替换
4. 视情况补充 Rust 宿主能力或 Lua 技能说明文档，使方案对外可解释、可维护。
5. 编写高质量使用说明，明确：
   - 何时优先用 `vmcp-patch`
   - selector 如何逐步收敛
   - `auto/full/body` 三种模式的使用建议
   - 安全边界与失败重试方式
6. 使用临时测试文件完成验证，覆盖：
   - 成功 patch
   - AST 自检失败并回滚
   - 歧义 selector 返回候选
7. 完成验证后补充执行变更总结，并将计划文件归档到 `docs/completed/20260414/`。

# 技术选型

- 优先在 Lua 层实现安全改造，避免不必要的宿主接口膨胀。
- 若 Lua 层无法稳妥完成“临时文件替换 + 回滚”，再补 Rust 宿主原子写能力。
- `vmcp-patch` 继续只支持函数级 patch，不扩展到类型名、字段名、局部变量名等低粒度修改。
- 使用临时测试文件验证异常场景，避免污染真实业务代码。

# 验收标准

1. `vmcp-patch` 写盘后具备 AST 自检能力。
2. AST 自检失败时，文件内容能够恢复到写前状态。
3. 写盘流程不再是单次直接覆盖，具备更安全的临时文件替换机制。
4. 高质量使用说明已经补齐，且与工具当前行为一致。
5. 临时测试覆盖成功与失败两类关键场景。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 重构 `vmcp-patch` 的写后校验逻辑，移除了仅针对 Rust 的 `rustfmt` 校验分支，改为基于 ast-grep 的通用 `ERROR` 节点扫描与 AST 结构重定位校验。
- 保留并强化了临时文件写入、备份文件回滚、最终替换后再校验的安全闭环，确保 patch 失败时可恢复原文件。
- 增加目标函数的稳定身份路径校验，避免仅靠旧行号或弱 selector 继续写入错误位置。
- 补充并提升 `vmcp-patch` 中文使用说明，明确 selector 收敛方式、失败类型、回滚语义与最佳实践。

## 2. 📂 文件变更清单

### 修改

- `runtime/lua_skills/ast-grep/main_patch.lua`
- `runtime/lua_skills/ast-grep/skill.json`

### 新增

- `docs/vmcp_patch_usage_cn.md`

### 同步

- `output/lua_skills/ast-grep/main_patch.lua`
- `output/lua_skills/ast-grep/skill.json`

## 3. 💻 关键代码调整详情

- 在 `main_patch.lua` 中新增 `build_symbol_identity_path`，用结构名称路径而不是旧行号重新识别同一函数节点。
- 新增 `build_error_node_rule`、`scan_ast_error_nodes`、`summarize_error_node_matches`，通过 ast-grep 通用 `ERROR` 节点扫描跨语言探测语法损坏。
- 重写 `validate_ast_after_write`，将写后校验改为三层：
  - AST 可重新扫描；
  - 不存在 `ERROR` 节点；
  - 原目标函数仍能被唯一重定位，且 `body` 模式不允许改变函数声明外壳。
- 修复 Lua 前向引用问题，确保 `validate_ast_after_write` 内可稳定调用 `find_matching_patch_targets`。
- 更新 `vmcp-patch` 工具 prompt 与中文说明文档，明确临时文件、回滚、歧义 selector、`syntax_error_nodes_detected` 等行为。

## 4. ⚠️ 遗留问题与注意事项

- 当前 `body` 模式仍主要面向花括号函数语言，非花括号语言如果只传函数体内容，稳定性仍有限。
- `ERROR` 节点扫描属于通用解析错误防线，但不等同于各语言完整编译检查；它的目标是保证 AST 工具链继续可用，而不是替代编译器。
- 本次验证使用了临时测试文件，已在任务结束前清理，未保留测试产物。
- 工作区中仍存在其他历史未整理改动（如 `docs/lua_skills.md`、`src/main.rs`、`runtime/lua_skills/ast-grep/main_rg.lua` 等），本次未回退也未额外修改其既有内容。
