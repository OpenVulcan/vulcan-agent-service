# 任务计划：为 vmcp-rg 增加完整函数显示开关

## 任务目标

调整 `vmcp-rg` 工具的输出行为，新增一个控制“是否展开完整函数源码”的参数。默认关闭，仅显示命中行与结构上下文；显式开启时，才展示命中行所属函数的完整源码片段。同时补充对外说明，明确：

- 全盘分析或功能检索场景不建议开启
- 精确文件分析场景建议开启

## 执行步骤

1. 定位 `vmcp-rg` 的参数定义、参数校验、命中标记逻辑与树渲染逻辑。
2. 为 `vmcp-rg` 增加新的布尔参数，并实现默认值与校验规则。
3. 修改命中后的源码展开逻辑，使其仅在参数开启时展开完整函数源码，否则保留命中行与结构树。
4. 更新 `skill.json`、相关文档和提示词说明，明确该参数的使用建议。
5. 执行最小验证，确认默认关闭时不再展开完整函数源码，开启后恢复完整函数显示。
6. 对照计划补充执行变更总结，并归档到 `docs/completed/20260415/`。

## 技术选型

- 参数采用布尔类型，默认关闭，以降低全盘分析时的输出体积。
- 仅在 `vmcp-rg` 渲染链路中收敛行为，不修改 `vmcp-ast` 或其它工具逻辑。
- 保持现有树结构输出格式，仅调节函数源码展开条件。

## 验收标准

- `vmcp-rg` 新参数默认值为关闭。
- 默认关闭时，只显示命中行与结构节点，不展开完整函数源码。
- 显式开启时，展示命中行对应函数的完整源码片段。
- 工具定义和文档中明确给出使用建议：全盘分析/功能检索不建议开启，精确文件分析建议开启。

## 验证结果

1. 已在 `runtime/lua_skills/ast-grep/main_rg.lua` 中新增 `show_full_function` 布尔参数校验逻辑，默认值为 `false`。
2. 已修改命中标记逻辑：函数/方法命中时，仅在 `show_full_function=true` 时展开完整源码；默认仅保留命中行与结构树。
3. 已同步更新 `runtime/lua_skills/ast-grep/skill.json` 与 `docs/lua_skills.md`，补充参数定义与推荐使用场景。
4. 已通过仓库内置 `--call-tools` 模式完成最小验证：
   - 默认调用 `vmcp-rg` 时，仅输出 `L70`、`L131` 这类命中行与结构上下文，不展开函数全文。
   - 传入 `show_full_function=true` 后，成功输出 `async_main` 与 `build_server` 的完整源码片段。
5. 验证过程中生成的临时配置、联接目录和依赖产物已清理完成。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作为 `vmcp-rg` 增加了“完整函数显示开关”能力。默认行为从“函数命中即展开整段源码”调整为“仅显示命中行与结构上下文”，以降低全盘分析和功能检索时的输出膨胀；当调用方显式开启 `show_full_function=true` 时，工具才会渲染命中函数的完整源码片段，更适合精确文件分析和函数级审阅场景。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main_rg.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`
- 新增：`docs/plan/20260415-01-VMCP_RG_FULL_FUNCTION_TOGGLE.md`
- 后续归档目标：`docs/completed/20260415/01-VMCP_RG_FULL_FUNCTION_TOGGLE.md`

### 3. 💻关键代码调整详情

- 在 `main_rg.lua` 中新增 `validate_show_full_function_argument`，负责：
  - 未传参时返回默认值 `false`
  - 非布尔值输入时返回结构化错误
- 调整 `annotate_tree_with_rg_hits`：
  - 传入新的 `show_full_function` 参数
  - 函数/方法命中时，只有在开关开启后才调用 `mark_symbol_expand_source`
  - 默认路径改为为命中结构追加 `Lx | text` 单行命中信息
- 更新 `skill.json`：
  - 为 `vmcp-rg` 增加 `show_full_function` 参数定义
  - 在 prompt 中明确默认关闭，且给出“全盘分析/功能检索不建议开启，精确文件分析建议开启”的使用建议
- 更新 `docs/lua_skills.md` 的输出规范说明，确保文档与实际行为保持一致

### 4. ⚠️遗留问题与注意事项

- 当前行为只影响 `vmcp-rg`，不会改变 `vmcp-ast` 或 `vmcp-patch` 的输出模式。
- 由于 `show_full_function=true` 会显著增加 `files[].content` 的体积，在跨目录搜索或高命中场景下应谨慎使用。

## 追加执行变更总结：ext 语义修正与语言名归一化

### 1. 核心修复与调整概述

在完成 `show_full_function` 开关后，进一步修正了 `ext` 参数的语义问题。现在工具文案会明确提示 `ext` 本质上是“文件扩展名过滤”，同时在实现层新增“完整语言名 -> 对应扩展名集合”的自动归一化逻辑，例如传入 `rust` 会自动转换为 `rs`。这样既能降低模型把 `ext` 误解成“语言枚举但不生效”的概率，也保留了 `rs`、`ts`、`js` 这类精确扩展名输入的原有行为。

### 2. 📂文件变更清单

- 修改：`runtime/lua_skills/ast-grep/main.lua`
- 修改：`runtime/lua_skills/ast-grep/skill.json`
- 修改：`docs/lua_skills.md`

### 3. 💻关键代码调整详情

- 在 `main.lua` 的共享 `validate_extension_argument` 中新增归一化逻辑：
  - 若传入值是 `rust`、`typescript`、`python` 这类完整语言名，则自动展开为该语言在注册表中的扩展名集合
  - 若传入值本身就是 `rs`、`ts`、`js` 这类扩展名，则按精确扩展名处理，不做范围放大
- 更新 `skill.json` 中 `vmcp-ast`、`vmcp-rg` 的 `ext` 参数说明与 prompt，明确：
  - `ext` 是文件扩展名过滤
  - 允许传入常见语言名并自动归一化
- 更新 `docs/lua_skills.md`，补充“扩展名过滤”和“语言名自动归一化”的约定

### 4. ✅额外验证结果

- 通过 `--call-tools` 模式验证：
  - `vmcp-ast` 在 `ext: "rust"` 下可正常扫描 `.rs` 文件
  - `vmcp-rg` 在 `ext: "rust"` 下可正常命中 `src/main.rs` 中的 `build_server(`

### 5. ⚠️补充注意事项

- 当前自动扩展只针对“明确语言名”生效；短扩展名仍保持精确匹配，这是为了避免用户输入 `js` 时被自动放大到 `jsx/mjs/cjs`。
