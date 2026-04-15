# 任务目标

修复 Lua C 模块搜索路径配置，排除对 `lua_packages/*/5.1/` 目录的依赖，统一改为优先使用 `lua_packages/lib/lua/` 与 `lua_packages/share/lua/` 布局，并检查构建/复制流程是否错误地把模块放进了 `5.1` 目录。

# 执行步骤

1. 审阅当前 `package.path` / `package.cpath` 的构造逻辑，确认 `5.1` 目录被写入的具体位置。
2. 检查项目中的构建、打包、复制脚本或 make 相关流程，确认 `lua_packages` 的目录布局是否被错误复制成 `5.1` 子目录。
3. 修改运行时代码，统一改为使用 `lua_packages/share/lua/` 与 `lua_packages/lib/lua/` 作为 Lua 包与 C 模块的主要查找目录，并排除对 `5.1` 子目录的依赖。
4. 做最小验证，确认配置逻辑可编译，并尽可能验证 `lfs` 的加载路径问题已被修复。
5. 追加执行变更总结并将计划文件归档。

# 技术选型

- 以当前实际部署目录 `lua_packages/lib/lua/` 与 `lua_packages/share/lua/` 为标准布局，不再依赖 `5.1` 子目录。
- 优先修正宿主的路径拼装逻辑，而不是继续容忍多套目录布局并行存在。
- 同步检查构建或复制链路，避免后续继续把模块重复复制到错误层级。

# 验收标准

- `package.cpath` / `package.path` 已改为基于 `lua` 目录而不是 `5.1` 目录。
- 已确认构建/复制流程中是否存在将 Lua 包复制到 `5.1` 子目录的错误行为。
- 至少完成编译级验证；若能在当前环境下验证 `lfs` 加载成功则一并完成。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 修复了 Lua 宿主运行时对 `package.path` / `package.cpath` 的目录拼装逻辑，排除了对 `lua_packages/*/5.1/` 子目录的依赖，统一改为使用 `lua_packages/share/lua/` 与 `lua_packages/lib/lua/`。
- 确认当前问题并不是 `lfs.dll` 缺失，而是运行时查找路径只覆盖了 `5.1` 子目录，导致在仅存在 `lib/lua/lfs.dll` 的部署环境下 `require(\"lfs\")` 失败。
- 同步修复构建输出流程，避免再把 `third_party/lua_packages` 中的 `5.1` 子目录复制到产物目录，减少历史布局继续污染运行环境。

## 2. 📂文件变更清单

### 修改文件

- `src/lua_engine.rs`
- `scripts/build.ps1`
- `scripts/build.sh`
- `scripts/install_lua_deps.ps1`
- `docs/plan/20260415-30-LUA_PACKAGE_PATH_LAYOUT_FIX.md`（后续已归档）

### 新增文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

- `src/lua_engine.rs`
  - 将 Windows 下 `package.cpath` 从：
    - `share/lua/5.1/*.dll`
    - `lib/lua/5.1/*.dll`
    - `lib/lua/5.1/loadall.dll`
    调整为：
    - `lib/lua/*.dll`
    - `lib/lua/*/init.dll`
    - `lib/lua/loadall.dll`
  - 将 `package.path` 从：
    - `share/lua/5.1/*.lua`
    - `share/lua/5.1/*/init.lua`
    调整为：
    - `share/lua/*.lua`
    - `share/lua/*/init.lua`
- `scripts/build.ps1` 与 `scripts/build.sh`
  - 原先会把 `third_party/lua_packages/lib/lua` 与 `share/lua` 整棵树原样复制到输出目录，因此如果源目录里存在 `5.1` 子目录，也会被一并带进产物。
  - 现在改为显式跳过名为 `5.1` 的顶层子目录，仅同步运行时真正需要的 `lua` 目录内容。
- `scripts/install_lua_deps.ps1`
  - “Installed files” 输出改为以 `lib/lua` 与 `share/lua` 为主视图，同时过滤 `5.1` 子目录，避免继续把旧布局当作默认结果展示。

## 4. ⚠️遗留问题与注意事项

- 本次已完成 `cargo fmt` 与 `cargo check`，但没有在目标外部部署目录上直接跑一次新二进制验证 `require(\"lfs\")`；要完成这一步需要重新部署新构建产物。
- 从现有脚本看，`make` 本身不是直接错误来源；真正会把 `5.1` 子目录带进产物的是构建同步阶段对 `third_party/lua_packages` 的原样复制逻辑。
- `third_party/lua_packages` 中是否仍会被 luarocks 安装出 `5.1` 子目录并不影响本次运行时修复，但后续如果想彻底消除重复布局，可以再单独收敛 Lua 依赖安装树结构。
