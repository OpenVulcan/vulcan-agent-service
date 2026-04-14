## 任务目标

为 Lua skill 体系增加对以下能力的注册与加载支持，并以 `codeview_ast` 为示例完成一套可验证实现：

1. 注册提示词（prompts）
2. 注册资源（resources）
3. 注册资源模板（resource templates）
4. 增加对应的说明节点
5. 支持提示词文件
6. 支持模板参数计算文件
7. 完成示例技能配置与实际验证

## 执行步骤

1. 梳理当前 Lua skill 元数据结构、加载逻辑与 `McpServer` 中的 tools/resources/prompts 注册链路。
2. 设计 skill 目录约定与元数据扩展方案，明确：
   - 资源定义文件
   - 提示词定义文件
   - 模板参数计算文件
   - 说明节点文件
3. 改造 Rust 侧 skill 元数据解析与加载逻辑，使其能够把 skill 附带的 prompts/resources/templates 注册到 MCP 服务中。
4. 改造 `McpServer` 资源与 prompts 读取逻辑，使其支持 skill 级静态内容与模板展开。
5. 在 `codeview_ast` 技能下增加一套示例文件，作为：
   - 工具使用说明资源
   - 工具使用 prompt 模板
   - 参数化模板示例
6. 进行本地验证，确保：
   - `resources/list`
   - `resources/read`
   - `resource_templates/list`
   - `prompts/list`
   - `prompts/get`
   能看到并读取示例内容。
7. 记录执行变更总结并归档。

## 技术选型

- 继续沿用 skill 目录驱动方式，不引入独立数据库存储这类元信息。
- 静态资源与提示词优先采用文件声明，参数化模板通过独立 Lua/脚本文件计算参数与展开结果。
- 以 `codeview_ast` 作为首个落地示例，方便同时验证工具与提示词/资源协同效果。

## 验收标准

1. Lua skill 可以注册 prompts/resources/resource templates。
2. skill 目录存在清晰的约定与示例文件。
3. `codeview_ast` 有一套实际可读、可列举、可展开的示例。
4. 相关 MCP 接口可成功读取这些新增能力。
5. 完成中文记录并归档。

## 补充约束

1. skill 不应强制要求注册 tool，应允许仅注册 prompts/resources/templates，或任选其一组合。
2. skill 初始化脚本不再使用单一 `init_script` 字段，需支持在配置中分别声明 `ps1` 与 `sh`，并由宿主根据当前系统自动选择。

## 执行变更总结

### 1. 核心修复与调整概述

- 扩展 Lua skill 元数据模型，新增 skill 级 `resources`、`resource_templates`、`prompts` 注册能力。
- 将 skill 加载模型从“必须注册 tool”调整为“tool/prompts/resources/templates 可任意组合”，支持纯文档型或纯提示词型 skill。
- 将初始化脚本配置升级为 `init_scripts.ps1` / `init_scripts.sh` 双通道，并保留旧版 `init_script` 兼容回退。
- 为 `codeview_ast` 增加说明资源、提示词文件、资源模板与模板参数计算 Lua 文件，并完成端到端协议验证。

### 2. 📂文件变更清单

新增：
- `runtime/lua_skills/codeview_ast/resources/guide.md`
- `runtime/lua_skills/codeview_ast/prompts/first_pass.md`
- `runtime/lua_skills/codeview_ast/templates/example.md`
- `runtime/lua_skills/codeview_ast/templates/example_params.lua`
- `output/lua_skills/codeview_ast/resources/guide.md`
- `output/lua_skills/codeview_ast/prompts/first_pass.md`
- `output/lua_skills/codeview_ast/templates/example.md`
- `output/lua_skills/codeview_ast/templates/example_params.lua`

修改：
- `src/lua_skill.rs`
- `src/lua_engine.rs`
- `src/server.rs`
- `runtime/lua_skills/codeview_ast/skill.json`
- `output/lua_skills/codeview_ast/skill.json`

删除：
- 无

### 3. 💻关键代码调整详情

- 在 `src/lua_skill.rs` 中新增 skill 级资源、模板、提示词元数据结构，并为 `tool_name`、`lua_entry`、`lua_module` 提供可选化支持。
- 在 `src/lua_engine.rs` 中新增：
  - skill 级资源、模板、提示词枚举接口；
  - 基于文本模板的占位符替换能力；
  - 基于 Lua 辅助脚本的模板参数计算能力；
  - 非 tool skill 的加载兼容逻辑；
  - 按系统选择 `ps1/sh` 初始化脚本逻辑。
- 在 `src/server.rs` 中把 Lua skill 注册源合并到 `resources/list`、`resources/read`、`resources/templates/list`、`prompts/list`、`prompts/get` 的服务端处理链路。
- 在 `codeview_ast` skill 中增加：
  - `skill://codeview_ast/guide` 说明节点；
  - `codeview_ast_first_pass` 提示词模板；
  - `skill://codeview_ast/example/{topic}` 资源模板；
  - `templates/example_params.lua` 参数计算脚本示例。

### 4. ⚠️遗留问题与注意事项

- 当前模板替换仍是轻量级 `{{key}}` 文本替换机制，暂不支持复杂条件分支或循环模板。
- `target/debug` 下的临时验证依赖复制 `runtime/lua_skills` 到 `target/lua_skills`，这是因为当前宿主按可执行文件相对路径查找技能目录。
- 现网运行中的 `output/bin/vulcan-mcp.exe` 仍需重启，新的 skill 元数据与协议读取逻辑才会对外生效。
