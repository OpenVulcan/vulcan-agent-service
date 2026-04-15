# 任务目标

统一取消 `codekit-ast` 与 `codekit-rg` 的 `workdir` 参数，并移除“大结果优先写入工作目录”的行为，确保缓存结果不再落到用户工作目录中，避免干扰模型对仓库真实状态的判断。

# 执行步骤

1. 定位 `codekit-ast` 与 `codekit-rg` 中与 `workdir` 参数校验、解析、落盘目录解析相关的实现。
2. 移除两个工具的 `workdir` 参数与对应校验逻辑。
3. 将大结果落盘目录统一改为 `vulcan.temp_dir/mcp/cache/`，不再写入工作目录。
4. 同步更新 `skill.json` 与 `docs/lua_skills.md` 中的对外说明，避免调用方继续感知或使用 `workdir`。
5. 完成验证并补齐执行变更总结后，按规范归档计划文件。

# 技术选型

- 采用最小范围修改，仅调整 `ast` / `rg` 的参数层与缓存落盘目录解析逻辑。
- 保留 `export_md_path` 能力，便于显式导出场景继续使用。
- 对“超出内联预算的自动落盘”统一收敛到宿主提供的 `vulcan.temp_dir`，避免污染仓库工作区。

# 验收标准

1. `codekit-ast` 与 `codekit-rg` 对外参数中不再包含 `workdir`。
2. 两个工具在大结果自动落盘时仅写入 `vulcan.temp_dir/mcp/cache/`。
3. `skill.json` 与 `docs/lua_skills.md` 中相关说明同步更新。
4. 现有 AST 备注回归验证通过，确认主流程未被破坏。

---

# 执行变更总结

## 1. 核心修复与调整概述

已统一移除 `codekit-ast` 与 `codekit-rg` 的 `workdir` 参数，并取消“自动把缓存结果写入工作目录”的行为。现在两个工具在结果过大需要自动落盘时，都会统一写入 `vulcan.temp_dir/mcp/cache/`，从而避免在仓库目录中产生额外缓存文件干扰模型判断。

## 2. 📂文件变更清单

修改：

- `runtime/lua_skills/vulcan-codekit/main.lua`
- `runtime/lua_skills/vulcan-codekit/main_rg.lua`
- `runtime/lua_skills/vulcan-codekit/skill.json`
- `docs/lua_skills.md`
- `docs/plan/20260415-13-AST_RG_REMOVE_WORKDIR_CACHE.md`

新增：

- 无

删除：

- 无

## 3. 💻关键代码调整详情

- 删除了 `codekit-ast` 与 `codekit-rg` 中的 `validate_workdir_argument`、入口参数解析以及 `finalize_*_result` 中对 `workdir` 的透传。
- 将两个工具的 `resolve_large_result_directory` 改为无参实现，统一返回 `vulcan.temp_dir/mcp/cache/`。
- 更新 `skill.json`，从 `codekit-ast` / `codekit-rg` 的参数定义里移除 `workdir`，并同步修改提示词描述。
- 更新 `docs/lua_skills.md`，明确说明大结果自动落盘只走 `vulcan.temp_dir/mcp/cache/`，不再写入工作目录。

## 4. ⚠️遗留问题与注意事项

- 本次仅取消了自动缓存落盘对工作目录的写入；显式指定 `export_md_path` 的导出能力仍然保留。
- 若旧调用方继续传入 `workdir`，当前实现层不会再读取或使用该参数，但调用方也应尽快移除该参数，避免产生误解。
- 本次验证使用“关键字符串回扫无匹配 + AST 备注回归脚本通过”的组合方式，未额外执行更完整的端到端大结果落盘冒烟。
