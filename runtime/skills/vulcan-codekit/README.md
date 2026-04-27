# Vulcan CodeKit

`Vulcan CodeKit` 的核心定位不是“又一个 AST 工具”，而是把代码结构转换成 Agent 能直接推理、继续执行的上下文。

**传统 AST 工具帮助人导航代码，CodeKit 帮助 Agent 理解代码。**

它不是另一个把 `grep`、`AST`、`LSP` 胶起来的普通工具包，而是一层 Agent-native 的代码理解中间层：

当前 LuaSkills 新命名采用 `skill_id-entry_name` 的 canonical 形式，因此推荐直接使用：

- `vulcan-codekit-ast-tree`
- `vulcan-codekit-ast-detail`
- `vulcan-codekit-rg`
- `vulcan-codekit-markdown-menu`
- `vulcan-codekit-node-source`
- `vulcan-codekit-patch`

在部分 MCP 客户端或宿主绑定里，工具名可能会被转写成下划线形式，例如 `vulcan_codekit_ast_tree`。这只是暴露层命名差异，语义上仍对应同一组 CodeKit 入口。

它更像一层专门给 Agent 和高级开发工作流准备的“结构化代码理解协议”：

- 先用结构化地图理解项目
- 再用符号视图理解文件
- 再用文本锚点反查 owner context
- 最后在明确结构边界后安全修改

如果你做过大型仓库开发，你会很清楚，真正浪费时间的通常不是“打字”，而是：

- 不知道该看哪个目录
- 搜到了关键字，但不知道归谁负责
- 打开了一个 4000 行文件，然后开始滚动迷路
- 看了一堆源码，最后才发现改动点根本不在这里

`Vulcan CodeKit` 的目标很直接：

**让代码理解从“摸黑搜索”升级成“先看结构图，再施工”。**

## 这东西到底解决什么问题

传统工具当然都很强：

- `grep` 很快
- `LSP` 很准
- `AST` 很结构化
- IDE 也很好用

但它们大多是给“人类自己一点点探索”准备的。人类有 IDE 视野、长期项目记忆和空间导航能力，所以传统工具更偏向提供跳转、补全、诊断、引用这类点状能力。

而在 Agent 场景下，真正缺的不是更多原始信息，而是更高密度、更可推理、更适合继续执行的结构化中间结果。Agent 需要先建立项目地图，再判断文件职责、符号归属、阅读优先级和修改边界。

`Vulcan CodeKit` 解决的正是这层空缺：

- 它不只是告诉你“哪一行命中了”
- 它更关心“这行属于哪个函数、哪个 `impl`、哪个结构上下文”
- 它不只是给你文件列表
- 它更关心“这个目录里哪些文件最值得继续往下看”
- 它不只是展开源码
- 它更关心“先把结构轮廓抽出来，再决定读哪里、改哪里”

一句话：

**它面向的不是“手工点点点式浏览”，而是“可编排、可推理、可继续执行”的代码理解流程。**

换句话说，CodeKit 并不是把原始 AST 直接倒给模型。原始 AST 节点太细、噪声太高、缺少任务导向；CodeKit 会把它压缩成更适合 Agent 消费的目录地图、符号骨架、owner context 和安全 patch 目标。

## 为什么它比普通搜索更像下一代开发基础设施

因为它的输出不是给人看热闹，而是给 Agent 继续干活用的。

这意味着它天然适合：

- AI Coding Agent
- MCP Tooling
- 自动化代码审查
- 结构化代码导航
- 基于上下文预算的精确阅读
- 大仓库按需分析
- 安全函数级替换

也就是说，它不是简单替代：

- `grep`
- `rg`
- `ctags`
- 某个 IDE 面板

它更像把这些能力重新组织成一套适合 Agent 消费的工作流协议。

## 核心能力

### `vulcan-codekit-ast-tree`

先看地图，再看细节。

它会返回目录级分组视图，并给出每个候选文件的压缩指标，例如：

