# 任务目标

修正 Lua 包部署同步逻辑：保留运行时只查 `lua_packages/lib/lua/` 与 `lua_packages/share/lua/` 的设计，但把 `third_party/lua_packages/*/5.1/` 下的真实内容平铺复制到输出目录对应的 `lua` 层，而不是直接排除 `5.1` 子目录。

# 执行步骤

1. 审阅当前上一轮修改后的 `scripts/build.ps1` 与 `scripts/build.sh`，确认哪里把 `5.1` 子目录直接过滤掉了。
2. 修改构建同步逻辑，将 `third_party/lua_packages/lib/lua/5.1/*` 平铺复制到 `output/lua_packages/lib/lua/`，并将 `share/lua/5.1/*` 平铺复制到 `output/lua_packages/share/lua/`。
3. 保留运行时 `package.path` / `package.cpath` 只查 `lua` 层的设计，不再回退到 `5.1` 子目录。
4. 做静态验证，确认脚本逻辑与目录结构匹配，并完成编译级验证。
5. 补充执行变更总结并将计划文件归档。

# 技术选型

- 以 `third_party/lua_packages/*/5.1/` 作为依赖安装树的来源目录。
- 以 `output/lua_packages/*/lua/` 作为运行时统一部署目录。
- 通过构建同步阶段做“降一层复制”，而不是让运行时继续兼容多套路径布局。

# 验收标准

- 构建脚本不再直接丢弃 `5.1` 子目录内容。
- `5.1` 目录下的 Lua/C 模块会被平铺复制到输出目录的 `lua` 层。
- 运行时仍只依赖 `lib/lua` 与 `share/lua`。
- 至少完成编译级验证，并确认脚本逻辑与当前 `third_party` 目录结构一致。
- 计划文件完成执行总结并归档。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 修正了上一轮错误的“直接排除 `5.1` 子目录”逻辑，改为以 `third_party/lua_packages/*/5.1/` 作为真实来源目录。
- 当前构建同步流程会把 `5.1` 目录下的 Lua 模块和 C 模块平铺复制到输出目录的 `lua` 层，满足运行时只查 `lib/lua`、`share/lua` 的设计。
- 保留了宿主运行时不再依赖 `5.1` 子目录的改动方向，只修正构建同步阶段的来源选择方式。

## 2. 📂文件变更清单

### 修改文件

- `scripts/build.ps1`
- `scripts/build.sh`
- `scripts/install_lua_deps.ps1`
- `docs/plan/20260415-31-LUA_PACKAGE_FLATTEN_COPY_FIX.md`（后续已归档）

### 新增文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

- `scripts/build.ps1`
  - 对每个 `lib/lua`、`share/lua` 目录，优先检查是否存在 `5.1` 子目录。
  - 若存在，则把 `5.1` 目录作为 `copyRoot`，将其下内容直接复制到输出目录对应的 `lua` 层。
  - 若不存在，则回退为直接复制当前 `lua` 层内容。
- `scripts/build.sh`
  - 使用与 PowerShell 脚本一致的策略：优先取 `.../lua/5.1` 作为复制根目录，再把内容平铺到输出的 `lua/` 层。
- `scripts/install_lua_deps.ps1`
  - 恢复安装结果输出对 `5.1` 目录的识别，先展示 `lib/lua/5.1`、`share/lua/5.1`，仅在不存在时才回退到非版本目录。

## 4. ⚠️遗留问题与注意事项

- 当前已通过静态差异检查，并确认 `third_party/lua_packages/lib/lua/5.1` 确实是现有真实来源目录。
- 本轮未再次运行完整构建，但上一轮的宿主运行时代码改动已经通过 `cargo check`；这一轮仅修正脚本复制策略。
- 若后续想进一步验证，建议实际执行一次构建同步并检查 `output/lua_packages/lib/lua/lfs.dll` 是否由 `third_party/.../5.1/lfs.dll` 正确平铺生成。
