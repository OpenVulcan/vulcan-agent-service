# 任务目标

对 `vulcan-mcp-client` 当前代码进行连续深度审核，并在发现问题后立即修复与验证；重复执行“审核 -> 修复 -> 验证”循环，直到连续两轮审核均未发现新的明确问题后停止，并输出累计修复内容总结。

# 执行步骤

1. 聚焦当前近期改动主链，优先审核 `space_controller`、`luaskills_host`、配置装配、运行时根路径解析与宿主接线逻辑。
2. 每一轮审核只记录明确、可落地、可复现的问题，不记录纯风格意见或不确定风险。
3. 对每个明确问题立即修复，并补充或更新回归测试。
4. 每轮修复后执行格式化与测试，确保现有能力无回退。
5. 修复完成后立即开始下一轮审核。
6. 当连续两轮审核都没有发现新的明确问题时，停止循环并整理累计修复总结。

# 技术方案

1. 优先通过本地源码静态审查与现有测试定位问题。
2. 对路径解析、配置装配、运行时行为等高风险逻辑，优先补充单元测试覆盖。
3. 对涉及宿主启动、回退路径、控制器接入的逻辑，尽量将错误前移到配置构建阶段。
4. 所有代码修改使用最小必要改动，避免引入不必要的结构性重写。

# 验收标准

1. 至少完成两轮连续“无新问题”审核。
2. 所有本轮修复项均有对应代码改动与必要测试。
3. `cargo fmt --all` 与 `cargo test` 保持通过。
4. 最终输出需明确说明累计修复内容与停止条件已满足。

---

# 执行变更总结

## 1. 核心修复与调整概述

本轮连续审核共完成 5 类核心修复，并在后续两轮确认性审查中未再发现新的明确问题：

1. 修复 `space_controller.executable_path` 显式配置路径缺少存在性校验的问题，将错误前移到宿主选项构建阶段。
2. 修复 `space_controller.executable_path` 相对路径依赖当前工作目录的问题，改为稳定相对 `runtime_root` 解析。
3. 修复回退复制路径只检查 `exists()` 的问题，目录同名时现在会在构建阶段直接报错。
4. 修复显式 `runtime_root` 的相对路径漂移与坏路径前置校验问题，并让其相对配置基准目录稳定归一化。
5. 修复 `temp` 目录与 `skill_roots/skills_override` 没有真正遵循统一 `runtime_root` / 配置基准目录的问题，统一了运行时路径锚点语义。

## 2. 📂文件变更清单

### 修改文件

- `docs/plan/20260421-02-CONTINUOUS_CODE_AUDIT_AND_FIX.md`
- `src/config.rs`
- `src/luaskills_host.rs`
- `src/main.rs`
- `src/temp_maintenance.rs`

## 3. 💻关键代码调整详情

### `src/luaskills_host.rs`

- 将 `resolve_space_controller_executable_path()` 改为返回 `Result<Option<PathBuf>, String>`。
- 对显式 `space_controller.executable_path` 增加：
  - 相对 `runtime_root` 的稳定解析
  - `exists()` 校验
  - `is_file()` 校验
- 对回退复制路径 `runtime_root/bin/tools/vldb-controller(.exe)` 增加同样的 `is_file()` 校验。
- 将 `resolve_space_controller_options()` 改为返回 `Result`，把配置错误前移到 `build_luaskills_engine_options()` 阶段。
- 新增 `resolve_config_base_dir()`，统一配置文件相对路径的基准目录解析逻辑。
- 修复 `resolve_runtime_root_from_config()`：
  - 相对 `runtime_root` 现在相对配置基准目录解析
  - 显式 `runtime_root` 若不存在或不是目录则直接拒绝
- 修复 `resolve_skill_roots_from_config()`：
  - `skill_roots`
  - `skills_override`
  现在都会相对配置基准目录稳定解析，不再依赖当前工作目录。
- 补充 5 条回归测试，覆盖：
  - controller 显式路径缺失
  - controller 回退路径为目录
  - controller 显式相对路径解析
  - `runtime_root` 相对路径解析
  - `skill_roots` 相对路径解析

### `src/config.rs`

- 给 `Config` 新增 `loaded_config_path`（`serde(skip)`），用于在配置加载后保留配置文件来源路径，支撑后续相对路径稳定归一化。
- 在 `Config::from_file()` 中注入 `loaded_config_path`。

### `src/temp_maintenance.rs`

- 新增 `CONFIGURED_RUNTIME_ROOT` 全局运行根记录。
- 新增 `initialize_runtime_temp_root()`，允许主流程在读取配置后显式注册统一运行根。
- 将 `resolve_runtime_temp_dir()` 改为优先使用显式运行根派生 `temp` 目录，而不是始终基于可执行文件目录。
- 新增测试验证显式运行根能够正确接管 `temp` 根目录位置。

### `src/main.rs`

- 在 `Serve`、`--call-tools`、`--internal-luaexec-request` 三条主路径中，在执行 temp 清理前先初始化 `runtime_root` 到 temp 维护模块。
- 调整本地调试模式和内部 luaexec 模式的启动顺序，确保 `temp` 路径能真正跟随配置中的统一运行根。

## 4. ⚠️遗留问题与注意事项

- 本轮连续审核在最终两轮确认性审查中均未发现新的明确问题，当前停止条件已满足。
- `cargo fmt --all` 与 `cargo test` 已保持通过，当前测试结果为 `26/26`。
