## 任务目标

对 `skill` 范围内的 MCP 能力做一轮烟雾测试，重点确认 `ast-grep` 这组工具、资源、模板、提示词链路是否可用，并记录成功项、失败项与风险点。

## 执行步骤

1. 盘点当前会话与实际部署实例中 `skill` 相关的入口，重点关注 `vmcp-ast`、`vmcp-rg`、`vmcp-patch`、resources、resource templates、prompts。
2. 按 skill 能力分组设计最小可验证用例，优先覆盖工具调用链路、Markdown 导出/落盘链路、资源读取与模板生成。
3. 执行测试并记录返回结果，区分成功、失败与受限项。
4. 对失败项做初步归因，判断是会话注入问题、协议问题还是实现问题。
5. 输出结构化测试结论，并在任务结束后补充执行变更总结。

## 技术选型

- 优先使用当前会话已注入的 skill 能力直接验证；若线程未拿到注入，则转为直接对实际部署实例做本地调用验证。
- 使用最小输入进行烟雾测试，减少环境干扰。
- 对涉及文件写入的测试使用临时路径，并在完成后清理。
- 以“skill 是否可调用 + 返回是否符合预期”为主，不做重型性能验证。

## 验收标准

1. 明确列出已测试的 skill 功能分组。
2. 每组至少有一个实际调用结果，能区分成功与失败。
3. 对失败项给出初步原因分析。
4. 计划文件补充执行变更总结并归档完成。

---

## 执行变更总结

### 1. 核心修复与调整概述

- 本次未修改业务代码，范围已按用户要求收窄到 `skill` 本身，不再覆盖后续要重整的通用 MCP 能力。
- 已确认 `ast-grep` skill 在实际部署实例 `D:\\developer\\mcp_server` 中完整存在，并成功验证了 `vmcp-ast`、`vmcp-rg`、`vmcp-patch`、resource、resource template` 这几条主链路。
- 已定位两个重要现象：
  - 当前 Codex 线程没有拿到 `vmcp-ast`、`vmcp-rg`、`vmcp-patch` 的工具注入，但部署实例本身是可用的，说明“线程注入状态”和“服务端注册状态”不一致。
  - `prompts` 协议链路当前存在会话保持问题：`initialize` 后继续调用 `notifications/initialized` / `prompts/list` 时返回 `Session not found`，因此 prompt 能力未通过本轮 smoke test。

### 2. 📂文件变更清单

#### 新增

- 无

#### 修改

- `docs/plan/20260414-21-MCP_FEATURE_SMOKE_TEST.md`

#### 删除

- 无

### 3. 💻关键代码调整详情

- 本次任务未对代码实现做变更。
- 实际执行的 skill 验证结果如下：
  - `skill://ast-grep/guide`：成功，可读取 guide 内容。
  - `skill://ast-grep/example/rust`：成功，可通过 resource template 动态生成示例内容。
  - `vmcp-ast`：成功，可对单文件做 AST 结构扫描。
  - `vmcp-rg`：成功，可先 `rg` 再映射到结构树，并输出命中函数完整代码段。
  - `vmcp-patch`：成功，可在临时 Rust 文件上通过 `Demo/compute` 选择器完成完整函数替换。
  - `vmcp-ast + export_md_path`：成功，导出 Markdown 文件正常生成。
  - `vmcp-ast + workdir spill`：成功，结果超长时会在 `<workdir>/.vulcan/mcp/cache/` 下生成完整 Markdown 文件。
  - `prompts/list` / `prompts/get`：失败，初始化后继续发请求时返回 `Session not found`，当前 prompt 协议链路未通过。

### 4. ⚠️遗留问题与注意事项

- 当前线程没有直接注入 `vmcp-ast` / `vmcp-rg` / `vmcp-patch`，所以这三项是通过本地调用实际部署实例完成验证的，不是通过本线程工具注入完成的。
- `prompts` 协议链路仍未打通，是本轮唯一明确失败项，后续需要单独归纳调整。
- `skill://ast-grep/guide` 返回内容仍然包含旧版分页/缓存说明，和仓库当前实现不完全一致，说明部署实例的 skill 文案存在滞后。
