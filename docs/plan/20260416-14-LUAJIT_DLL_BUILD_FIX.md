# 任务目标

修复在全新环境下按 `make deps host -> make build release -> make deps lua` 顺序执行时，`install_lua_deps.ps1` 在 LuaJIT SDK 步骤构建 `lua51.dll` 失败的问题，确保 release 构建产物与 Lua 依赖初始化流程能够正确衔接。

# 执行步骤

1. 复盘并定位 `install_lua_deps.ps1` 中 LuaJIT SDK 处理逻辑。
   - 检查 `target/release/build/mlua-sys-*/out/luajit-build/src` 的检测规则
   - 检查 `msvcbuild.bat` 调用方式与失败条件
   - 确认 release 产物与 DLL 二次构建之间的差异

2. 设计并实现修复方案。
   - 优化 LuaJIT DLL 构建流程或回退逻辑
   - 必要时增强日志输出，避免后续再次出现黑盒失败
   - 保证 debug/release 两种构建路径都能复用

3. 回归验证。
   - 复现用户提供的顺序：`make deps host` -> `make build release` -> `make deps lua`
   - 确认 `install_lua_deps.ps1` 可成功完成 LuaJIT SDK 安装与 Lua 包安装

4. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先在现有 `install_lua_deps.ps1` 基础上修复，不改变对外命令顺序。
- 修复应兼容全新环境与已有缓存环境。
- 保留 PowerShell 原生流程，避免引入新的外部依赖。

# 验收标准

1. `make deps host`、`make build release`、`make deps lua` 的顺序在 Windows 环境下可正常执行。
2. `install_lua_deps.ps1` 不再在 LuaJIT DLL 构建阶段失败。
3. LuaJIT SDK 最终能正确落到 `third_party/luajit`。
4. 不破坏已有 Lua package 安装流程。
