# 任务目标

为 `vldb-lancedb` 与 `vldb-sqlite` 的原生发布链补充 `macOS x86_64` 平台支持，确保 GitHub Actions 能够产出 `x86_64-apple-darwin` 的二进制包与库包，并与主仓库现有的宿主依赖下载规则保持一致。

# 执行步骤

1. 检查两个仓库当前 `build-native-release.yml` 的平台矩阵，确认是否缺少 `x86_64-apple-darwin`。
2. 检查 `vulcan-mcp-client` 的宿主依赖安装脚本是否已经支持 `x86_64-apple-darwin` 资产命名。
3. 为两个仓库的 native release workflow 增加 `mac_x64` 手动输入与 `x86_64-apple-darwin` 构建矩阵。
4. 调整 macOS 构建参数，确保 `CMAKE_OSX_ARCHITECTURES` 在 Intel 平台上显式使用 `x86_64`。
5. 复核差异与链路一致性，并补充执行变更总结后归档。

# 技术选型

1. `macOS x64` 采用单独的 workflow input：`mac_x64`。
2. 默认 runner 使用 GitHub Hosted 的 `macos-13`，避免和 ARM 构建 runner 混淆。
3. 不改宿主下载脚本逻辑；若脚本已支持 `x86_64-apple-darwin`，则只补 release workflow。

# 验收标准

1. `vldb-lancedb` 与 `vldb-sqlite` 的 native release workflow 都包含 `x86_64-apple-darwin`。
2. 两个 workflow 都存在 `mac_x64` 手动输入。
3. `vulcan-mcp-client` 的宿主安装脚本与新增平台资产命名保持一致。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次修复补齐了两个 `vldb` 仓库在 GitHub 原生发布流程中缺失的 `macOS x86_64` 构建矩阵。此前主仓库宿主依赖安装脚本已经支持：

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

但两个库的 `build-native-release.yml` 只发布 ARM 版 macOS 产物，导致 Intel macOS 用户即使主仓库脚本支持，也拿不到对应 release asset。现在两个库的 native release workflow 都已经增加了：

- `mac_x64` 手动输入
- `x86_64-apple-darwin` 构建目标
- 对应的 `CMAKE_OSX_ARCHITECTURES=x86_64`

## 2. 📂文件变更清单

### 修改

- `D:/projects/VulcanLocalDataGateway/vldb-lancedb/.github/workflows/build-native-release.yml`
- `D:/projects/VulcanLocalDataGateway/vldb-sqlite/.github/workflows/build-native-release.yml`
- `docs/plan/20260417-09-ADD_MACOS_X64_NATIVE_RELEASE.md`

## 3. 💻关键代码调整详情

1. 为两个 workflow 新增：
   - `workflow_dispatch.inputs.mac_x64`
2. 为两个 workflow 的构建矩阵新增：
   - `target: x86_64-apple-darwin`
   - `runner: ${{ inputs.mac_x64 || 'macos-13' }}`
3. 将：
   - `CMAKE_OSX_ARCHITECTURES`
   从只处理 `arm64` 的表达式，扩展为同时支持：
   - `arm64`
   - `x86_64`
4. 复核了主仓库：
   - `scripts/install_host_deps.ps1`
   - `scripts/install_host_deps.sh`
   确认它们原本就支持 `x86_64-apple-darwin` 资产命名，因此无需同步修改。

## 4. ⚠️遗留问题与注意事项

1. 本次只补了 release workflow 矩阵，没有实际触发 GitHub Actions 去构建 `x86_64-apple-darwin`，因此仍建议后续在 CI 中实跑一轮确认。
2. `docker-build.yml` 本身只做 Linux 多架构镜像发布，不涉及 macOS 镜像，因此这次没有调整。
