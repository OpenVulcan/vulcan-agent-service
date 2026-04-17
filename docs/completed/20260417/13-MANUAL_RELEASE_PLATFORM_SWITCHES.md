# 任务目标

调整 `vldb-lancedb` 与 `vldb-sqlite` 的原生编译发布工作流，使手动触发时可以按平台单独关闭不需要的编译项；默认仍保持全平台开启，满足常规 tag 发布全量构建的需求。

# 执行步骤

1. 审查两个仓库当前 `build-native-release.yml` 的 `workflow_dispatch` 输入与矩阵生成逻辑。
2. 为手动触发增加显式的平台开关输入，并保证默认值为开启。
3. 调整矩阵生成逻辑，使被关闭的平台不进入构建矩阵。
4. 复查 tag / 常规发布路径，确认默认行为仍为全量构建。
5. 对两个仓库做语法与关键输出验证。
6. 在计划文件末尾补充执行变更总结并归档。

# 技术选型

1. 使用 `workflow_dispatch.inputs` 的布尔开关控制各平台是否进入矩阵。
2. 保持默认值为 `true`，确保手动触发默认仍为全量构建。
3. 保持非手动路径的默认平台集合不变，避免影响既有发布行为。

# 验收标准

1. `vldb-lancedb` 与 `vldb-sqlite` 的原生发布工作流均支持手动关闭指定平台。
2. 手动触发时可只编译单个平台（例如仅 `macos-x64`）。
3. 默认配置下仍会编译全部支持平台。
4. 工作流 YAML 语法与矩阵生成逻辑正确。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次调整为 `vldb-lancedb` 与 `vldb-sqlite` 的原生发布工作流新增了“按平台开关构建”的手动触发能力。手动运行时现在可以显式关闭不需要的平台，仅编译指定目标；而 tag 推送路径仍保持全平台默认开启，不会改变原有正式发布的全量构建行为。

## 2. 📂 文件变更清单

### 修改

- `D:\\projects\\VulcanLocalDataGateway\\vldb-lancedb\\.github\\workflows\\build-native-release.yml`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\.github\\workflows\\build-native-release.yml`

### 新增

- 无

### 删除

- 无

## 3. 💻 关键代码调整详情

1. 为两个工作流的 `workflow_dispatch` 新增了五个布尔开关：
   - `build_linux_x64`
   - `build_linux_arm`
   - `build_mac_arm`
   - `build_mac_x64`
   - `build_win_x64`
2. 新增 `prepare-matrix` 预处理作业，按触发来源与平台开关动态生成最终构建矩阵。
3. `build` 作业改为依赖 `prepare-matrix` 输出的 JSON 矩阵，避免平台关闭后仍进入无效构建。
4. 保持非手动路径默认行为不变：只要不是 `workflow_dispatch`，矩阵会自动包含全部目标平台。
5. 增加“至少启用一个平台”的保护，防止手动全部关闭后工作流进入空矩阵状态。

## 4. ⚠️ 遗留问题与注意事项

1. 本次做的是矩阵生成逻辑调整，尚未实际触发 GitHub Actions 去跑“只编译单平台”的真实回归，后续建议至少实跑一次 `macOS x64 only` 场景确认 UI 与 runner 选择都符合预期。
2. 当前校验以差异检查与 `git diff --check` 为主，未额外引入 YAML 解析器做静态语法验证。
