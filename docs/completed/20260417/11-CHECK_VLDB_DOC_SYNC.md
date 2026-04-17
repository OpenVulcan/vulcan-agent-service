# 任务目标

检查 `vldb-lancedb` 与 `vldb-sqlite` 两个仓库当前文档是否与最新代码状态、发布方式和库模式保持同步，重点关注：

1. README 主说明；
2. `docs/LIBRARY_USAGE.zh-CN.md`；
3. 其他 docs 入口文件；
4. 当前 release / Docker / FFI / Rust 静态与 Go FFI 相关说明是否遗漏或过时。

# 执行步骤

1. 盘点两个仓库当前文档文件。
2. 对照最近代码与工作流状态，检查 README 与库模式文档是否同步。
3. 标记不同步、遗漏或容易误导的点。
4. 输出审核结论，并在计划文件末尾补执行变更总结后归档。

# 技术选型

1. 使用目录扫描与定向文件阅读相结合的方式检查文档。
2. 本次以“审核”为主，不主动修改文档，除非用户后续要求。
3. 重点关注当前真实状态，不回写历史归档说明。

# 验收标准

1. 明确说明两个仓库文档是否同步。
2. 若存在不同步项，给出文件定位与影响说明。
3. 给出是否适合直接提交/发布的判断。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次对 `vldb-lancedb` 与 `vldb-sqlite` 的 README 与 docs 入口做了同步性审核。结论是：

- `vldb-lancedb`：当前主文档与库模式文档整体和最新代码状态保持一致，未发现新的明显脱节项；
- `vldb-sqlite`：主文档、gRPC 文档和库模式说明整体结构已对齐最新能力，但仍残留多处固定版本示例 `v0.1.1`，与当前实际版本/标签演进不一致。

## 2. 📂文件变更清单

### 修改

- `docs/plan/20260417-11-CHECK_VLDB_DOC_SYNC.md`

### 说明

- 本次只进行了审核与记录，没有直接修改两个 `vldb` 仓库的文档。

## 3. 💻关键代码调整详情

1. 对 `vldb-lancedb` 检查了：
   - `README.md`
   - `docs/LIBRARY_USAGE.zh-CN.md`
   - `docs/README.zh-CN.md`
   - `docs/README.en.md`
2. 对 `vldb-sqlite` 检查了：
   - `README.md`
   - `docs/LIBRARY_USAGE.zh-CN.md`
   - `docs/README.zh-CN.md`
   - `docs/README.en.md`
   - `docs/grpc-integration.zh-CN.md`
   - `docs/grpc-integration.en.md`
3. 发现的主要不同步项集中在 `vldb-sqlite`：
   - Docker 固定版本示例仍写成 `v0.1.1`，没有跟随当前版本迭代更新。

## 4. ⚠️遗留问题与注意事项

1. `vldb-lancedb` 当前文档没有发现新的阻塞性不同步问题。
2. `vldb-sqlite` 的旧版本示例虽然不影响功能理解，但会在提交/发布前给读者造成“推荐使用旧 tag”的误导，建议在正式发布前顺手统一更新。
