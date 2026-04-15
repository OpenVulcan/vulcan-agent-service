# 任务计划：VMCP 工具冒烟测试

## 任务目标

验证 `vmcp_ast`、`vmcp_rg`、`vmcp_patch` 三个工具在当前仓库中的基本可用性，其中：

- 使用 `vmcp_ast` 进行代码结构扫描，确认能够返回函数/方法级结构信息。
- 使用 `vmcp_rg` 进行文本检索并映射回结构上下文，确认能够定位目标函数。
- 使用 `vmcp_patch` 基于临时文件完成一次函数级补丁替换，确保不影响正式业务文件。

## 执行步骤

1. 检查仓库内 `docs/plan/` 与 `docs/completed/` 目录状态，确认当天计划序号。
2. 创建本次测试对应的临时源码文件，文件内容应包含可被 AST 扫描、可被 RG 检索、可被 Patch 替换的函数。
3. 运行 `vmcp_ast` 对临时文件进行结构分析，记录返回的符号与行号信息。
4. 运行 `vmcp_rg` 对临时文件中的特征文本进行检索，验证结构映射结果。
5. 运行 `vmcp_patch` 对临时文件中的目标函数进行替换，验证补丁工具生效。
6. 读取补丁后的文件内容并执行必要校验，确认修改结果符合预期。
7. 对照本计划逐项验收，并在文末追加执行变更总结。
8. 验证通过后，将本计划迁移到 `docs/completed/20260414/` 目录。

## 技术选型

- 临时测试文件选择 TypeScript，便于 AST、正则检索以及函数级替换的结果观察。
- 临时文件放置在仓库内独立测试目录，避免污染现有业务模块。
- 文件写入采用补丁方式完成，函数替换采用 `vmcp_patch` 完成，以贴近真实工具使用流程。

## 验收标准

- `vmcp_ast` 成功返回临时文件中的函数结构与对应行号。
- `vmcp_rg` 能够根据指定关键字定位到临时文件中的目标函数上下文。
- `vmcp_patch` 成功替换目标函数，且补丁前后差异清晰可验证。
- 临时测试过程不修改任何正式业务逻辑文件。
- 最终计划文件包含完整的执行变更总结并完成归档。

## 验证结果

1. `vmcp_ast` 已成功扫描临时 TypeScript 文件，并返回接口 `VmcpTestPayload` 以及函数 `buildGreeting`、`renderGreeting` 的结构与行号信息。
2. `vmcp_rg` 已通过关键字 `vmcp-rg-anchor` 成功命中 `renderGreeting` 函数，并映射回对应函数上下文。
3. `vmcp_patch` 已在临时文件中成功替换 `buildGreeting` 函数，将返回文案改为包含 `[patched]` 后缀，补丁结果已完成读取核验。
4. 全流程仅操作临时测试文件，未修改任何正式业务模块；验证完成后已删除临时文件，仓库未残留测试代码。

## 执行变更总结

### 1. 核心修复与调整概述

本次工作完成了 `vmcp_ast`、`vmcp_rg`、`vmcp_patch` 三项工具的冒烟验证。通过先创建临时 TypeScript 文件，再执行结构扫描、正则检索映射和函数级补丁替换，确认三个工具在当前仓库环境下均可正常工作。测试结束后已主动清理临时文件，避免对代码库造成干扰。

### 2. 📂文件变更清单

- 新增：`docs/plan/20260414-22-VMCP_TOOL_SMOKE_TEST.md`
- 新增后删除：`tmp/vmcp-tool-test.ts`
- 后续归档目标：`docs/completed/20260414/22-VMCP_TOOL_SMOKE_TEST.md`

### 3. 💻关键代码调整详情

- 在临时文件中定义了 `VmcpTestPayload` 接口以及 `buildGreeting`、`renderGreeting` 两个函数，用于承载 AST、RG、Patch 的联合测试。
- 使用 `vmcp_ast` 确认函数结构与行号，证明结构分析工具可识别 TypeScript 代码符号。
- 使用 `vmcp_rg` 通过字符串锚点 `vmcp-rg-anchor` 定位 `renderGreeting` 函数，证明文本检索与结构回溯链路正常。
- 使用 `vmcp_patch` 将 `buildGreeting` 函数替换为带有中间变量 `decoratedGreeting` 的版本，并把结果文案修改为 `Hello, {name} [patched]`，证明函数级补丁成功生效。

### 4. ⚠️遗留问题与注意事项

- 本次仅验证了 TypeScript 临时文件场景，尚未覆盖更复杂的 Rust、多文件跨模块或泛型/宏等结构。
- `vmcp_patch` 属于函数级替换工具，实际使用时仍应优先确保选择器足够精确，以避免命中同名函数的歧义场景。
