# 任务计划：修复 Lua 依赖安装脚本的 luarocks 参数兼容问题

## 任务目标

修复 `scripts/install_lua_deps.ps1` 在当前 Windows 环境下执行 `luarocks install` 时因传入不兼容参数而导致 `luasec`、`luaossl`、`lyaml`、`lrexlib-pcre`、`lua-zlib` 等包安装失败的问题，确保脚本能够在项目本地依赖布局下正确安装 Lua C 扩展依赖。

## 执行步骤

1. 检查 `scripts/install_lua_deps.ps1` 当前实现，定位 `luarocks install` 参数拼装逻辑以及依赖目录注入方式。
2. 在本地复现并确认 `unknown option '--variable'` 的触发条件，核对当前项目内置 `luarocks` 版本支持的命令参数。
3. 按照 `luarocks` 实际支持的调用方式调整脚本，优先采用兼容当前版本且可维护的依赖路径传递方案。
4. 重新执行安装脚本或针对失败包进行验证，确认关键失败项恢复正常，且不破坏已安装包的处理逻辑。
5. 对照本计划逐项自检，补充执行变更总结后，将计划文件迁移到 `docs/completed/20260413/`。

## 技术选型

- 保持 PowerShell 方案，不引入额外脚本语言或新的外部工具。
- 优先兼容项目当前下载的 `luarocks` Windows 发行版，而不是依赖用户全局环境升级。
- 尽量复用现有的 `config.lua`、`PATH` 与依赖目录结构，避免扩大改动范围。

## 验收标准

- `scripts/install_lua_deps.ps1` 不再向 `luarocks install` 传递当前版本不支持的 `--variable` 参数。
- 至少对本次报错的核心包安装流程完成实测验证，确认不再出现相同报错。
- 脚本修改符合仓库规范，新增或调整的代码带有必要的中英文双语注释。
- 计划文件包含完整的执行变更总结，并在任务完成后迁移到 `docs/completed/20260413/`。

## 执行变更总结

### 1. 核心修复与调整概述

- 修复 `scripts/install_lua_deps.ps1` 与 `scripts/install_lua_deps.sh` 中对 LuaRocks 3.12.x 的错误参数用法，将依赖变量注入方式从 `--variable=...` 调整为 `VAR=VALUE`。
- 修复 Windows PowerShell 5.1 下 `$IsWindows/$IsMacOS` 不可用导致的平台误判问题，改为使用 `RuntimeInformation` 做统一平台探测。
- 为 Windows 安装流程补齐 VS/MSVC 环境激活、`uname/true` 兼容桩，以及若干 Windows 专用 rockspec 覆盖文件，解决 `luasec`、`lrexlib-pcre2`、`luaossl`、`lyaml` 在当前预编译依赖布局下的构建失败。
- 实际重新执行 `powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1`，确认所有目标包均安装成功。

### 2. 📂文件变更清单

- 修改：`scripts/install_lua_deps.ps1`
- 修改：`scripts/install_lua_deps.sh`
- 修改：`scripts/lua_packages.txt`
- 新增：`scripts/luarocks_overrides/windows/lrexlib-pcre2-2.9.2-1.rockspec`
- 新增：`scripts/luarocks_overrides/windows/luasec-1.3.2-1.rockspec`
- 新增：`scripts/luarocks_overrides/windows/luaossl-20250929-0.rockspec`
- 新增：`scripts/luarocks_overrides/windows/lyaml-6.2.8-1.rockspec`

### 3. 💻关键代码调整详情

- 在 PowerShell 安装脚本中新增 `Get-LuaRocksVariableAssignments`，集中管理 OpenSSL、PCRE2、zlib、libyaml 的 LuaRocks 变量覆盖参数。
- 在 PowerShell 安装脚本中新增 `Ensure-UnameStub`，为 Windows 构建环境提供最小的 `uname` 与 `true` 命令兼容桩。
- 在 PowerShell 安装脚本中新增 `Get-LuaRocksInstallTarget`，对 Windows 下存在 upstream rockspec 兼容问题的包切换到仓库内维护的覆盖版 rockspec。
- 在共享包清单中将 `lrexlib-pcre` 调整为与依赖一致的 `lrexlib-pcre2`。
- 在 Windows 覆盖版 rockspec 中分别修复了以下问题：
  - `luasec`：补齐 `crypt32/user32/advapi32` 链接库。
  - `lrexlib-pcre2`：切换为 `pcre2-8-static` 并补充 `PCRE2_STATIC` 宏。
  - `luaossl`：将 Windows 下的 OpenSSL 1.x 库名改为当前预编译产物使用的 OpenSSL 3 库名，并补充系统链接库。
  - `lyaml`：改用 LuaRocks `builtin` 构建器直接编译 `ext/yaml/*.c`，绕开 Unix-only 的 `luke` 构建器，并补充 `YAML_DECLARE_STATIC` 宏。

### 4. ⚠️遗留问题与注意事项

- `scripts/install_lua_deps.sh` 已同步修正 `VAR=VALUE` 传参逻辑，但本次仅在 Windows PowerShell 环境完成了完整实测，Linux/macOS 侧未在当前回合做端到端验证。
- `scripts/luarocks_overrides/windows/` 下的覆盖版 rockspec 当前是为项目现有预编译依赖版本定制的；若未来升级 OpenSSL、PCRE2 或 libyaml 版本，需同步复核这些覆盖文件中的库名与链接参数。
