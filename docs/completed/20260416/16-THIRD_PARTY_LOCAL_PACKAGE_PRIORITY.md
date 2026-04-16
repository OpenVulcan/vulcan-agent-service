# 任务目标

为宿主依赖与 Lua 依赖初始化脚本增加本地预置包优先策略：在访问 GitHub Release 之前，先从 `third_party` 目录检测当前平台对应的依赖压缩包；如果本地已存在匹配包，则直接解压安装，避免私有仓库阶段因为匿名 API 访问失败而中断调试流程。

# 执行步骤

1. 梳理当前宿主依赖与 Lua 依赖脚本中的下载链路。
   - 确认 `install_host_deps` 与 `install_lua_deps` 的本地/远程优先级
   - 明确需要支持本地预置包的资产名称规则

2. 设计本地优先策略。
   - 在 `third_party` 下检测当前平台对应资产文件是否存在
   - 若存在，直接使用本地压缩包完成解压安装
   - 若不存在，再继续现有远程下载逻辑

3. 实现脚本修改。
   - PowerShell 脚本支持本地包优先
   - Bash 脚本支持本地包优先
   - 保持现有目录布局与安装结果不变

4. 回归验证。
   - 语法校验通过
   - 本机至少验证一条本地包命中路径
   - 确认不影响远程下载回退逻辑

5. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 本地包命名直接沿用远程资产名称，例如 `vldb-lancedb-lib-v0.1.2-x86_64-pc-windows-msvc.zip`。
- 本地检测范围限定在 `third_party` 及其直接子目录，避免全盘递归增加噪音。
- 继续保留“本地优先、远程回退”的双路径设计，不引入 GitHub Token 机制。

# 验收标准

1. `install_host_deps` 在 `third_party` 存在匹配压缩包时可直接解压安装。
2. `install_lua_deps` 的宿主依赖调用链能够复用该本地优先策略。
3. 本地不存在压缩包时，脚本仍可继续走现有远程回退逻辑。
4. 脚本语法校验通过，且本机至少完成一次关键路径验证。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已为 `install_lua_deps` 补齐本地预置包优先策略：在访问 GitHub Release 之前，先从 `third_party` 顶层及其直接子目录查找当前平台对应的 `lua-deps-*` 压缩包。
- 已将 `lua-deps` 平台命名与当前实际 release 资产对齐，支持：
  - `windows-x64`
  - `linux-x64`
  - `linux-arm64`
  - `macos-x64`
  - `macos-arm64`
- 已保留“本地优先 -> GitHub Release -> 本地编译”的回退链路，并将私有仓库场景下的日志提示改得更准确，不再误导成单纯“release 不存在”。

## 2. 📂文件变更清单

### 修改
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.ps1`
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.sh`

## 3. 💻关键代码调整详情

- `install_lua_deps.ps1`
  - 新增 `Get-CurrentArchitectureKey`
  - 新增 `Get-PrebuiltDepsPlatform`
  - 新增 `Find-LocalArchive`
  - 重写 `Download-Prebuilt-Deps`，使其优先使用 `third_party` 本地压缩包，再回退到 GitHub Release
- `install_lua_deps.sh`
  - 新增 `find_local_archive`
  - 新增 `get_prebuilt_deps_platform`
  - 重写 `download_prebuilt_deps`，实现与 PowerShell 版本一致的本地优先逻辑
- 实际回归验证中，Windows 环境已成功命中：
  - `D:\projects\vulcan-mcp-client\third_party\lua-deps-windows-x64.tar.gz`
  并完成整轮 Lua 依赖安装。

## 4. ⚠️遗留问题与注意事项

- 当前 `install_host_deps` 与 `install_lua_deps` 已都支持本地预置包优先，但这轮只对 `lua-deps-*` 的命中链路做了完整实机回归。
- 私有仓库阶段仍不支持 GitHub Token；如果本地没有预置包，脚本仍会尝试匿名访问 GitHub Release，然后再回退到本地编译。
- `20260416-14-LUAJIT_DLL_BUILD_FIX.md` 对应的“完整全新环境顺序验证”仍可后续单独做一次完整闭环验证，本次未一并归档该计划。
