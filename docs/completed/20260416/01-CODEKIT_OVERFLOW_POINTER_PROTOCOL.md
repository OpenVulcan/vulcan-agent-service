# 任务目标

为 `vulcan-codekit` 的大结果返回建立统一的 overflow 协议：当 AST/RG/Tree 结果超过客户端安全阈值时，不再返回任何残缺正文，而是改为输出原始文件地址与安全分块读取计划，确保后续由 LLM 或客户端读取原始文件时不会受到 MCP/JSON 包装截断污染。

# 执行步骤

1. 检查当前 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-rg` 的超限处理逻辑，确认现有缓存与内联返回路径。
2. 设计统一的 overflow 判定规则，采用客户端名义阈值的安全比例作为实际内联限制。
3. 调整正文生成流程，在构建 Markdown 内容时同步记录逐行 UTF-8 字节数与累计分布。
4. 实现统一的 raw-file pointer 返回结构，超限时仅返回文件地址、总行数与安全读取 chunk 计划，不再返回正文片段。
5. 保证写入缓存的原始文件只包含纯正文内容，不包含任何警示头或提示信息。
6. 为极端大结果场景增加边界策略：当 chunk 数量过大时，明确提示应缩小范围，而不是继续硬读。
7. 对 `codekit-ast-tree`、`codekit-rg`、`codekit-ast-detail` 分别进行校验，确认未超限与超限两种路径都符合预期。
8. 追加执行变更总结并归档计划文件。

# 技术选型

- 使用统一的共享长度策略作为客户端预算来源，但实际输出采用额外的安全阈值比例控制。
- 以逐行字节统计为基础切 chunk，确保 chunk 边界稳定且可复用。
- raw file 继续使用现有缓存目录，但写入内容必须保持纯 Markdown 正文。
- overflow 返回内容优先采用 Markdown 文本指针块，便于不同客户端和模型直接消费。

# 验收标准

- 超限时不再输出任何残缺正文，只返回 raw file 指针与读取计划。
- raw file 中仅包含纯正文，不包含警示头、读取说明或其它包装信息。
- chunk 计划至少包含起始行、结束行、行数与字节数，且与原始文件内容对齐。
- `codekit-ast-tree`、`codekit-rg`、`codekit-ast-detail` 三个工具的 overflow 行为统一。
- 极端大结果场景下会明确提示先缩小范围，而不是继续提供误导性读取建议。

# 执行变更总结

## 1. 核心修复与调整概述

- 为 `codekit-ast-detail`、`codekit-ast-tree`、`codekit-rg` 新增统一的 shared overflow 协议，超限后不再返回残缺正文，而是返回 raw file 指针与逐段读取计划。
- 将超限判定改为客户端预算的 95% 安全阈值，并在生成最终文本时按逐行 UTF-8 字节数切分安全 chunk。
- 保证缓存原始文件只写纯 Markdown 正文，不再把提示头或告警信息混入文件内容。
- 同步更新 `skill.json` 与 `docs/lua_skills.md`，使外部说明与新 overflow 行为保持一致。

## 2. 📂文件变更清单

### 新增

- `runtime/lua_skills/vulcan-codekit/shared_overflow.lua`

### 修改

- `runtime/lua_skills/vulcan-codekit/main.lua`
- `runtime/lua_skills/vulcan-codekit/main_ast_tree.lua`
- `runtime/lua_skills/vulcan-codekit/main_rg.lua`
- `runtime/lua_skills/vulcan-codekit/skill.json`
- `docs/lua_skills.md`

## 3. 💻关键代码调整详情

- 在 `shared_overflow.lua` 中统一封装：
  - 95% 安全内联阈值计算
  - 逐行字节统计与 chunk 规划
  - raw file pointer Markdown 渲染
  - chunk 过多时的范围收窄提示
- `codekit-ast-detail` 改为通过共享模块完成超限处理，并在 pointer 中附带 `files_scanned/files_with_symbols/items_found` 摘要。
- `codekit-ast-tree` 改为超限后仅返回 raw file 指针与 chunk 表，不再把树正文拼回返回值。
- `codekit-rg` 改为超限后仅返回 raw file 指针与 chunk 表，并附带 `files_scanned/files_with_matches/items_found/rg_matches` 摘要。
- 文档与 prompt 文案中的“cache notice”描述更新为“raw-file pointer and chunked read plan”。

## 4. ⚠️遗留问题与注意事项

- 当前工作区仍有与本任务无关的既有修改与未跟踪目录，本次未处理。
- 本次已做真实冒烟校验：
  - `codekit-ast-tree` 超限 pointer 路径通过
  - `codekit-rg` 超限 pointer 路径通过
  - `codekit-ast-detail` 超限 pointer 路径通过
  - `codekit-ast-tree` 与 `codekit-rg` 的未超限直出正文路径通过
- 已确认写入缓存的 `.md` 文件为纯正文内容，不含 pointer 头部。
