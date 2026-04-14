# 任务计划：提纯 Lua 依赖安装器并将包差异下沉到配置文件

## 任务目标

将 `scripts/install_lua_deps.ps1` 与 `scripts/install_lua_deps.sh` 中当前按具体包名分支处理的安装差异提纯为“通用安装器”逻辑，把包级别的开关、变量覆盖、安装目标、附加环境变量、Windows 覆盖 rockspec 等差异信息迁移到 `scripts/lua_packages.txt` 中统一声明。

## 执行步骤

1. 盘点当前 PowerShell 与 Shell 安装脚本中所有按包名或依赖名硬编码的差异处理点。
2. 设计并实现 `scripts/lua_packages.txt` 的扩展配置语法，使其可以表达：
   - 包安装目标（远端包名 / 本地 rockspec 路径）
   - 附加安装参数
   - 包级环境变量
   - 依赖变量映射与可选覆盖
3. 调整 `scripts/install_lua_deps.ps1` 与 `scripts/install_lua_deps.sh` 的解析与执行逻辑，使脚本只基于配置驱动安装，不再硬编码具体包判断。
4. 更新 `scripts/lua_packages.txt`，将现有 Windows 特化逻辑迁移为配置声明。
5. 重新执行 Windows 安装脚本验证，确认所有 Lua 包仍可成功安装。
6. 完成自检、补充执行变更总结，并将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 继续使用现有 `lua_packages.txt` 文本配置格式，做小步扩展，不额外引入 JSON/YAML/TOML 解析器。
- 保持 `ps1/sh` 的职责为“解析配置 + 安装执行 + 通用环境准备”，避免脚本内部继续堆积具体包知识。
- 尽量让 PowerShell 与 Shell 版本共享同一套配置语义，降低后续维护成本。

## 验收标准

- `scripts/install_lua_deps.ps1` 与 `scripts/install_lua_deps.sh` 中不再存在按 `luasec`、`luaossl`、`lyaml`、`lua-zlib`、`lrexlib-pcre2` 等具体包名分支处理安装差异的逻辑。
- `scripts/lua_packages.txt` 可以完整表达现有包差异，包括本地 override rockspec、变量覆盖与附加环境变量。
- Windows 下重新执行 `powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1` 后，目标包仍全部安装成功。
- 计划文件完成变更总结并迁移至 `docs/completed/20260413/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 将包级安装目标、本地 override rockspec、依赖变量映射、包级环境变量全部下沉到 `scripts/lua_packages.txt`。
- 将 `scripts/install_lua_deps.ps1` 重构为“解析配置 + 解析引用 + 通用安装执行”模式，移除对具体包名的安装分支判断。
- 将 `scripts/install_lua_deps.sh` 同步重构为相同的配置驱动模式，并将依赖收集逻辑改为由配置推导，而不是脚本内硬编码依赖列表。
- 重新执行 Windows 安装脚本验证通过，并对 Shell 脚本执行 `bash -n` 语法校验通过。

### 2. 📂文件变更清单

- 修改：`scripts/install_lua_deps.ps1`
- 修改：`scripts/install_lua_deps.sh`
- 修改：`scripts/lua_packages.txt`
- 新增沿用：`scripts/luarocks_overrides/windows/lrexlib-pcre2-2.9.2-1.rockspec`
- 新增沿用：`scripts/luarocks_overrides/windows/luasec-1.3.2-1.rockspec`
- 新增沿用：`scripts/luarocks_overrides/windows/luaossl-20250929-0.rockspec`
- 新增沿用：`scripts/luarocks_overrides/windows/lyaml-6.2.8-1.rockspec`

### 3. 💻关键代码调整详情

- 在 PowerShell 脚本中新增通用函数：
  - `Get-CurrentPlatformKey`
  - `Test-ConfigOsMatch`
  - `Join-BaseWithRelativePath`
  - `Resolve-ConfigReference`
- 在 Shell 脚本中新增对应的通用解析函数：
  - `get_current_platform`
  - `config_os_matches`
  - `append_assoc_list`
  - `resolve_config_ref`
- 扩展 `lua_packages.txt` 语法，支持以下配置项：
  - `install <os> <target_ref>`
  - `arg <os> <single_arg>`
  - `env <os> <name> <value_ref>`
  - `depvar <dep_name> <os> <var_name> <value_ref>`
- 将以下原本写死在脚本中的包差异迁移到配置文件：
  - Windows 下 `luasec`、`luaossl`、`lrexlib-pcre2`、`lyaml` 的本地 override rockspec 目标
  - `lua-zlib` 的 `CMAKE_PREFIX_PATH`
  - OpenSSL、PCRE2、zlib、libyaml 的包级变量映射
- 将 PowerShell 中依赖 `bin` 目录注入 `PATH` 的逻辑改为遍历所有解析出的依赖路径，而不再列举固定依赖名。
- 将 Shell 中的预编译依赖检测和源码编译循环改为基于配置推导的 `REQUIRED_DEPS` 列表。

### 4. ⚠️遗留问题与注意事项

- `scripts/install_lua_deps.sh` 已完成配置驱动改造并通过 `bash -n` 语法校验，但本回合没有在 Linux/macOS 真实环境做端到端安装验证。
- 当前 `lua_packages.txt` 中的 `arg` 配置按“单个参数一行”设计；若后续需要传递包含空格的复杂参数，建议继续使用引用值或再扩展配置语法。

## 补充执行变更总结（lyaml 下载地址修正）

### 1. 核心修复与调整概述

- 修复 `scripts/luarocks_overrides/windows/lyaml-6.2.8-1.rockspec` 中使用 `http://github.com/...` 导致归档下载失败的问题。
- 将 `lyaml` 的主页地址与源码归档地址统一切换为 HTTPS，并使用 GitHub 标签归档路径，避免旧下载地址跳转或拒绝访问造成安装中断。
- 重新执行 `powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1`，确认 `lyaml` 可以正常拉取、编译并安装，且整套 Lua 包安装结果恢复为全量成功。

### 2. 📂文件变更清单

- 修改：`scripts/luarocks_overrides/windows/lyaml-6.2.8-1.rockspec`
- 修改：`docs/completed/20260413/02-LUA_PACKAGE_CONFIG_REFINEMENT.md`

### 3. 💻关键代码调整详情

- 将 `homepage` 从 `http://github.com/gvvaughan/lyaml` 调整为 `https://github.com/gvvaughan/lyaml`。
- 将 `source.url` 从 `http://github.com/gvvaughan/lyaml/archive/v6.2.8.zip` 调整为 `https://github.com/gvvaughan/lyaml/archive/refs/tags/v6.2.8.zip`。
- 复测结果显示 `lyaml 6.2.8-1` 已成功安装到 `D:\projects\vulcan-mcp-client\third_party\lua_packages`，安装汇总中全部目标包均为 `OK`。

### 4. ⚠️遗留问题与注意事项

- 本次问题属于上一次 Windows override 配置中的外部下载地址失效，不影响“安装器提纯、逻辑下沉到 `lua_packages.txt`”这一主线设计。
- 若后续其他 override rockspec 也引用历史 GitHub `http` 归档地址，建议继续统一巡检并切换为稳定的 HTTPS 标签归档地址，减少外部源波动带来的安装失败。