- 行数
- 类型数量
- `impl` 数量
- 函数数量
- 方法数量
- 顶层符号摘要

适合场景：

- 我不知道项目结构
- 我不知道该从哪个目录切入
- 我只想快速建立“代码地图”

### `vulcan-codekit-ast-detail`

给定明确文件后，不直接把全文砸你脸上，而是先展开结构骨架。

它会返回：

- 顶层 `struct` / `enum` / `type`
- `impl` 块
- 方法列表
- 函数签名
- 行号范围

适合场景：

- 超长文件结构预读
- 判断模块职责
- 精确定位改动边界
- 在真正读源码之前先看骨架

### `vulcan-codekit-rg`

这是整个工具组里最容易让人上头的部分。

普通搜索告诉你：

- “关键词出现在第几行”

`vulcan-codekit-rg` 告诉你：

- “关键词出现在第几行”
- “这行属于哪个函数”
- “这个函数属于哪个 `impl` 或哪个结构体上下文”

这意味着：

- 一个日志
- 一个错误串
- 一个函数名
- 一个配置名
- 一个协议字段

都可以迅速反查回真正的 owner。

对大型仓库来说，这不是小优化，而是效率量级差。

### `vulcan-codekit-markdown-menu`

先看文档标题树，再决定读正文。

它适合：

- 帮 Agent 快速定位正确文档
- 给大文档树做导航
- 防止一开始把整堆 Markdown 正文塞进上下文

### `vulcan-codekit-node-source`

当 `ast-detail` 或 `rg` 已经确认目标函数/方法后，按结构 selector 直接取回一个或多个节点的完整源码，支持跨文件批量读取。

它会返回：

- 命中的文件
- selector 数量
- 每个 selector
- 函数/方法签名
- 行号范围
- 完整节点源码
- 每个节点的 `ok` / `missing` / `ambiguous` / `duplicate` / `skipped` / `error` 状态
- `node_hash` 与 `file_hash`
- `overflow_mode: truncate`

适合：

- patch 前精读当前实现
- review 一个或多个 owner 函数而不是整文件
- 避免为了拿函数正文退回全文读取

节点读取统一使用 `nodes[]`：

- 每个节点项都携带自己的 `file` 与 `selector`
- 同文件多节点时重复同一个 `file`
- 如果更紧凑，也可以在单个节点项的 `selector` 中按行写多个 selector
- 跨文件多节点直接在不同节点项中写不同 `file`

它会部分成功返回，不会因为某个 selector 未命中、歧义、文件不存在或 selector 格式错误就丢掉所有已成功提取的节点；单节点问题会以 `status: error` 和 `node_index` 标出。
默认最多处理 20 个节点，重复命中同一节点会标记为 `duplicate`，超过上限的请求会标记为 `skipped`。

### `vulcan-codekit-patch`

当目标已经明确到函数级别后，用结构化方式替换一个或多个函数/方法。

它不是“随便文本替换”，而是围绕 AST 目标做完整函数/方法替换：先用 selector 定位目标，再写入替换内容，最后重新扫描 AST 并拒绝引入解析错误节点的结果。批量模式下默认 `atomic=true`，任一 patch 未命中、歧义、stale、replacement 不是完整函数或同文件范围重叠，整批都会在写入前被拒绝。

适合：

- 明确 owner 后的单个或多个整函数替换
- handler/helper/test 一次性修复
- 避免大文件中手工行号漂移
- 让函数级改动更可控

边界也很明确：

- 不用于零散局部文本替换
- `replacement` 必须是完整函数或方法源码
- 批量输入使用 `patches = [{ file, selector, replacement }, ...]`
- 可传入 `expected_node_hash`、`expected_source_hash`、`expected_file_hash`、`expected_range` 做 stale check
- 成功结果会区分 `previous_node_hash` 与 `new_node_hash`，后续 stale check 应使用 `new_node_hash`
- stale 拒绝会返回对应的 expected/actual 诊断字段，便于调用方判断当前源码状态
- selector 如果命中多个候选，会返回候选而不是盲目修改

