# 任务目标

清理当前仓库中仍然活跃的 `lsqlite3` / `lsqlite3complete` 引入代码，避免 Lua 依赖安装链继续携带已经被宿主管理 SQLite 能力替代的旧绑定。

# 执行步骤

1. 盘点代码、脚本与当前文档中的活跃 `lsqlite3` 引入位置，区分“正在执行的代码链”与“历史归档文档”。
2. 删除活跃安装链中的 `lsqlite3complete` 包声明，确保 `install_lua_deps` 后续不再尝试安装旧 SQLite Lua 绑定。
3. 检查当前运行中的 SQLite skill 是否仍依赖 Lua 侧 `lsqlite3`，确认宿主管理 SQLite 路径已完全覆盖原需求。
4. 执行回归检索，确认 `src/`、`runtime/`、`scripts/` 等活跃目录中不再残留 `lsqlite3` 引入代码。
5. 在计划文件末尾补充执行变更总结，并在任务完成后归档到 `docs/completed/20260417/`。

# 技术选型

1. 保留历史归档文档原貌，不回写旧记录，避免破坏历史上下文。
2. 仅清理当前活跃代码链与安装链中的 `lsqlite3` 残留。
3. 以 `ripgrep` 全局检索作为验收基线，确保清理范围可复核。

# 验收标准

1. `scripts/lua_packages.txt` 中不再包含 `lsqlite3complete`。
2. `src/`、`runtime/`、`scripts/` 等活跃目录中不存在 `lsqlite3` / `lsqlite3complete` 引入代码。
3. 历史归档文档可以保留旧记录，但需要在本次执行总结中明确说明。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次清理的重点是收掉当前仍会实际参与安装链的 `lsqlite3complete` 引入。仓库现状已经切换到宿主管理 SQLite，因此继续在 Lua 依赖安装清单中保留 `lsqlite3complete` 会造成旧链路误导和不必要的安装开销。实际处理上仅移除了活跃安装链中的包声明，并确认 `src/`、`runtime/`、`scripts/` 活跃目录已不存在 `lsqlite3` 相关引入代码。

## 2. 📂文件变更清单

### 修改

- `scripts/lua_packages.txt`
- `docs/plan/20260417-06-REMOVE_LSQLITE3_INTEGRATION.md`

### 说明

- `docs/completed/20260416/` 与 `docs/completed/20260417/` 中的旧文档仍保留历史记录，没有回写修改。

## 3. 💻关键代码调整详情

1. 删除 `scripts/lua_packages.txt` 中的：
   - `pkg lsqlite3complete`
2. 保留其他 Lua 包声明不变，避免影响非 SQLite 相关依赖安装。
3. 使用全文检索确认活跃代码目录：
   - `src/`
   - `runtime/`
   - `scripts/`
   中已无 `lsqlite3` / `lsqlite3complete` 命中。

## 4. ⚠️遗留问题与注意事项

1. 历史归档文档中仍会出现 `lsqlite3` / `lsqlite3complete` 记录，这是保留历史上下文的预期行为，不代表当前代码链仍在使用。
2. 如果后续重新运行 `install_lua_deps`，将不再安装 Lua 侧 SQLite 绑定；当前 SQLite 能力应统一走宿主动态库链路。
