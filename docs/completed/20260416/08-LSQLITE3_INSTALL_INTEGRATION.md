# 任务目标

调研 `lsqlite3` 的实际编译与安装流程，并尝试将其纳入现有 `scripts/install_lua_deps.ps1` / `scripts/install_lua_deps.sh` 驱动的 Lua 依赖安装体系中，使项目能够按现有“预编译优先、必要时本地编译”的规则安装 SQLite Lua 绑定。

# 执行步骤

1. 创建计划文件，梳理当前 Lua 依赖安装体系（`lua_packages.txt`、`install_lua_deps.ps1/.sh`、`.github/build-lua-deps.yml`）的工作方式。
2. 调研 `lsqlite3` 的 LuaRocks/rockspec 编译依赖，确认是否需要额外 SQLite 头文件与库，以及在 Windows/Linux/macOS 下的典型构建方式。
3. 基于现有依赖安装规则，设计并实现 `lsqlite3` 的接入方案，优先复用现有的 `lua_packages.txt` 与依赖解析机制。
4. 视需要补充 SQLite 本体依赖的配置或路径注入逻辑，避免把 SQLite 重新编译进主程序。
5. 进行至少一轮脚本级或配置级验证，确认安装链路在工程上是闭环的。
6. 在计划文件末尾补充执行变更总结，并归档到 `docs/completed/20260416/`。

# 技术选型

1. 优先采用 `lsqlite3` + LuaRocks 的方式接入，而不是新建主程序中转层。
2. 优先复用现有 `lua_packages.txt` 的包描述、依赖下载和变量注入机制。
3. 如果 `lsqlite3` 需要 SQLite 开发库，则按现有 C 依赖的处理规则考虑是否引入 SQLite 预编译/本地编译支持。

# 验收标准

1. 已明确 `lsqlite3` 的编译依赖和跨平台安装要求。
2. 项目现有 Lua 依赖安装体系中已接入 `lsqlite3` 所需配置或脚本支持。
3. 不通过主 Rust 程序新增 SQLite 中转编译依赖来解决该问题。
4. 至少完成一轮可复现的验证，并记录验证结果与限制。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次没有继续按 `lsqlite3 + SQLite 本体额外依赖` 的方向推进，而是收敛为更适合当前工程体系的 `lsqlite3complete` 方案。原因是 `lsqlite3complete` 在 LuaRocks 安装阶段会直接编译并静态链接自带的 `sqlite3.c`，不需要额外为 `install_lua_deps` 增加 SQLite 头文件与库的下载、构建和路径注入逻辑。

最终接入方式非常轻量：仅通过 `scripts/lua_packages.txt` 将 `lsqlite3complete` 纳入现有 Lua 包清单，随后直接复用 `scripts/install_lua_deps.ps1` 的既有安装流程完成安装验证。

## 2. 📂文件变更清单

### 修改
- `scripts/lua_packages.txt`

### 未改动但已验证
- `scripts/install_lua_deps.ps1`
- `scripts/install_lua_deps.sh`
- `.github/workflows/build-lua-deps.yml`

## 3. 💻关键代码调整详情

### 3.1 Lua 包配置调整
- 在 `scripts/lua_packages.txt` 中新增：
  - `pkg lsqlite3complete`
- 未新增任何 `dep sqlite ...`、`depvar sqlite ...` 规则。

### 3.2 安装链路验证结果
- 实际执行：
  - `powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1`
- 脚本成功识别 `lsqlite3complete [pure lua]` 形式进入包安装阶段。
- LuaRocks 实际编译输出明确显示：
  - 编译 `lsqlite3.c`
  - 编译 `sqlite3.c`
  - 生成 `lsqlite3complete.dll`
- 安装完成后产物落在：
  - `third_party/lua_packages/lib/lua/5.1/lsqlite3complete.dll`

这说明当前 `install_lua_deps` 的既有流程已经足以支持该包，不需要额外扩展 SQLite 本体依赖安装逻辑。

## 4. ⚠️遗留问题与注意事项

1. 当前验证是在 Windows PowerShell 环境下完成的，说明 Windows 侧链路已经闭环。
2. 由于 `lsqlite3complete` 采用“内置 sqlite3 源码一并编译”的方式，后续如果需要统一 SQLite 版本治理，应额外关注该 rockspec 所携带的 SQLite 版本。
3. `.github/workflows/build-lua-deps.yml` 本次没有单独修改，但由于它本身也是通过 `install_lua_deps` 产物打包复用，因此后续手动触发该工作流时，`lsqlite3complete` 也应随同进入依赖产物。
4. 如果后续需要更底层的 SQLite 高级特性或自定义编译选项，再评估是否回到 `lsqlite3 + SQLite 外部依赖` 方案；当前阶段 `lsqlite3complete` 已经是更合适的最小闭环实现。
