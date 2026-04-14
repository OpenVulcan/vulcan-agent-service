## 任务目标

将 Lua skill 附属能力统一为显式的内容提供器模型，覆盖以下三类能力：

1. prompts
2. resources
3. resource templates

统一约定：

- 支持 `type` 字段
- `type` 默认值为 `file`
- 可显式指定 `type: generator`
- `file` 与 `generator` 两种模式应互斥且可校验

同时完成 `codeview_ast` 示例配置升级，并进行本地验证。

## 执行步骤

1. 梳理当前 skill 元数据结构与运行时加载逻辑，确认 prompts/resources/templates 现有字段与读取分支。
2. 为三类 skill 附属能力增加统一的 `type=file|generator` 元数据模型，并保留默认 `file` 兼容行为。
3. 调整运行时读取逻辑：
   - `file` 模式读取静态文件
   - `generator` 模式执行 Lua 并返回对应协议结构
4. 调整 `codeview_ast` 示例 skill，使用新的 provider type 约定。
5. 完成本地构建与隔离验证。
6. 记录执行变更总结并归档。

## 技术选型

- 继续沿用 skill 目录驱动，不引入额外存储层。
- 使用统一 provider type 模型减少不同能力之间的实现分叉。
- 兼容默认 `file` 模式，避免旧配置全部失效。

## 验收标准

1. prompts/resources/resource templates 三类能力均支持 `type=file|generator`。
2. 未声明 `type` 时默认按 `file` 模式处理。
3. `generator` 模式能通过 Lua 返回对应能力的完整结构。
4. `codeview_ast` 示例已迁移并验证可用。

## 执行变更总结

### 1. 核心修复与调整概述

- 将 skill 附属能力统一为显式 `provider type` 模型，覆盖 `prompts`、`resources`、`resource_templates` 三类能力。
- 新增 `type=file|generator` 约定，其中 `file` 为默认模式，`generator` 用于 Lua 动态返回最终协议结果。
- 调整 `codeview_ast` 示例，使静态说明资源走默认 `file`，资源模板示例走显式 `generator`。
- 同步更新技能开发文档，补充 `init_scripts.ps1/sh` 与 provider type 约定说明。

### 2. 📂文件变更清单

新增：
- `runtime/lua_skills/codeview_ast/templates/example_generator.lua`
- `output/lua_skills/codeview_ast/templates/example_generator.lua`

修改：
- `src/lua_skill.rs`
- `src/lua_engine.rs`
- `runtime/lua_skills/codeview_ast/skill.json`
- `output/lua_skills/codeview_ast/skill.json`
- `docs/lua_skills.md`

删除：
- 无

### 3. 💻关键代码调整详情

- 在 `src/lua_skill.rs` 中为 `resource/prompt/resource_template` 元数据新增 `type` 与 `generator` 字段，并将默认 provider 设为 `file`。
- 在 `src/lua_engine.rs` 中增加：
  - provider type 判定逻辑；
  - `generator` 模式下的结果标准化逻辑；
  - 资源生成器返回 `ResourceReadResult` 的兼容包装；
  - 提示词生成器返回 `PromptGetResult` 的兼容包装。
- 在 `codeview_ast` 示例中：
  - `skill://codeview_ast/guide` 保持默认静态文件模式；
  - `skill://codeview_ast/example/{topic}` 切换为 `generator` 模式；
  - `codeview_ast_first_pass` 保持默认静态提示词文件模式，用于体现默认 `file` 行为。

### 4. ⚠️遗留问题与注意事项

- 当前 `prompt` 的 `file` 模式为“静态原文返回”，不会自动替换 `{{placeholder}}`；如需根据参数动态生成，应使用 `generator` 模式。
- `resource_template` 的 `file` 模式仍支持基于 URI 参数的轻量占位符替换，适合简单模板文本。
- 运行中的正式 `vulcan-mcp` 进程仍需重启，新的 provider type 逻辑与示例配置才会对外生效。
