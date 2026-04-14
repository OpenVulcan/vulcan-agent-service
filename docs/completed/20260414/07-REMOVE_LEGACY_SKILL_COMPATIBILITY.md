## 任务目标

移除当前 skill 体系中保留的旧版本兼容逻辑，仅保留新的规范实现，确保后续行为明确且单一。

本次范围重点包括：

1. 移除旧版 `init_script` 兼容，仅保留 `init_scripts.ps1` / `init_scripts.sh`
2. 清理当前 skill 元数据与运行时里残留的旧兼容逻辑说明
3. 同步更新 skill 说明文档与示例配置
4. 完成构建验证并归档记录

## 执行步骤

1. 梳理当前 skill 元数据、Lua 引擎初始化脚本选择逻辑与文档中的兼容说明。
2. 修改元数据结构与运行时逻辑，去除旧版初始化脚本兼容分支。
3. 同步更新 `codeview_ast` 示例配置与技能开发说明文档。
4. 执行编译验证。
5. 记录执行变更总结并归档。

## 技术选型

- 不保留旧字段兼容逻辑，直接统一到新规范。
- 配置层失败要尽早暴露，避免静默回退掩盖错误。

## 验收标准

1. skill 初始化脚本仅支持 `init_scripts.ps1` / `init_scripts.sh`
2. 运行时不存在旧版 `init_script` 回退逻辑
3. 文档与示例配置已同步到新规范
4. `cargo build` 通过

## 执行变更总结

### 1. 核心修复与调整概述

- 删除 `SkillMeta` 中旧版单字段 `init_script`。
- 删除 Lua 引擎里对 `init_script` 的回退兼容逻辑，仅保留 `init_scripts.ps1/sh`。
- 同步修正技能开发文档中的旧示例与旧说明，统一到新规范。

### 2. 📂文件变更清单

修改：
- `src/lua_skill.rs`
- `src/lua_engine.rs`
- `docs/lua_skills.md`

新增：
- 无

删除：
- 无

### 3. 💻关键代码调整详情

- 在 `src/lua_skill.rs` 中移除了 `init_script` 元数据字段，skill 配置模型不再接受旧入口。
- 在 `src/lua_engine.rs` 中将 `select_init_script` 改为只从 `init_scripts.ps1/sh` 取值，不再做旧字段回退。
- 在 `docs/lua_skills.md` 中将初始化脚本说明、JSON 示例和模板示例全部替换为 `init_scripts` 双字段写法。

### 4. ⚠️遗留问题与注意事项

- 旧版 skill.json 若仍使用 `init_script`，现在将不再生效，需显式迁移到 `init_scripts.ps1/sh`。
- 当前运行中的正式服务仍需重启，新的配置模型才会真正对外生效。
