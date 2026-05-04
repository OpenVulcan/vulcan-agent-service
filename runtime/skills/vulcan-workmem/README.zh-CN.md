# Vulcan WorkMem

English: [README.md](README.md)

`Vulcan WorkMem` 是一个面向 AI 编程智能体的显式 checkpoint 与恢复记忆层。

它不是自动长期记忆，也不尝试替模型判断“上下文还剩多少”。它的职责更窄也更实用：当用户、宿主、项目规则、恢复流程或交接流程明确需要保留任务状态时，WorkMem 写入紧凑 checkpoint，让智能体之后可以有选择地恢复关键上下文。

**WorkMem 帮助智能体恢复任务状态，但不会把当前对话变成记忆倾倒场。**

当前 LuaSkills 命名模型使用标准 `skill_id-entry_name` 形式，因此推荐工具名是：

- `vulcan-workmem-task-create`
- `vulcan-workmem-set`
- `vulcan-workmem-list`
- `vulcan-workmem-get`
- `vulcan-workmem-get-all`
- `vulcan-workmem-delete`
- `vulcan-workmem-task-list`
- `vulcan-workmem-task-close`

部分 MCP 客户端或宿主绑定可能会把工具暴露为下划线形式，例如 `vulcan_workmem_set`。这只是暴露层命名差异，语义仍然对应同一组 WorkMem 入口。

一句话概括：

**显式创建任务 checkpoint，写入紧凑事实，只召回需要的内容，并在任务完成后关闭任务记忆。**

## 解决什么问题

AI 编程智能体经常会在这些场景里丢失有用的工作状态：

- 对话被压缩。
- 长时间暂停后恢复任务。
- 一个任务需要跨会话、跨智能体或跨环境交接。
- 用户希望在高风险或多阶段操作前写入 durable checkpoint。
- 项目规则在当前提示词之外保存了稳定任务记忆 ID。

如果没有 WorkMem，智能体往往只能重新读文件、重新搜索、重新推断，或者依赖可能丢失关键决策的压缩摘要。这既浪费，也容易出错。

`Vulcan WorkMem` 提供一个很小但明确的记忆协议：

- 通过 LuaSkills 运行时把项目级任务节点保存到 SQLite。
- 鼓励保存紧凑事实，而不是完整日志或源码倾倒。
- 先列出 tag，再按需读取内容，避免一次性污染上下文。
- 将任务生命周期和长期 `LUASKILL_SID` 分离。
- 让记忆使用保持显式，而不是自动触发。

## 什么时候使用

适合使用 WorkMem 的情况：

- 用户明确要求使用 WorkMem。
- 项目说明中已经包含保存过的 `LUASKILL_SID`。
- 任务正在从已知 LuaSkills 托管身份恢复。
- 用户请求交接或 checkpoint 行为。
- 宿主或用户明确提示即将发生上下文压缩。

不要仅仅因为任务复杂、很长或涉及多个文件就触发 WorkMem。MCP 宿主通常无法暴露剩余模型上下文，智能体也不应该猜测上下文压力。

## 核心工具

### `vulcan-workmem-task-create`

创建或恢复一个项目级 WorkMem 任务。

在明确需要记忆的任务开始时使用。如果已有 `LUASKILL_SID`，通过 `LUASKILL_SID` 传入；只有在确实需要生成新的公开身份时才省略它。

使用公开身份的 create 成功后，智能体都必须在对话中显著告知用户当前 `LUASKILL_SID`，避免上下文压缩后丢失。宿主管理模式下，宿主可以注入并脱敏 `LUASKILL_SID`；智能体不应询问、打印或保存原始托管身份。

### `vulcan-workmem-set`

写入紧凑任务节点。

适合保存文件摘要、关键决策、风险、进度、待办、验证摘要和稳定恢复点。不要保存完整源码、完整命令日志或冗长叙述。

支持的节点类型：

- `note`
- `file_summary`
- `decision`
- `todo`
- `progress`
- `risk`
- `checkpoint`
- `tool_result`

