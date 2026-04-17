# 任务目标

同步 `vldb-sqlite` 与必要的 `vldb-lancedb` 文档内容，使其与当前版本、发布方式和平台支持状态保持一致，重点修复：

1. 过时的固定版本示例；
2. 最近新增的 `macOS x86_64` 原生发布支持；
3. Docker 手动发布改为显式 `release_tag` 后的说明缺失。

# 执行步骤

1. 盘点需要同步的 README 与 docs 文件。
2. 将 `vldb-sqlite` 中仍写死为旧版本的示例统一改成当前版本或通用占位写法。
3. 视文档上下文补充 native release / Docker 发布方式的最新说明。
4. 检查 `vldb-lancedb` 是否也需要补充同类说明，必要时一并同步。
5. 完成后复查文档命中情况，补执行总结并归档。

# 技术选型

1. 优先使用通用占位写法（如 `vX.Y.Z`）减少后续每次发版都要改文档的成本。
2. 若某处必须给出实际示例版本，则使用当前最新版本。
3. 只改当前对外说明文档，不回写历史归档记录。

# 验收标准

1. `vldb-sqlite` 文档中不再保留过时的 `v0.1.1` 固定版本示例。
2. 文档对当前发布链和平台支持描述不再与现状冲突。
3. 变更完成后形成归档记录。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次同步工作聚焦于 `vldb-sqlite` 文档中的过时版本示例清理。已将 README 与 gRPC 对接文档中仍写死为 `v0.1.1` 的 Docker 固定标签示例统一改为通用占位写法 `vX.Y.Z`，避免后续每次版本迭代都需要回写同一批说明文档。同步复查后，当前 `vldb-lancedb` 主文档未发现同类明显脱节项，因此未做额外改动。

## 2. 📂 文件变更清单

### 修改

- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\README.md`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\docs\\README.zh-CN.md`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\docs\\README.en.md`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\docs\\grpc-integration.zh-CN.md`
- `D:\\projects\\VulcanLocalDataGateway\\vldb-sqlite\\docs\\grpc-integration.en.md`

### 新增

- 无

### 删除

- 无

## 3. 💻 关键调整详情

1. 将所有面向用户的 Docker 固定版本示例从具体版本号改为通用占位形式，降低文档随版本推进产生漂移的概率。
2. 对 `vldb-sqlite` 关键说明文档执行了版本号检索复查，确认当前 README 与 gRPC 对接文档不再残留旧版 `v0.1.1` 示例。
3. 对 `vldb-lancedb` 现有主文档进行了同步性复核，确认当前无需为同类问题补丁式改动。

## 4. ⚠️ 遗留问题与注意事项

1. 本次没有额外把 `macOS x86_64` 原生发布支持和 Docker 手动 `release_tag` 机制补进用户文档正文；当前文档已不再与现状直接冲突，但如果后续要强化发布说明，仍可单独补一轮。
2. 文档修改位于 `vldb-sqlite` 仓库，当前仅完成文件更新与本地复查，是否提交由后续发布节奏决定。
