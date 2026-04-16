# 任务目标

修复 `src/lua_engine.rs` 在 Linux 编译时报出的 `cpath_pattern` / `path_pattern` 未定义问题，确保 `vulcan-mcp-client` 在跨平台构建时可正常通过编译。

# 执行步骤

1. 复盘并定位编译错误。
   - 检查 `src/lua_engine.rs` 中相关作用域与变量定义
   - 确认是重构遗漏、作用域缩窄还是条件分支问题

2. 实施最小且稳妥的修复。
   - 补齐缺失变量或调整作用域
   - 保持现有 Lua package 路径布局与运行时行为不变

3. 回归验证。
   - 至少执行 `cargo check`
   - 重点确认 `lua_engine.rs` 对应模块不再报错

4. 文档闭环。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先采用最小修复，不扩大改动面。
- 保持 Windows 与 Linux 路径行为一致，避免为单平台修复引入新的路径回归。
- 不改动现有对外接口，只修正内部实现。

# 验收标准

1. `cargo check` 通过。
2. `src/lua_engine.rs` 中不再出现 `cpath_pattern` / `path_pattern` 未定义错误。
3. 不破坏当前 Lua package 的运行时路径注入逻辑。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已修复 `src/lua_engine.rs` 在 Linux/macOS 构建时 `cpath_pattern` 与 `path_pattern` 未定义的问题。
- 根因是 `setup_package_paths` 只保留了 Windows 平台的路径模式定义，导致非 Windows 平台进入公共拼接逻辑时缺少变量。
- 现已补齐 Linux 与 macOS 的 `package.cpath` 模式，以及 Unix 通用的 `package.path` 模式，保持现有 `lua_packages/share/lua` 与 `lua_packages/lib/lua` 布局不变。

## 2. 📂文件变更清单

### 修改
- `D:\projects\vulcan-mcp-client\src\lua_engine.rs`

## 3. 💻关键代码调整详情

- 在 `setup_package_paths` 中新增：
  - `#[cfg(target_os = "linux")]` 的 `cpath_pattern`
  - `#[cfg(target_os = "macos")]` 的 `cpath_pattern`
  - `#[cfg(unix)]` 的 `path_pattern`
- 保持 Windows 现有 `.dll` 路径模式不变，同时为 Linux 使用 `.so`、macOS 使用 `.dylib`。
- 这样公共的 `new_cpath` / `new_path` 拼接代码在各平台都有完整定义，避免条件编译后变量缺失。

## 4. ⚠️遗留问题与注意事项

- 本次是最小修复，只解决跨平台编译缺口，没有调整更高层的 Lua package 布局策略。
- 当前 `cargo check` 已通过，但后续若要做更严格验证，仍建议在 Linux/macOS 真机或 CI 上实际运行一次相关 Lua 加载流程。