### `vulcan-workmem-list`

列出已保存节点 tag，不展开完整内容。

恢复任务或上下文压缩后应先调用它，让智能体选择需要召回的 tag，而不是直接把所有内容倒入上下文。

### `vulcan-workmem-get`

读取指定 tag；省略 `tags` 时返回最多 8 条最新紧凑节点预览。

这是 `list` 之后的常规召回路径。

### `vulcan-workmem-get-all`

读取一个任务下的全部节点。

仅用于完整恢复、交接、审计，或用户明确要求读取全部记忆的情况。

### `vulcan-workmem-delete`

删除一个任务下的指定过期 tag。

### `vulcan-workmem-task-list`

列出一个 `LUASKILL_SID` 下已有的任务名。

适用于已知 ID 但不知道任务名的情况。

### `vulcan-workmem-task-close`

关闭一个任务并移除任务级节点。

关闭最后一个任务时可能清理内部空身份行，但不会使已保存的长期 `LUASKILL_SID` 失效。

## 工作流

1. 需要使用 WorkMem 时，先读取 WorkMem help flow：

```text
skill=vulcan-workmem
flow=main
```

2. 用 `vulcan-workmem-task-create` 启动或恢复一个显式记忆任务。
3. 告知用户当前公开 `LUASKILL_SID`。
4. 用 `vulcan-workmem-set` 保存 durable checkpoints。
5. 恢复时先调用 `vulcan-workmem-list`，再按需调用 `vulcan-workmem-get`。
6. 仅在完整恢复或用户明确要求时使用 `vulcan-workmem-get-all`。
7. 任务完成后用 `vulcan-workmem-task-close` 关闭任务记忆。

## 仓库说明

本仓库是 `vulcan-workmem` LuaSkill 包的独立源码仓库，对应 LuaSkills 运行时使用的发布包：

- `runtime/`：LuaSkill 工具入口与共享 WorkMem 运行时逻辑
- `help/`：面向 AI 的严格 help flow
- `attachments/`：供宿主导入的 Codex skill 工作流附件
- `overflow_templates/`：预留的本地 overflow 模板目录
- `resources/`：预留资源目录
- `licenses/`：第三方声明
- `scripts/`：校验、打包与发布辅助脚本

本仓库不再作为 demo skill 维护，而是 `vulcan-workmem` 的发布源。发布会生成标准 LuaSkill 产物：

- `vulcan-workmem-v{version}-skill.zip`
- `vulcan-workmem-v{version}-checksums.txt`
- `vulcan-workmem-v{version}-source.yaml`

zip 内部顶层目录必须是运行时 skill 名称：

```text
vulcan-workmem/
```

## 依赖与产物

`dependencies.yaml` 声明运行时依赖。当前 `vulcan-workmem` 不声明外部 tool、Lua 或 FFI 依赖。

WorkMem 依赖 LuaSkills 宿主运行时提供的 SQLite 能力。

本地校验：

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

可选 source metadata：

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

默认生成的 metadata 指向 `LuaSkills/vulcan-workmem` GitHub Release 资产；如需其他分发渠道，可传入 `--base-url`。GitHub release workflow 会将该 metadata 与 zip、checksums 资产一起生成并上传。

## 发布流程

发布由 tag 驱动。推送匹配 `v*` 的 tag 会触发 release workflow，且 tag 版本必须与 `skill.yaml.version` 一致。

推荐本地发布步骤：

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py --emit-source-yaml
.\scripts\tag_release.ps1 0.1.1
```

Unix-like shell：

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py --emit-source-yaml
./scripts/tag_release.sh 0.1.1
```

## 一句话总结

**普通记忆试图记住一切，`Vulcan WorkMem` 只记住能帮助智能体恢复、交接或应对上下文压缩的显式 checkpoint。**

**WorkMem 不是自动记忆，而是面向智能体时代的任务恢复基础设施。**