## 一套更适合 Agent 的代码工作流

在 `Vulcan CodeKit` 里，推荐路径通常不是：

1. 先全文搜索
2. 到处开文件
3. 一边滚动一边猜

而是：

1. `ast-tree` 建图
2. `ast-detail` 看骨架
3. `rg` 用锚点反查 owner
4. `node-source` 获取精确节点源码
5. `patch` 批量结构化替换

也就是：

**先收缩问题空间，再阅读；先确认结构归属，再动手。**

这套方式的价值，在以下场景尤其明显：

- 陌生仓库
- 大型仓库
- 多层模块化项目
- 单文件超长
- Rust / Go / TypeScript 这类结构边界很重要的代码库
- Agent 需要节省上下文预算

## 一个非常现实的对比

在一次真实对比测试中，同样让 AI 分析 Codex 源代码，完成“理解完整项目，并从中找到一个问题”的任务：

| 场景 | 总用时 | API 交互 | 工具调用 |
| --- | ---: | ---: | ---: |
| 有 CodeKit AST 工具 | 约 4 分钟 | 11 次 | 10+ 次 |
| 没有 CodeKit AST 工具 | 约 20 分钟 | 54 次 | 110 次 |

差异并不只是“搜索快一点”。没有 CodeKit 时，Agent 往往需要反复执行：

- 用 `rg` 搜到几个命中
- 逐个开文件
- 不确定 owner
- 来回切换
- 反复滚动
- 最后才定位到真正入口

有 CodeKit 时，流程会变成：

- 先知道目录和候选文件
- 再知道文件里的结构轮廓
- 再知道某个关键字属于哪个函数、哪个 `impl`
- 最后只读真正相关的几段代码

这类提升，很多时候不是“快一点”，而是：

**从手工拼图，变成结构化导航。**

更重要的是，它减少了上下文污染。Agent 不再为了建立项目地图而吞入大量无关源码，后续判断也更干净。

## 这不是另一个 grep 包装层

如果你只是想搜索文本，那很多工具都能做。

`Vulcan CodeKit` 真正的不同点在于，它把代码理解拆成了几个更适合自动化执行的阶段：

- 地图阶段
- 结构阶段
- owner 定位阶段
- 安全替换阶段

也正因为这样，它才更适合作为：

- Agent Runtime 的基础设施
- MCP 平台的高级代码工具层
- IDE / AI 助手的结构化代码导航后端
- 自动化 review / patch 工作流的一部分

## 为什么我们对它很有信心

因为真实开发里最贵的不是 CPU，不是 AST，不是搜索速度，而是：

- 上下文预算
- 注意力
- 方向感
- 减少误判

`Vulcan CodeKit` 干的事情，就是把这些最贵的东西省下来。

它让 Agent 少看无关代码，少改错地方，少在巨型文件里迷路，少在项目结构里乱撞。

这不是体验层的小修小补，而是代码开发工作流的一个结构性升级。

## 适合谁

- 正在做 AI Coding Agent 的团队
- 想给本地 Agent 增强代码理解能力的平台
- 想在 MCP 工具体系里补强代码分析层的开发者
- 想把代码搜索升级成“结构化搜索”的工程团队
- 正在处理大仓库、多模块、长文件的高级开发者

## 当前包含的工具

- `vulcan-codekit-ast-tree`
- `vulcan-codekit-ast-detail`
- `vulcan-codekit-rg`
- `vulcan-codekit-markdown-menu`
- `vulcan-codekit-node-source`
- `vulcan-codekit-patch`

## 独立仓库说明

当前仓库是 `vulcan-codekit` LuaSkill 的独立源码仓库，内容对应 LuaSkills 运行时中的正式 skill 包：

