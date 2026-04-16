# 任务目标

在 `D:\projects\vulcan-mcp-client` 中完善一键依赖安装与下载逻辑，使其能够识别 `OpenVulcan/vldb-lancedb` 的最新 GitHub Release，并根据当前操作系统/架构自动下载对应平台的库模式发布包，接入现有依赖安装流程。

# 执行步骤

1. 梳理当前依赖安装链路。
   - 检查现有 Lua/工具依赖安装脚本
   - 检查 GitHub Release 下载规则与平台匹配逻辑
   - 确认现有 `dependencies.yaml` 或安装脚本的扩展点

2. 核对 `vldb-lancedb` 发布产物命名。
   - 确认最新 Release 的 tag 与资产文件名
   - 明确不同平台库模式包的命名规则

3. 扩展安装逻辑。
   - 增加对 `OpenVulcan/vldb-lancedb` 最新版本解析
   - 增加当前系统/架构到 release asset 的匹配下载逻辑
   - 将库模式包纳入现有一键依赖安装流程

4. 回归验证。
   - 验证脚本能解析最新版本
   - 验证不同平台选择逻辑是否合理
   - 验证本机平台能命中预期资产名称

5. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先复用当前仓库已有的 GitHub Release 下载与平台识别逻辑
- 版本来源以 GitHub Releases 的最新稳定 tag 为准
- 资产命名匹配严格基于 `vldb-lancedb-lib-v<version>-<target>` 规则，不做模糊猜测

# 验收标准

1. 安装脚本可自动发现 `OpenVulcan/vldb-lancedb` 最新发布版本。
2. 安装脚本可根据当前平台选择正确的库模式发布包。
3. 下载逻辑接入现有依赖安装流程，不破坏既有 Lua 依赖安装能力。
4. 本机平台能够成功匹配出目标资产名。
5. 相关文档或说明同步更新到位。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已在 `install_lua_deps.ps1` 与 `install_lua_deps.sh` 中新增宿主级 `vldb-lancedb` 动态库自动下载逻辑。
- 安装脚本现在会查询 `OpenVulcan/vldb-lancedb` 最新 GitHub Release，并按当前平台自动选择 `vldb-lancedb-lib-v<version>-<target>` 资产。
- 动态库统一落到 `third_party/deps`，头文件与库模式文档落到 `third_party/vldb_lancedb/include` 与 `third_party/vldb_lancedb/docs`，与 skill 依赖链路完全隔离。
- Windows 本机已实跑 `install_lua_deps.ps1`，确认能够成功下载 `v0.1.2` 的库模式包并完成落盘。

## 2. 📂文件变更清单

### 修改
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.ps1`
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.sh`

### 运行产物
- `D:\projects\vulcan-mcp-client\third_party\deps\vldb_lancedb.dll`
- `D:\projects\vulcan-mcp-client\third_party\vldb_lancedb\include\vldb_lancedb.h`
- `D:\projects\vulcan-mcp-client\third_party\vldb_lancedb\docs\LIBRARY_USAGE.zh-CN.md`

## 3. 💻关键代码调整详情

- 为 PowerShell 脚本新增：
  - 当前 CPU 架构解析函数
  - `vldb-lancedb` 平台资产映射函数
  - 最新 Release 查询与资产选择逻辑
  - 压缩包解压、动态库提取、头文件/文档整理逻辑
  - 版本化 marker 文件，避免重复下载
- 为 Bash 脚本新增：
  - Linux/macOS 平台目标映射逻辑
  - `vldb-lancedb` 最新 Release 查询与资产解析逻辑
  - `.tar.gz` / `.zip` 解压处理与运行时库提取逻辑
  - 与 PowerShell 脚本一致的安装目录布局
- 两个脚本都将 `vldb-lancedb` 定义为宿主必需依赖，安装失败会直接中断流程，避免运行期才暴露缺失问题。

## 4. ⚠️遗留问题与注意事项

- 当前只在 Windows 平台实跑了完整脚本；Linux/macOS 侧已做语法校验与 GitHub Release 资产命名校验，仍建议后续在对应平台各跑一轮真实下载验证。
- 当前 `vldb-lancedb` 最新 Release 未包含 `x86_64-apple-darwin` 资产，如果在 Intel macOS 上执行，脚本会按资产名查找并在缺失时失败退出，这是预期行为。
