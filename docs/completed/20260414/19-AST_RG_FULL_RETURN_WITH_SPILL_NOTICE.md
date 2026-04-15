## 任务目标

修正 `vmcp-ast` / `vmcp-rg` 当前“大结果落盘”行为与需求不一致的问题。

当前实现把“大于 10000 字节”错误处理成了“工具侧只返回精简预览”，但实际需求应为：

1. 工具结果仍然尽量全量返回；
2. 当结果较大时，仅额外补充提示消息与完整落盘文件路径；
3. 只有客户端自身发生截断时，调用方才去查看落盘文件，而不是工具主动裁剪返回内容。

## 执行步骤

1. 梳理 `vmcp-ast` 与 `vmcp-rg` 当前的 `build_inline_preview_result` / `finalize_*_result` 逻辑。
2. 移除“按 10000 字节做工具侧预览裁剪”的行为，改为保留全量 `files` 返回。
3. 保留大结果落盘 Markdown 文件、`!msg`、完整路径与大小信息。
4. 调整返回字段，移除或停用 `preview_files`、`omitted_files` 这类工具侧裁剪痕迹。
5. 通过本地验证确认：
   - 大结果返回时仍包含完整文件列表
   - 同时存在落盘 Markdown 文件与提示信息
   - 小结果仍保持原样返回

## 技术选型

- 保持大结果阈值判断继续存在，但仅用于“是否补充提示并落盘”，不再用于裁剪 `files`。
- 继续沿用当前 Markdown 落盘机制，不引入新的分页/缓存行为。
- 不做历史兼容，直接纠正为与需求一致的最终规则。

## 验收标准

1. `vmcp-ast` / `vmcp-rg` 在大结果场景下，返回内容仍为全量结果，而不是仅保留前几个文件。
2. 大结果场景下，仍会提供 `!msg` 与完整 Markdown 落盘路径。
3. 返回中不再出现误导性的 `preview_files` / `omitted_files` 工具侧裁剪痕迹。
4. 本地验证通过，确认文件数量不再被工具主动截断。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 修正了 `vmcp-ast` / `vmcp-rg` 的大结果返回策略，不再把“超过 10000 字节”错误实现成“工具侧只返回预览”。
- 当前行为调整为：结果仍全量返回，同时附带 `!msg`、完整 Markdown 落盘路径与文件大小信息。
- 移除了工具侧 `preview_files` / `omitted_files` 这类误导性字段，避免 AI 误以为这是最终完整结果。

### 2. 📂文件变更清单

#### 修改

- `runtime/lua_skills/ast-grep/main.lua`
- `runtime/lua_skills/ast-grep/main_rg.lua`

#### 新增

- `docs/plan/20260414-19-AST_RG_FULL_RETURN_WITH_SPILL_NOTICE.md`

### 3. 💻关键代码调整详情

- `runtime/lua_skills/ast-grep/main.lua`
  - 移除基于内联大小阈值的 `build_inline_preview_result` 预览裁剪逻辑。
  - 新增“附加大结果提示”逻辑，仅往完整结果上补充 `!msg`、`full_output_file` 与 `full_output_bytes`。
  - 保留 Markdown 落盘，但不再修改 `files` 内容与文件数量。
- `runtime/lua_skills/ast-grep/main_rg.lua`
  - 与 `vmcp-ast` 同步，取消工具侧预览裁剪。
  - 大结果时仍全量返回结构树结果，并附带 Markdown 落盘提示信息。

### 4. ⚠️遗留问题与注意事项

- 当前 10000 字节阈值仍然保留，但作用仅为“是否额外落盘并加提示”，不再影响结果主体。
- 本轮已做本地验证：
  - `vmcp-ast` 在 `D:/projects/vulcan-mcp-client` 场景下返回了完整 `files` 列表，不再只剩 2 个文件
  - `vmcp-rg` 也已验证为全量返回
  - `truncated` 当前保持 `false`，因为工具本身不再主动裁剪结果