- `runtime/`：LuaSkill 工具入口与共享运行时代码
- `rules/`：按语言拆分的 ast-grep 结构匹配规则
- `help/`：严格帮助流与各工具说明
- `skills/`：Codex 技能说明与 Agent 使用指引
- `ast-grep-ffi/`：基于 Rust 的 ast-grep FFI 动态库项目
- `scripts/`：skill 包与 FFI release 产物的校验、打包脚本

仓库不再作为 demo skill 维护，而是作为 `vulcan-codekit` 的发布源。发布时会生成两类产物：

- LuaSkill 包：包含 `runtime/`、`rules/`、`help/`、`skills/`、`dependencies.yaml` 等运行所需文件
- FFI 组件包：包含平台对应的 `vulcan_codekit_ast_grep_ffi` 动态库

## 依赖与发布产物

`dependencies.yaml` 负责声明运行时依赖。当前依赖分为两类：

- `rg`：仍作为工具依赖提供文本搜索与 Markdown 文件枚举能力
- `ast-grep-ffi`：作为 FFI 依赖提供 AST 结构扫描、结构匹配和 patch 校验能力

当前 `ast-grep` 不再通过原始 CLI 调用，而是由 `ast-grep-ffi/` 构建出的动态库承载。Lua 运行时代码会从 LuaSkills 注入的 FFI 依赖目录加载对应平台的动态库；本地开发时也会回退查找 `ast-grep-ffi/target/release` 与 `ast-grep-ffi/target/debug`。

当前 release workflow 只构建并发布以下平台的 FFI 组件：

- `macos-arm64`
- `macos-x64`
- `linux-arm64`
- `linux-x64`
- `windows-x64`

暂不发布 `windows-arm64` 组件；对应平台也未在 `dependencies.yaml` 中声明。新增平台时需要同时更新 release matrix、`dependencies.yaml`、校验脚本和 README。

## 发布流程

本仓库遵循 LuaSkills 的 GitHub Release 安装规则。技能包就是标准 LuaSkill 包，发布资产名称为：

- `vulcan-codekit-v{version}-skill.zip`
- `vulcan-codekit-v{version}-checksums.txt`

压缩包内部的顶层目录必须是运行时技能名：

- `vulcan-codekit/`

`ast-grep-ffi` 不打进技能包本体，而是由 `dependencies.yaml` 通过 GitHub Release 依赖安装。当前准确的 Release 仓库地址是：

```yaml
repo: LuaSkills/vulcan-codekit
```

GitHub Actions 中的 `Release Vulcan CodeKit LuaSkill` 支持手动运行。运行时填写 `version`，例如 `v0.1.0`，然后按需选择：

- `build_luaskill=on/off`：是否构建并上传 LuaSkill 技能包
- `luaskill_runner`：技能包构建 runner
- 各平台 `*_runner`：对应 FFI 平台 runner，设为 `off` 即跳过该平台

LuaSkill 技能包构建和 FFI 原生组件构建可以分离执行；只要 `version` 相同，所有启用的产物都会上传到同一个 GitHub Release。运行时安装 FFI 组件时，LuaSkills 依赖管理器会根据 `dependencies.yaml` 中的 `version`、`repo` 与平台 `asset_name` 解析同一个 Release 下的对应资产。

Rust FFI 依赖许可证报告由 `cargo-deny` 自动生成：

```powershell
python .\scripts\generate_cargo_deny_notices.py
```

生成结果写入 `THIRD_PARTY_LICENSES.md`，CI 会在 `ast-grep-ffi/` 下通过 `cargo deny check -c deny.toml --exclude-dev licenses` 检查许可证策略，并校验报告是否仍然匹配当前依赖图。

## 一句话总结

**如果说传统代码工具是在回答“文本在哪里”，那么 `Vulcan CodeKit` 更关心“结构在哪里、owner 是谁、下一步应该读哪里、改哪里”。**

**传统 AST 工具帮助人导航代码，CodeKit 帮助 Agent 理解代码。**

这就是它为什么不只是一个工具集，而更像一层给 Agent 时代准备的代码理解基础设施。
