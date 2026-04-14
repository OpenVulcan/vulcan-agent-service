## 任务目标

为当前新增的 skill 调试能力补充专门的中英文分离文档，采用 `_cn` 与 `_en` 命名，系统说明 `--call-tools` 调试模式的用途、调用方式、参数格式、常见示例与排障方式，方便后续独立调试 Lua skill / tool。

## 执行步骤

1. 梳理当前 `--call-tools` 调试入口与 `vmcp-rg` / `vmcp-ast` 的实际用法。
2. 设计文档命名与落点，新增中文与英文两份独立文档。
3. 在文档中补充：
   - 功能目的
   - 命令格式
   - JSON 参数传递方式
   - 常见示例
   - 调试建议与常见问题
4. 视需要在现有 `docs/lua_skills.md` 中加入引用入口，方便检索。
5. 自检文档内容与命令示例，补充执行变更总结并归档。

## 技术选型

- 文档采用独立 Markdown 文件，按 `_cn` / `_en` 后缀分离。
- 示例优先使用仓库当前已经可运行的 `output/debug/vulcan-mcp.exe --call-tools ...` 调试方式。
- 中文文档面向当前项目维护者，英文文档保持与命令行行为一致，便于对外说明或后续共享。

## 验收标准

1. 至少新增两份独立文档，分别对应中文与英文版本。
2. 文档文件名包含 `_cn` / `_en` 后缀。
3. 文档明确说明 `--call-tools` 的完整使用方式与调试场景。
4. 文档包含至少一个 `vmcp-ast` 或 `vmcp-rg` 的可直接参考示例。
5. 如有目录索引文档，则已补充入口引用。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 新增了 `skill` 调试专用中英文文档，分别说明 `--call-tools` 调试模式的用途、命令格式、参数写法、示例与排障方法。
- 在 `docs/lua_skills.md` 中补充了调试文档入口，方便后续统一查找。

### 2. 📂文件变更清单

新增：
- `D:\projects\vulcan-mcp-client\docs\skill_debugging_cn.md`
- `D:\projects\vulcan-mcp-client\docs\skill_debugging_en.md`
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-12-SKILL_DEBUGGING_DOCUMENTATION.md`

修改：
- `D:\projects\vulcan-mcp-client\docs\lua_skills.md`

删除：
- 无

### 3. 💻关键代码调整详情

1. 中文文档 `docs/skill_debugging_cn.md`
   - 详细说明 `--call-tools` 的调试目标与适用范围；
   - 提供 PowerShell 下的 JSON 参数传递写法；
   - 补充 `vmcp-rg`、`vmcp-ast` 的可直接参考命令；
   - 增加常见问题与调试建议。

2. 英文文档 `docs/skill_debugging_en.md`
   - 与中文文档保持结构对应；
   - 方便对外说明或跨语言协作时直接引用。

3. `docs/lua_skills.md`
   - 增加调试文档入口；
   - 简要说明当前仓库已支持 `--call-tools <tool_name> [json_arguments]` 调试模式。

### 4. ⚠️遗留问题与注意事项

1. 当前文档聚焦 `--call-tools` 本地调试模式，不覆盖 HTTP / gRPC / Streamable MCP 联调细节。
2. 文档示例默认基于 Windows PowerShell 环境编写；若后续需要，也可再补充 Bash / Linux 示例。
