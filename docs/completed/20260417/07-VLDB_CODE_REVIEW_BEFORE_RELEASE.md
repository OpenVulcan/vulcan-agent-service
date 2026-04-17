# 任务目标

对 `vldb-lancedb` 与 `vldb-sqlite` 当前待提交代码进行一次面向“提交与编译前”的工程化审核，优先识别：

1. ABI / FFI 不一致风险；
2. gRPC、库模式与发布脚本之间的不一致；
3. 多库 runtime、词典、FTS、动态库导出等最近重构区域中的编译风险与行为回归风险；
4. Docker / GitHub Actions / release 打包可能导致后续发布失败的问题。

# 执行步骤

1. 对两个仓库先做结构映射，确认核心模块与最近重构区域。
2. 查看两个仓库当前工作区状态与未提交改动，聚焦本轮实际待提交内容。
3. 审核 `ffi / include / proto / runtime / service / workflow / Dockerfile` 等关键文件。
4. 对可疑点回看相关调用链，判断是否属于真实问题、潜在回归或测试缺口。
5. 输出按严重级别排序的审核结论，并在计划文件末尾追加执行变更总结。

# 技术选型

1. 使用 CodeKit 工具做结构映射与定向检索，避免只凭文本片段下结论。
2. 审核结论只基于当前代码与配置现状，不臆测未提交的“计划中改动”。
3. 若未发现问题，也必须明确说明残余风险与测试空白。

# 验收标准

1. 给出 `vldb-lancedb` 与 `vldb-sqlite` 的代码审核结论。
2. 发现的问题需包含文件定位与影响说明。
3. 审核结论优先服务“提交前是否还需要修”的决策。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次工作以“提交与编译前审核”为目标，对 `vldb-lancedb` 与 `vldb-sqlite` 的当前待提交变更和关键代码链进行了复核。实际检查覆盖了：

- 当前未提交差异（版本号与 Docker 工作流变更）；
- 核心库代码（`runtime / engine / ffi / tokenizer / fts`）；
- 发布相关工作流（`docker-build.yml`、`build-native-release.yml`）。

结果上，两库的 `cargo check` 与 `cargo test --lib` 均已通过，主代码链没有发现新的编译级问题；当前明确需要拦截的是 Docker 工作流在改成手动触发后，版本标签策略没有同步调整的问题。

## 2. 📂文件变更清单

### 新增

- `docs/plan/20260417-07-VLDB_CODE_REVIEW_BEFORE_RELEASE.md`

### 说明

- 本次审核未对 `vldb-lancedb` 或 `vldb-sqlite` 代码做修改，仅新增审核计划与执行总结文档。

## 3. 💻关键代码调整详情

1. 对 `vldb-lancedb` 执行了：
   - `cargo check`
   - `cargo test --lib`
2. 对 `vldb-sqlite` 执行了：
   - `cargo check`
   - `cargo test --lib`
3. 检查了两个仓库的：
   - `.github/workflows/docker-build.yml`
   - `.github/workflows/build-native-release.yml`
   - `src/ffi.rs`
   - `src/runtime.rs`
   - `src/engine.rs` / `src/fts.rs` / `src/tokenizer.rs`
4. 审核结论是：代码主链可编译、可测试，但 Docker 手动触发策略与版本标签逻辑存在发布行为风险。

## 4. ⚠️遗留问题与注意事项

1. 当前两个仓库的 Docker 工作流在手动触发时，只会推送 `latest`，不会自动生成 `vX.Y.Z` 与纯版本标签；如果从 tag 手动触发，还可能误把一次版本发布覆盖成单纯的 `latest` 更新。
2. 除该问题外，本轮未发现新的代码级阻塞问题；但由于本次审核没有执行完整的跨平台 GitHub Actions 实跑，发布链仍建议在修正工作流后再做一次实际流水线验证。
