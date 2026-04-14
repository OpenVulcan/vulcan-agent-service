## 任务目标

为 Lua skill 体系增加一条内部约定，并提供一个可复制的模板示例：

1. `lua_skills` 目录下，任何以 `__` 开头的技能目录都不参与自动加载
2. 新增 `__demo` skill，作为 skill 逻辑演示与一键复制模板

## 执行步骤

1. 梳理当前 Lua skill 目录扫描与加载逻辑，定位过滤内部目录的入口。
2. 修改技能加载器，跳过所有以 `__` 开头的技能目录。
3. 创建 `runtime/lua_skills/__demo` 示例 skill，覆盖：
   - tool
   - resource
   - resource template
   - prompt
   - `.lua` 生成器示例
4. 同步 `output/lua_skills/__demo` 与技能说明文档。
5. 完成构建验证，并验证 `__demo` 不会被自动注册。
6. 记录执行变更总结并归档。

## 技术选型

- 将 `__` 前缀视为内部或模板目录约定，不参与自动装载。
- `__demo` 保持结构完整，便于用户直接复制改名后投入使用。
- 示例遵循当前最新规范：仅使用 `file` 字段，并通过扩展名判断静态内容或 Lua 生成器。

## 验收标准

1. `lua_skills` 加载器会跳过 `__` 前缀目录
2. `__demo` 已建立完整可复制模板结构
3. 文档已补充该约定与示例说明
4. `cargo build` 通过
5. 验证 `__demo` 不会出现在自动加载结果中

## 执行变更总结

### 1. 核心修复与调整概述

- 在 Lua skill 自动加载阶段增加 `__` 前缀目录过滤规则。
- 新增 `__demo` 模板技能目录，覆盖 tool、resource、resource template、prompt 与 `.lua` 生成器示例。
- 同步更新技能说明文档，明确 `__` 前缀目录属于内部模板，不参与自动加载。

### 2. 📂文件变更清单

新增：
- `runtime/lua_skills/__demo/skill.json`
- `runtime/lua_skills/__demo/main.lua`
- `runtime/lua_skills/__demo/resources/guide.md`
- `runtime/lua_skills/__demo/resources/dynamic_guide.lua`
- `runtime/lua_skills/__demo/templates/example.md`
- `runtime/lua_skills/__demo/templates/example_generator.lua`
- `runtime/lua_skills/__demo/prompts/static_prompt.md`
- `runtime/lua_skills/__demo/prompts/dynamic_prompt.lua`
- `output/lua_skills/__demo/skill.json`
- `output/lua_skills/__demo/main.lua`
- `output/lua_skills/__demo/resources/guide.md`
- `output/lua_skills/__demo/resources/dynamic_guide.lua`
- `output/lua_skills/__demo/templates/example.md`
- `output/lua_skills/__demo/templates/example_generator.lua`
- `output/lua_skills/__demo/prompts/static_prompt.md`
- `output/lua_skills/__demo/prompts/dynamic_prompt.lua`

修改：
- `src/lua_engine.rs`
- `docs/lua_skills.md`

删除：
- 无

### 3. 💻关键代码调整详情

- 在 `src/lua_engine.rs` 的 `load_from_dirs` 中增加对 `__` 前缀目录的跳过逻辑，并输出明确日志：`Internal template skipped: <name>`。
- 新增 `__demo` 模板 skill，示范：
  - `main.lua` 工具入口
  - 静态资源
  - `.lua` 动态资源
  - 静态资源模板
  - `.lua` 动态资源模板
  - 静态 prompt
  - `.lua` 动态 prompt
- 在 `docs/lua_skills.md` 中补充 `__` 前缀目录约定与 `__demo` 模板建议结构。

### 4. ⚠️遗留问题与注意事项

- `__demo` 是模板目录，不会被自动加载；如需实际启用，必须复制并改名为非 `__` 前缀目录。
- 正式运行中的服务仍需重启，新的跳过规则与 `__demo` 目录才会反映到实际运行环境中。
