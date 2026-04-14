## 任务目标

将 skill 附属能力的提供方式进一步简化为统一的 `file` 规则，不再使用 `type` 或 `generator` 字段。

新规范如下：

1. `prompts` / `resources` / `resource_templates` 仅保留 `file`
2. 若 `file` 扩展名为 `.lua`，则按生成器执行
3. 若 `file` 不是 `.lua`，则按静态文件处理
4. 不保留任何历史兼容逻辑

## 执行步骤

1. 清理当前 skill 元数据中的 `type` / `generator` 字段定义与运行时分支。
2. 将运行时读取逻辑统一改为基于 `file` 扩展名判断静态模式或 Lua 生成器模式。
3. 调整 `codeview_ast` 示例配置与相关文件。
4. 同步更新技能说明文档。
5. 完成构建与隔离验证。
6. 记录执行变更总结并归档。

## 技术选型

- 不再显式声明 provider type，避免重复配置。
- 统一通过 `file` 扩展名判断模式，降低配置复杂度。
- 不保留 `type`、`generator` 的兼容读取逻辑。

## 验收标准

1. skill 元数据中不再存在 `type`、`generator` 字段
2. 运行时仅通过 `file` 是否为 `.lua` 判断模式
3. `codeview_ast` 示例已迁移到新规范
4. 文档已同步
5. `cargo build` 与隔离验证通过

## 执行变更总结

### 1. 核心修复与调整概述

- 删除 skill 附属能力中的 `type` / `generator` 配置模型。
- 统一改为只使用 `file` 字段，并通过扩展名判断：
  - `.lua` -> 生成器
  - 其他文件 -> 静态内容
- 同步迁移 `codeview_ast` 示例与技能说明文档。

### 2. 📂文件变更清单

修改：
- `src/lua_skill.rs`
- `src/lua_engine.rs`
- `runtime/lua_skills/codeview_ast/skill.json`
- `output/lua_skills/codeview_ast/skill.json`
- `docs/lua_skills.md`

新增：
- 无

删除：
- 无

### 3. 💻关键代码调整详情

- 在 `src/lua_skill.rs` 中移除了 `resource/prompt/resource_template` 的 `type` 与 `generator` 字段，只保留 `file`。
- 在 `src/lua_engine.rs` 中增加基于 `file` 扩展名的模式判断：
  - `file` 为 `.lua` 时调用 Lua 生成器
  - 否则读取静态文件
- 在 `codeview_ast` 示例中，将资源模板示例配置从 `generator` 字段切换为 `file = "templates/example_generator.lua"`。
- 在 `docs/lua_skills.md` 中同步更新为“只认 file、按扩展名判断模式”的规范说明。

### 4. ⚠️遗留问题与注意事项

- 现在 `prompt` 的静态文件模式仍为原文返回，不会替换 `{{placeholder}}`；如需动态展开，应直接使用 `.lua` 文件。
- 旧的 `type` / `generator` 配置现在不会再被识别，必须改成新的单 `file` 规则。
- 正式运行中的服务仍需重启，新的 skill 规则才会对外生效。
