# 任务目标

将宿主级原生依赖初始化从 `install_lua_deps` 中拆分出来，形成独立的 `install_host_deps` 脚本，避免未来宿主依赖与 Lua 依赖职责混杂；同时保证当前一键初始化体验不退化。

# 执行步骤

1. 梳理当前 `install_lua_deps.ps1/.sh` 中宿主级依赖逻辑与 Lua 依赖逻辑的边界。
2. 新增独立的宿主依赖初始化脚本（PowerShell / Bash），承载 `vldb-lancedb` 下载与安装逻辑。
3. 调整 `install_lua_deps.ps1/.sh`，通过委托调用或共享逻辑方式接入宿主依赖脚本，保持当前使用体验。
4. 进行语法校验与本机验证，确认拆分后仍可正确安装宿主依赖与 Lua 依赖。
5. 在计划文件末尾追加执行变更总结，并归档到 `docs/completed/20260416/`。

# 技术选型

- 宿主级依赖脚本命名为 `install_host_deps.ps1` 与 `install_host_deps.sh`。
- `install_lua_deps` 保留现有入口职责，但通过显式调用宿主依赖脚本实现逻辑分层。
- 继续沿用 GitHub latest release 检测与平台资产匹配规则，不改变当前安装目录布局。

# 验收标准

1. 仓库新增独立的宿主依赖初始化脚本，且能独立安装 `vldb-lancedb`。
2. `install_lua_deps.ps1/.sh` 不再直接持有宿主依赖核心逻辑。
3. 当前一键初始化流程仍然可用，不影响 Lua 依赖安装。
4. 脚本语法校验通过，且本机至少完成一次关键路径验证。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已将宿主级原生依赖初始化从 `install_lua_deps` 中拆分为独立脚本：
  - `install_host_deps.ps1`
  - `install_host_deps.sh`
- `vldb-lancedb` 的 latest release 解析、平台资产选择、动态库/头文件/文档落盘逻辑已整体迁移到宿主依赖脚本中。
- `install_lua_deps.ps1/.sh` 现在只负责协调调用宿主依赖脚本，不再直接承载宿主级依赖实现细节，职责边界更清晰。
- 已验证：
  - 独立执行 `install_host_deps.ps1` 可正常工作
  - 原有 `install_lua_deps.ps1` 入口仍可完成完整初始化

## 2. 📂文件变更清单

### 新增
- `D:\projects\vulcan-mcp-client\scripts\install_host_deps.ps1`
- `D:\projects\vulcan-mcp-client\scripts\install_host_deps.sh`

### 修改
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.ps1`
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.sh`

## 3. 💻关键代码调整详情

- PowerShell 侧：
  - 新增独立宿主依赖脚本，封装 `vldb-lancedb` 的 GitHub Release 查询、平台 target 映射、压缩包解压与 marker 处理。
  - `install_lua_deps.ps1` 删除宿主依赖实现细节，仅保留对 `install_host_deps.ps1` 的委托调用。
- Bash 侧：
  - 新增 `install_host_deps.sh`，与 PowerShell 版本保持同等能力与目录布局。
  - `install_lua_deps.sh` 只保留对宿主依赖脚本的协调调用。
- 目录布局保持不变：
  - 动态库：`third_party/deps`
  - 头文件：`third_party/vldb_lancedb/include`
  - 文档：`third_party/vldb_lancedb/docs`

## 4. ⚠️遗留问题与注意事项

- 当前已完成 PowerShell 真实运行验证；Bash 侧完成了语法校验，但仍建议后续在 Linux/macOS 环境各执行一轮真实下载验证。
- 宿主依赖脚本目前仍只安装 `vldb-lancedb`；后续如果新增更多宿主级原生库，可以继续在 `install_host_deps` 中扩展，而不应再回填到 `install_lua_deps`。
