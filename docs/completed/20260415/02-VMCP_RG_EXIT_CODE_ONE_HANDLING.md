# 任务计划：修复 vmcp-rg 对 ripgrep 退出码 1 的误判

## 任务目标

修复 `vmcp-rg` 对 `ripgrep` 退出码 `1` 的处理方式。当前 `rg` 在“无匹配”场景下会返回退出码 `1`，但 `vmcp-rg` 会把它包装成执行失败。目标是将该场景改为正常返回“0 命中结果”，仅在真正的执行异常场景下返回 `rg_exec_failed`。

## 执行步骤

1. 定位 `vmcp-rg` 对 `vulcan.exec` 与 `ripgrep` 返回值的处理逻辑。
2. 确认宿主侧 `vulcan.exec` 返回结构中退出码、错误文本与 stdout/stderr 的语义。
3. 修改 `vmcp-rg` 的 `run_rg_command`，将 `exit code 1` 识别为“无匹配”并正常返回空结果。
4. 如有必要，补充对其它非零退出码的错误信息，避免误判。
5. 执行最小验证，确认：
   - 无匹配时不再报 `rg_exec_failed`
   - 有匹配时保持原有行为
   - 真正的执行失败仍能返回结构化错误
6. 补充执行变更总结并归档到 `docs/completed/20260415/`。

## 技术选型

- 优先在 `vmcp-rg` 自身的执行包装层修正，不改动 `ripgrep` 或宿主执行模型。
- 保持 `parse_rg_json_output` 与后续结构映射逻辑不变，尽量缩小改动范围。
- 无匹配场景返回空 stdout/stderr 或空命中集合，由后续主流程统一生成 `files_scanned = 0` 的正常结果。

## 验收标准

- `vmcp-rg` 在无匹配场景下返回正常空结果，而不是 `rg_exec_failed`。
- `vmcp-rg` 在有匹配场景下继续正常返回结构化命中结果。
- 真正执行失败或超时场景仍保留结构化错误返回。

## 验证结果

1. 已确认宿主 `vulcan.exec` 在进程非零退出时会返回：
   - `code`
   - `success`
   - `error`
   其中 `ripgrep` 无匹配场景会表现为 `code = 1` 且附带错误文本。
2. 已在 `runtime/lua_skills/ast-grep/main_rg.lua` 的 `run_rg_command` 中增加显式分流：
   - `code == 1` 且未超时时，按“无匹配”处理，返回空 stdout/stderr 与空错误对象
   - 其它 `result.error` 继续视为真实执行失败
3. 已通过 `--call-tools` 模式完成最小验证：
   - 搜索 `__definitely_no_match_pattern__` 时，返回正常空结果：
     - `files = []`
     - `files_scanned = 0`
     - `rg_matches = 0`
   - 搜索 `build_server\\(` 时，继续正常返回 `src/main.rs` 的结构化命中结果
4. 验证期间生成的临时配置、联接目录与依赖产物已清理完成。

## 执行变更总结

### 1. 核心修复与调整概述

本次修复解决了 `vmcp-rg` 将 `ripgrep` 退出码 `1` 误判为执行失败的问题。现在“无匹配”会被当作正常业务结果返回，而不是再向上抛出 `rg_exec_failed`。这使得 `vmcp-rg` 的行为与 `ripgrep` 本身的语义保持一致，也避免了代理在做宽泛检索时被误导为工具异常。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main_rg.lua`
- 新增：`docs/plan/20260415-02-VMCP_RG_EXIT_CODE_ONE_HANDLING.md`
- 后续归档目标：`docs/completed/20260415/02-VMCP_RG_EXIT_CODE_ONE_HANDLING.md`

### 3. 💻关键代码调整详情

- 在 `run_rg_command` 中增加对 `result.code` 的判断：
  - 若 `tonumber(result.code) == 1` 且 `result.timed_out ~= true`，直接返回空结果，不构造错误对象
  - 保留原有 `result.timed_out` 和 `result.error` 的错误路径处理
- 增加双语注释，明确说明 `ripgrep` 退出码 `1` 表示“无匹配”，不是执行失败

### 4. ⚠️遗留问题与注意事项

- 当前修复只作用于 `vmcp-rg` 的 `ripgrep` 包装层，不影响 `vmcp-ast` 或其它 tool 的进程退出码处理逻辑。
- 若未来还接入其它命令行工具，建议按各自原生命令退出码语义做同类分流，避免再次把“无结果”误判为“执行失败”。
