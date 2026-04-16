# 任务目标

修复当前 `vulcan-mcp-client` 主分支中由于 LanceDB skill 宿主集成代码不完整而导致的编译失败问题，确保 Linux 环境下 `cargo build` / `cargo check` 可以正常通过。

# 执行步骤

1. 复盘并定位缺失点。
   - 检查 `src/lua_engine.rs` 对 `lancedb_host` 与 `SkillMeta::effective_lancedb` 的引用
   - 核对 `src/lancedb_host.rs`、`src/lua_skill.rs`、`src/main.rs` 当前状态
   - 确认是模块未纳入构建、文件未提交，还是接口定义缺失

2. 实施最小且完整的修复。
   - 补齐缺失模块注册与元数据定义
   - 保证现有 Lua skill / LanceDB 宿主逻辑能够自洽
   - 避免只修一个报错点而留下新的编译缺口

3. 回归验证。
   - 执行 `cargo check`
   - 必要时再执行一次与 Lua/LanceDB 相关的最小验证

4. 文档闭环。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先采用最小但完整的修复，避免大范围重构。
- 以恢复主分支可编译为最高优先级。
- 保持现有已设计的 LanceDB skill 宿主方向，不做无关删减。

# 验收标准

1. `cargo check` 通过。
2. `crate::lancedb_host`、`SkillMeta::effective_lancedb` 等相关编译错误全部消失。
3. 不破坏现有 Lua skill 与 LanceDB 宿主集成代码路径。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已修复当前主分支因 LanceDB skill 宿主代码未完整纳入 crate 而导致的 Linux 编译失败。
- 实际缺失点不是 `lua_engine.rs` 逻辑本身，而是其依赖的三块实现没有一起进入主线：
  - `src/main.rs` 中缺少 `mod lancedb_host;`
  - `src/lua_skill.rs` 中缺少 LanceDB 元数据定义与 `effective_lancedb()`
  - `src/lancedb_host.rs` 尚未纳入仓库
- 补齐后已确认 `cargo check` 可通过。

## 2. 📂文件变更清单

### 新增
- `D:\projects\vulcan-mcp-client\src\lancedb_host.rs`

### 修改
- `D:\projects\vulcan-mcp-client\src\main.rs`
- `D:\projects\vulcan-mcp-client\src\lua_skill.rs`

## 3. 💻关键代码调整详情

- `src/main.rs`
  - 补充 `mod lancedb_host;`，让 crate 根能够解析 `crate::lancedb_host`
- `src/lua_skill.rs`
  - 补充 `SkillLanceDbLogLevel`
  - 补充 `SkillLanceDbMeta`
  - 在 `SkillMeta` 中加入 `lancedb_enable` 与结构化 `lancedb` 配置
  - 补充 `effective_lancedb()`，统一兼容旧布尔配置与新对象配置
- `src/lancedb_host.rs`
  - 纳入 LanceDB 动态库加载、skill 级绑定、JSON/bytes FFI 调用与状态返回等宿主实现

## 4. ⚠️遗留问题与注意事项

- 本次修复的重点是“补齐已设计但未进入主线的编译依赖实现”，不涉及新的接口扩展。
- 当前 `cargo check` 已通过，但工作区仍存在其他与本次修复无关的未提交修改，后续提交时需要继续保持精确挑选。
