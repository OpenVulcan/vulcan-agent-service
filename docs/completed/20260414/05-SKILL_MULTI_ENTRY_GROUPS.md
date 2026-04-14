## 任务目标

将当前 Lua Skill 的单入口注册模型升级为“分组 + 多入口”结构，使一个 `skill.json` 可以声明多个组，每个组内部可独立注册多个 `tools`、`prompts`、`resource_templates`、`resources`，并允许每个入口拥有自己的名称、描述、参数与文件绑定逻辑。

## 执行步骤

1. 梳理当前 `SkillMeta`、`LuaEngine`、协议注册方法与 `skill.json` 示例结构，确认单入口耦合点。
2. 设计新的 `skill.json` 分组结构，明确组级公共字段与入口级字段边界。
3. 重构元数据反序列化模型与运行时注册逻辑，支持多组多入口注册与调用。
4. 调整技能调用分发、资源读取、提示词读取、模板读取等逻辑，确保所有入口按各自定义独立工作。
5. 更新示例技能与说明文档，补充新的 `skill.json` 写法示例。
6. 通过构建和最小行为验证，确认多入口注册能够正常列出与调用。

## 技术选型

- 采用显式 `groups` 数组作为 skill 配置顶层主结构，避免继续在单 skill 顶层堆叠入口字段。
- 工具、提示词、资源、模板继续沿用现有 `file` 约定：`.lua` 为生成器，其他文件为静态文件。
- 运行时内部保留“入口 -> 所属 skill/group”映射，避免多组场景下遍历判断过于分散。

## 验收标准

1. `skill.json` 支持声明多个组，每组下支持多个 `tools`、`prompts`、`resource_templates`、`resources`。
2. 每个入口都可拥有独立名称、描述、参数与文件定义。
3. `tools/list`、`resources/list`、`resources/templates/list`、`prompts/list` 与对应读取/调用接口均支持新结构。
4. 示例技能与文档已同步到新结构。
5. `cargo build` 通过，且至少一个技能完成多入口验证。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将 Lua Skill 元数据从“单 tool 顶层结构”重构为“`groups` 分组结构”，允许一个 skill 同时声明多组、多 tools、多 prompts、多 resources、多 resource templates。
- 已同步重写运行时注册与分发逻辑，工具列表、资源列表、模板列表、提示词列表以及实际调用/读取流程都改为按分组入口展开。
- 已将 `codeview_ast` 与 `__demo` 示例技能迁移到新格式，其中 `__demo` 现在提供两个 tool 入口，作为复制模板的多入口样板。
- 已同步更新 Skill 说明文档，并刷新 `output/lua_skills/` 副本。

### 2. 📂文件变更清单

- 修改：`src/lua_skill.rs`
- 修改：`src/lua_engine.rs`
- 修改：`docs/lua_skills.md`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 修改：`runtime/lua_skills/__demo/skill.json`
- 同步：`output/lua_skills/**`
- 新增：`docs/plan/20260414-05-SKILL_MULTI_ENTRY_GROUPS.md`

### 3. 💻关键代码调整详情

- 在 `src/lua_skill.rs` 中新增 `SkillGroupMeta` 与 `SkillToolMeta`，并把资源、模板、提示词入口全部纳入 group 结构中。
- 在 `src/lua_engine.rs` 中改造 `load_single_skill`、`register_skill_functions`、`call_skill`、`read_resource`、`get_prompt` 等关键路径，支持从 group 中定位具体入口。
- `vulcan.call` 的工具调度表也改为从所有 group 下的 tool 入口统一展开，不再依赖 skill 顶层单一 `tool_name`。
- `__demo` 模板改成“一个 skill 两个 tools + 一组 resources/templates/prompts”的样式，便于直接复制并扩展。

### 4. ⚠️遗留问题与注意事项

- 新结构不再兼容旧版单入口 `skill.json` 字段；现有 skill 必须迁移到 `groups` 模式。
- 隔离验证中已确认单个临时 demo skill 可注册出 2 个 Lua tools（日志显示 `2 Lua skills loaded`），但临时实例的 `/mcp` 会话继续调用仍存在既有 session 问题，因此本次行为验证以构建通过与加载日志为主。
- 后续新增 skill 时，建议优先参考 `runtime/lua_skills/__demo/skill.json` 的多 group、多入口写法。
