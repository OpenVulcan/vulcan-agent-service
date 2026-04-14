# 任务计划：重构 `codeview_ast` 的路径与扩展名过滤参数

## 任务目标

将 `codeview_ast` 当前的三参数模式：

- `dir`
- `recursive`
- `lang`

调整为新的三参数模式：

- `path`
- `recursive`
- `ext`

其中：

1. `path` 既可以是目录，也可以是单文件；
2. 当 `path` 是单文件时：
   - 直接进入单文件模式；
   - `recursive` 无意义；
   - `ext` 无意义；
3. 当 `path` 是目录时：
   - `recursive` 控制是否扫描子目录；
   - `ext` 用于扩展名过滤；
   - 需要支持多扩展名形式，如 `js,ts,rs`；
4. 本次不实现缓存与增量对比逻辑，保持返回结果尽量直接、低噪音；
5. 保证现有结构输出格式不退化，仍然返回 `file / language / content`。

## 执行步骤

1. 审查 `runtime/lua_skills/codeview_ast/main.lua` 当前参数校验、文件收集与语言识别逻辑。
2. 修改技能参数模型与入口校验：
   - 删除 `dir` / `lang` 主路径；
   - 增加 `path` / `ext`；
   - 明确区分单文件模式与目录模式。
3. 实现扩展名过滤解析逻辑：
   - 支持逗号分隔字符串；
   - 统一去空白、去前导点并转小写；
   - 仅在目录扫描模式下生效。
4. 调整文件收集逻辑：
   - 若 `path` 指向文件，直接收集该文件；
   - 若 `path` 指向目录，按 `recursive + ext` 过滤；
   - 顶层返回字段从 `directory` 调整为更通用的 `path`。
5. 同步更新 `runtime/lua_skills/codeview_ast/skill.json` 参数描述。
6. 执行构建与真实 MCP 回归，覆盖：
   - 单文件模式；
   - 目录 + 非递归；
   - 目录 + 多扩展名过滤；
   - 单文件模式下忽略 `recursive` 与 `ext`。
7. 对照计划补充执行总结并迁移到 `docs/completed/20260413/`。

## 技术选型

- 路径模式判断直接使用宿主提供的 `vulcan.fs_is_dir` 与 `vulcan.fs_exists`。
- 扩展名过滤采用字符串集合匹配，不额外引入语言键过滤层，减少噪音与歧义。
- 单文件模式优先按文件扩展名自动识别语言，不再额外依赖调用方传入语言类型。

## 验收标准

- `codeview_ast` 支持 `path + recursive + ext` 新接口。
- `path` 为文件时能直接返回单文件结构结果。
- `path` 为目录时支持多扩展名过滤，如 `js,ts,rs`。
- 单文件模式下 `recursive` 与 `ext` 不影响结果。
- 构建通过，真实 MCP 回归通过。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 将 `codeview_ast` 的对外调用参数从 `dir / recursive / lang` 重构为 `path / recursive / ext`。
- `path` 现在支持自动识别两种模式：
  - 单文件模式：直接扫描单文件，忽略 `recursive` 与 `ext`；
  - 目录模式：按 `recursive` 与 `ext` 过滤目录内文件。
- 移除了原先语言覆盖参数主路径，目录扫描改为按扩展名过滤，更贴近“单文件更新 + 指定扩展筛选”的实际使用方式。
- 顶层返回字段从 `directory` 调整为 `path`，与新接口保持一致。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`docs/plan/20260413-10-CODEVIEW_AST_PATH_AND_EXTENSION_FILTER_REFACTOR.md`

### 3. 💻关键代码调整详情

- `main.lua`
  - 删除技能入口对 `lang` 的依赖，新增 `validate_path_argument` 与 `validate_extension_argument`；
  - `ext` 支持：
    - 逗号分隔字符串，如 `js,ts,rs`
    - 字符串数组（兼容实现，虽然当前 skill 描述以字符串为主）
  - 新增扩展名工具函数：
    - `extract_extension`
    - `matches_extension_filter`
    - `build_file_item`
  - `collect_files` 改为统一处理文件模式与目录模式：
    - 文件模式直接收集单文件；
    - 目录模式递归遍历并按扩展名过滤；
    - 若 `path` 不存在，返回结构化错误；
    - 若单文件扩展名不受支持，返回结构化错误。
- `skill.json`
  - 参数描述切换为：
    - `path`
    - `recursive`
    - `ext`
  - `ext` 描述明确为目录模式下使用的逗号分隔扩展名过滤。

### 4. ⚠️遗留问题与注意事项

- 当前扩展名过滤是“精确扩展名匹配”，例如：
  - `ts` 不会自动包含 `tsx`
  - `js` 不会自动包含 `jsx`
  这是有意保持行为直接、低歧义；如果后续需要“家族扩展名”联动，可以再单独扩展。
- 本次真实 MCP 回归已验证：
  - 单文件模式：`path = "src/config.rs"`, `recursive = true`, `ext = "js,ts"` 仍返回该 Rust 文件，证明单文件模式忽略 `recursive/ext`
  - 目录模式：`path = "src"`, `recursive = false`, `ext = "rs"` 返回 Rust 文件集合
  - 多扩展目录模式：`path = "."`, `recursive = false`, `ext = "rs,sh"` 返回 `build.rs` 与 `make.sh`
