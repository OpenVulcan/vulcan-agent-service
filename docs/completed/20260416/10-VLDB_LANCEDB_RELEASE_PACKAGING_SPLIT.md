# 任务目标

调整 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 的 Docker 与 GitHub 原生发布流程，修复当前仅发布二进制而未发布库模式产物的问题，并将库模式产物拆分为与常规二进制独立的单独发布包。

# 执行步骤

1. 梳理当前发布与 Docker 构建逻辑。
   - 检查 `Dockerfile`
   - 检查 `.github/workflows/build-native-release.yml`
   - 明确当前二进制包命名与打包行为

2. 调整 Docker 构建逻辑。
   - 将服务镜像构建收窄为仅编译 `vldb-lancedb` 二进制
   - 保持镜像运行逻辑不变

3. 调整原生发布工作流。
   - 保持现有常规二进制包命名不变
   - 为每个平台额外生成一个独立的库模式发布包
   - 库模式发布包包含动态库、头文件及库接入文档

4. 回归验证。
   - 检查 workflow 中二进制包与库包的命名是否清晰且互不混淆
   - 确认不同平台的动态库文件名映射正确
   - 确认头文件和文档被纳入库模式包

5. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 常规二进制包继续沿用当前命名规范，不破坏既有下载入口
- 库模式包单独命名，例如 `vldb-lancedb-lib-v<version>-<target>`
- 库模式包中只包含接入库模式所需产物，不混入服务二进制
- Docker 镜像只服务 gRPC 二进制场景，不承载库模式分发职责

# 验收标准

1. Dockerfile 改为只编译服务二进制。
2. GitHub 原生发布流程对每个平台产出两个独立包：常规二进制包、库模式包。
3. 库模式包包含动态库、头文件和库模式文档。
4. 常规包命名保持现状，不影响已有消费方。
5. workflow 配置逻辑清晰，可按平台正确选择对应动态库文件名。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 调整了 `vldb-lancedb` 的 Docker 构建逻辑，使服务镜像只编译 `vldb-lancedb` 二进制，不再把 `cdylib/rlib` 一起拉进服务镜像构建链。
- 调整了 GitHub 原生发布流程，在同一平台的一次构建结果上同时导出两类独立发布包：
  - 保持现有命名的常规二进制包
  - 新增独立命名的库模式包
- 库模式包现在会包含动态库、头文件与库模式说明文档，不再与常规服务二进制混装。

## 2. 📂文件变更清单

### 修改

- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\Dockerfile`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\.github\workflows\build-native-release.yml`

## 3. 💻关键代码调整详情

- `Dockerfile`
  - 将构建命令从 `cargo build --locked --release` 收窄为 `cargo build --locked --release --bin vldb-lancedb`
  - 明确 Docker 只服务 gRPC 二进制镜像场景
- `.github/workflows/build-native-release.yml`
  - 将构建阶段改为 `cargo build --locked --release --target ... --bin vldb-lancedb --lib`
  - 保持常规包名称不变：`vldb-lancedb-v<version>-<target>`
  - 新增库模式包名称：`vldb-lancedb-lib-v<version>-<target>`
  - Windows 平台库包纳入：
    - `vldb_lancedb.dll`
    - `vldb_lancedb.dll.lib`（若存在）
    - `include/vldb_lancedb.h`
    - `docs/LIBRARY_USAGE.zh-CN.md`
  - Unix 平台库包纳入：
    - `libvldb_lancedb.so` 或 `libvldb_lancedb.dylib`
    - `include/vldb_lancedb.h`
    - `docs/LIBRARY_USAGE.zh-CN.md`
  - 常规服务包仍只保留服务二进制与原有运行资料

## 4. ⚠️遗留问题与注意事项

- 本次只调整了发布与 Docker 打包逻辑，没有实际跑 GitHub Actions；真正的跨平台产物验证仍需要后续在 CI 中跑一轮。
- 当前库模式包附带的是中文说明文档；如果未来要面向更广泛的英文调用方，建议再追加英文版 library/ffi 文档并一起打包。
- 这次没有改 Docker 镜像运行时行为，镜像仍然只面向 gRPC 服务，不承担库模式分发职责。
