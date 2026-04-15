## 任务目标

本次任务聚焦于 `vmcp-ast` 与 `vmcp-rg` 的输出机制重构，目标是彻底移除这两个工具自身的分页/缓存调用概念，改为更直接的“内联输出 + 超限自动落盘”模式，降低 AI 理解成本并提升实际可用性。

## 执行步骤

1. 梳理 `vmcp-ast` 与 `vmcp-rg` 当前参数与返回结构，移除 `cache_id`、`page`、`truncate_chars`、`cache_ttl_sec` 等工具级缓存/分页参数。
2. 为两个工具增加可选 `workdir` 参数，并实现超限结果自动写入 `<workdir>/.vulcan/mcp/cache/`；若未提供工作目录，则回退到宿主提供的 MCP 临时目录。
3. 为两个工具增加 Markdown 导出参数，允许将完整结果写入指定 `.md` 文件。
4. 在 Rust 宿主侧增加 `vulcan.temp_dir` 暴露，路径规则为“程序目录的上级目录下的 `temp` 目录”。
5. 调整大结果返回策略：当 JSON 编码结果超过 10000 字节时，仅返回精简预览，并在结果顶层加入完整文件路径提示；当结果未超限时，保持原样输出且不附加提示。
6. 更新 `skill.json`、运行时副本与开发文档，确保新参数和新行为可被 AI 正确理解。
7. 通过本地构建与临时调用验证 `vmcp-ast` / `vmcp-rg` 的小结果、大结果、Markdown 导出三类场景。

## 技术选型

- Lua 层负责参数校验、结果编码长度判断、Markdown 渲染与超限落盘。
- Rust 宿主负责提供通用的 `vulcan.temp_dir`，避免技能脚本自行推断临时目录。
- 保留全局 MCP 缓存基础设施，但 `vmcp-ast` / `vmcp-rg` 不再依赖该机制暴露任何分页/缓存参数。

## 验收标准

1. `vmcp-ast` 与 `vmcp-rg` 的对外参数中不再出现缓存/分页相关字段。
2. 两个工具支持可选 `workdir`，且在结果超限时能把完整 JSON 输出到指定目录或宿主临时目录。
3. 两个工具支持可选 Markdown 导出路径，能成功写入 `.md` 文件。
4. 当结果超过 10000 字节时，返回内容包含完整输出文件路径提示；当结果不超过 10000 字节时，不出现提示。
5. `vulcan.temp_dir` 可在 Lua 中直接读取，且落点路径符合 `output/temp` 这一规则。
6. 文档与 `output` 运行时副本同步完成，构建与本地调试通过。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 完成 `vmcp-ast` 与 `vmcp-rg` 的输出机制重构，移除了工具级分页/缓存参数，改为“10KB 内联直返，超限自动落盘并返回精简预览”。
- 为两个工具新增 `workdir` 与 `export_md_path` 参数，支持完整 JSON 落盘与 Markdown 导出。
- 在 Rust 宿主中新增 `vulcan.temp_dir`，统一为 Lua skill 提供 MCP 临时目录。
- 修复大结果落盘目录创建对 `LuaFileSystem` 的单点依赖，改为“优先 `lfs`，缺失时回退 `vulcan.exec`”。
- 清理未发布阶段的兼容别名入口，降低 AI 后续理解成本。

### 2. 📂文件变更清单

#### 修改

- `src/lua_engine.rs`
- `runtime/lua_skills/ast-grep/main.lua`
- `runtime/lua_skills/ast-grep/main_rg.lua`
- `runtime/lua_skills/ast-grep/skill.json`
- `docs/lua_skills.md`

#### 新增

- `docs/plan/20260414-17-AST_RG_OUTPUT_AND_TEMP_EXPORT_REWORK.md`

### 3. 💻关键代码调整详情

- `src/lua_engine.rs`
  - 新增 `vulcan.temp_dir` 注入，路径规则为“可执行文件目录的上级目录/temp”。
  - 在注册 `vulcan` 模块时主动创建该临时目录，避免 Lua 端首次使用时再做路径猜测。
- `runtime/lua_skills/ast-grep/main.lua`
  - 删除 `cache_id/page/truncate_chars/cache_ttl_sec` 相关工具调用契约。
  - 增加 `workdir` 与 `export_md_path` 参数校验。
  - 新增完整结果 JSON 长度判断、超限落盘、Markdown 导出与精简预览生成逻辑。
  - 修正备注参数说明，明确 `comment` 默认关闭。
  - 目录创建逻辑增加 `vulcan.exec` 回退，避免缺少 `LuaFileSystem` 时大结果流程失效。
- `runtime/lua_skills/ast-grep/main_rg.lua`
  - 与 `vmcp-ast` 对齐，移除缓存/分页参数与兼容别名。
  - 新增大结果落盘、Markdown 导出与预览返回逻辑。
  - 目录创建逻辑同样增加 `vulcan.exec` 回退。
- `runtime/lua_skills/ast-grep/skill.json`
  - 更新 `vmcp-ast`、`vmcp-rg` 参数定义与 prompt，删除缓存/分页参数说明。
  - 补充 `workdir`、`export_md_path` 的使用语义。
- `docs/lua_skills.md`
  - 补充 `vmcp-ast` / `vmcp-rg` 的新输出规则。
  - 新增 `vulcan.temp_dir` 说明与使用示例。

### 4. ⚠️遗留问题与注意事项

- 本次已使用 `target/debug/vulcan-mcp.exe` 做本地调试验证；由于 `output/bin/vulcan-mcp.exe` 正被其他进程占用，未直接覆盖该运行中的二进制文件。
- 实际验证中已确认：
  - `vmcp-ast` 小结果场景可直接内联返回且不附加提示。
  - `vmcp-ast` / `vmcp-rg` 大结果场景会自动写入 JSON，并返回包含完整路径提示的预览结果。
  - `export_md_path` 可成功写出 Markdown 文件。
  - 未提供 `workdir` 时，会自动回落到宿主临时目录（调试模式下验证为 `target/temp/mcp/cache`）。
- 因调试模式二进制位置不同，正式运行于 `output/bin` 时，`vulcan.temp_dir` 会自然对应到 `output/temp`。
