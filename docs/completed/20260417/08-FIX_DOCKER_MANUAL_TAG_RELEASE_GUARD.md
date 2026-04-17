# 任务目标

修复 `vldb-lancedb` 与 `vldb-sqlite` 的 Docker 手动发布工作流，避免手动运行时把任意 ref 的镜像覆盖到 `latest`，并将发布入口收敛为“显式输入 tag”模式，阻止普通分支直接参与正式 Docker 分发。

# 执行步骤

1. 检查两个仓库当前 `docker-build.yml` 的 `workflow_dispatch` 与 tag 生成逻辑。
2. 将手动发布入口调整为必须显式输入 `release_tag`，并在工作流内校验必须符合 `v*` 版本标签格式。
3. 调整 `checkout` 行为，使实际构建基于输入 tag，而不是 GitHub UI 中任意选中的分支/提交。
4. 调整 Docker tags 生成逻辑，确保只有显式 release tag 才会生成：
   - `latest`
   - `vX.Y.Z`
   - `X.Y.Z`
5. 检查 YAML 语法与逻辑一致性，补执行总结并归档。

# 技术选型

1. 不依赖 GitHub UI 对 `workflow_dispatch` ref 选择的限制，因为该能力本身无法只保留 tag。
2. 使用 `release_tag` 输入作为唯一正式发布依据。
3. 通过工作流内显式校验和 `actions/checkout` 的 `ref` 锁定，确保即使用户从分支界面手动触发，实际构建也只会使用指定 tag。

# 验收标准

1. 两个仓库的 Docker 工作流都要求手动输入 release tag。
2. 未提供合法 `v*` tag 时，工作流会明确失败。
3. 手动运行不再对任意分支默认推送 `latest`。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次修复的重点是把 `vldb-lancedb` 与 `vldb-sqlite` 的 Docker 手动发布工作流从“手动点一次就可能覆盖 latest”的不安全状态，收敛成“必须显式输入 release tag，并且实际 checkout 该 tag”的发布模式。

修复完成后：

1. 两个工作流都新增了 `release_tag` 必填输入；
2. 工作流会显式校验该输入必须以 `v` 开头；
3. `actions/checkout` 不再跟随 GitHub UI 当前选择的任意分支，而是固定 checkout 到 `refs/tags/<release_tag>`；
4. Docker 标签只根据 `release_tag` 生成：
   - `latest`
   - `vX.Y.Z`
   - `X.Y.Z`

## 2. 📂文件变更清单

### 修改

- `D:/projects/VulcanLocalDataGateway/vldb-lancedb/.github/workflows/docker-build.yml`
- `D:/projects/VulcanLocalDataGateway/vldb-sqlite/.github/workflows/docker-build.yml`
- `docs/plan/20260417-08-FIX_DOCKER_MANUAL_TAG_RELEASE_GUARD.md`

## 3. 💻关键代码调整详情

1. 在两个 Docker 工作流里新增：
   - `workflow_dispatch.inputs.release_tag`
2. 将 `Checkout repository` 步骤改为：
   - `ref: refs/tags/${{ inputs.release_tag }}`
3. 在平台准备步骤和标签生成步骤中新增 `release_tag` 的 `v*` 校验。
4. 删除原先依赖 `github.event_name / github.ref_name` 的 Docker 标签生成逻辑，改为完全基于 `release_tag` 生成正式标签。

## 4. ⚠️遗留问题与注意事项

1. GitHub Actions 的 `workflow_dispatch` 界面本身**不能真正禁止用户从分支界面点击运行**；本次修复的做法是：即使从分支界面触发，工作流也只会 checkout 到输入的 tag，因此普通分支不会被当成正式发布源。
2. 当前默认 `release_tag` 值分别写成了：
   - `vldb-lancedb`: `v0.1.4`
   - `vldb-sqlite`: `v0.1.3`
   后续每次发新版时，建议同步更新默认值，避免 UI 误导。
