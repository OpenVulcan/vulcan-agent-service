# Vulcan File

`Vulcan File` 的核心定位不是“再包一层文件系统 API”，而是给 Agent 提供一套稳定、低噪声、可继续执行的原文文件操作协议。

**CodeKit 帮 Agent 理解代码结构，File 帮 Agent 安全处理原文文件。**

它解决的不是“有没有办法读文件”这种基础问题，而是 Agent 在真实工程任务里经常遇到的几个反复成本：

- 不想为了列文件写一段平台相关 shell
- 已经知道目标文件和行号，只想精确读取一小段
- 需要一次读取多个不相邻片段进行对比
- 想做一个很小的文本改动，但必须先看到预览
- 不希望生成物、依赖目录和忽略文件把上下文预算吃光

当前 LuaSkills 新命名采用 `skill_id-entry_name` 的 canonical 形式，因此推荐直接使用：

- `vulcan-file-list`
- `vulcan-file-read`
- `vulcan-file-edit`

在部分 MCP 客户端或宿主绑定里，工具名可能会被转写成下划线形式，例如 `vulcan_file_read`。这只是暴露层命名差异，语义上仍对应同一组 File 入口。

它更像一层专门给 Agent 和自动化工程流准备的“文本文件工作台”：

- 先用低 token 文件地图缩小候选范围
- 再用明确行号读取原文
- 再基于已确认上下文预览编辑
- 最后只在预览符合预期时写入

一句话：

**先定位文件，再读取证据；先看预览，再落盘修改。**

## 这东西到底解决什么问题

传统方式当然能完成这些事：

- `ls` 可以列文件
- `find` 可以枚举目录
- `sed` / `awk` / `PowerShell` 可以抽取行号
- shell 重定向可以改文件

但这些工具大多面向人类命令行操作。人类知道当前 shell、平台差异、编码边界和上下文风险，也能在脑子里记住刚刚看到的文件位置。

Agent 场景里真正缺的不是更多原始能力，而是更稳定的操作形状。Agent 需要的是：

- 输出足够紧凑，方便继续推理
- 行号稳定，方便后续引用和编辑
- 参数错误明确，方便自动修正调用
- 默认遵守 ignore 规则，减少上下文污染
- 编辑默认只预览，避免误写

`Vulcan File` 解决的正是这层空缺：

- 它不只是告诉你“目录里有什么”
- 它更关心“哪些候选文件值得下一步读取”
- 它不只是把全文塞给你
- 它更关心“按明确行段返回可引用的原文证据”
- 它不只是写文件
- 它更关心“先生成可审查预览，再由调用方显式确认写入”

## 它和 CodeKit 的边界

`Vulcan File` 不替代 `Vulcan CodeKit`。

两者的关系更像：

- CodeKit 负责结构理解、owner 定位、函数级源码提取和结构化 patch
- File 负责普通文本文件的发现、精确读取和小范围文本编辑

当任务需要理解函数、类、`impl`、Markdown 标题树、AST 结构或完整函数替换时，优先使用 CodeKit。

当任务已经明确到“我要看这个文件的这些行”或“我要在这个文件里做一个小文本改动”时，File 更直接、更轻量。

也就是说：

**CodeKit 用来理解结构，File 用来处理原文。**

## 核心能力

### `vulcan-file-list`

在还不知道目标文件在哪里时，先看一个低 token 的文件名地图。

它返回按目录分组的紧凑文件列表，支持文件名 glob，并默认遵守 `.gitignore`、`.ignore` 和内建高噪声目录忽略规则。

适合场景：

- 进入陌生目录前快速建立文件地图
- 已知道扩展名或文件名形状，想先筛候选文件
- 避免手写 shell 循环、排序和 ignore 处理
- 在读取正文前先降低搜索空间

典型参数：

- `path`：扫描根目录，应该尽量传最小可能目录；路径值可以包含 `${env:NAME}` 占位符
- `pattern`：文件名 glob，例如 `*.lua`、`*.md`、`Cargo.*`
- `recursive`：默认递归；只看直接子项时设为 `false`
- `noignore`：只有确实需要看生成物或被忽略目录时才设为 `true`
- `limit`：控制最多返回多少候选文件

它不是内容搜索工具。如果你手里有日志、错误串、函数名或文本锚点，应该先用 `vulcan-codekit-rg`。

### `vulcan-file-read`

当文件路径和大致行号已经明确后，用它读取精确原文。

它的核心参数是 `lines_rule`，格式是 `start,count`：

```text
5,10
25,30
```

这表示从第 5 行读取 10 行，再从第 25 行读取 30 行。多段读取会按请求顺序输出，不会擅自合并重叠区间。

它会返回：

- 文件路径
- 总行数
- 字节数
- 换行风格
- 当前展示范围
- 片段数量
- 是否因为超过文件尾部而截断

默认会保留 `L12:` 这类稳定行号前缀，便于后续 review、引用或调用 `vulcan-file-edit`。如果只需要纯文本，可以把 `numbered` 设为 `false`。

边界行为也很明确：

- `start` 与 `count` 必须是正整数
- `start` 超过文件总行数时返回参数错误
- `count` 超过文件尾部时自动截到 EOF，并在 header 中标记
- 目录路径只用于快速查看直接子项名称，递归找文件应使用 `vulcan-file-list`
- 路径值可以包含 `${env:NAME}` 占位符，工具会在访问文件系统前用 Lua `os.getenv` 展开

它不是“翻页猜文件”的工具。还不知道文本在哪里时，应先搜索或列候选文件。

### `vulcan-file-edit`

当目标文件和目标行已经确认后，用它做小范围文本编辑。

它默认只预览，不写入。只有显式传入 `apply=true` 时才会落盘。

`file` 路径可以包含 `${env:NAME}` 占位符，工具会在访问文件系统前用 Lua `os.getenv` 展开。

支持模式：

- `overwrite`：覆盖整个文件
- `append`：追加到文件尾
- `replace_range`：替换指定 1-based 行范围
- `insert_before`：插入到指定行之前
- `insert_after`：插入到指定行之后

返回结果会包含：

- 当前状态是 `PREVIEW_ONLY` 还是 `APPLIED`
- 原始行数与编辑后行数
- 原始影响范围
- 编辑后影响范围
- 面向操作的 diff 预览
- 参数错误时的明确修正提示

它刻意不做复杂结构判断。如果目标是完整函数或方法替换，应使用 `vulcan-codekit-patch`；如果需要先理解源码结构，应先使用 CodeKit。

## 一套更适合 Agent 的文件工作流

在 `Vulcan File` 里，推荐路径通常不是：

1. 用 shell 到处列目录
2. 猜一个文件打开全文
3. 复制一段脚本改文件
4. 事后再检查有没有改错

而是：

1. `list` 获取候选文件地图
2. `read` 精确读取目标行段
3. `edit` 生成预览
4. 预览正确后再次 `edit apply=true`

也就是：

**先把文件操作变成可审查的步骤，再让 Agent 继续执行。**

这套方式的价值，在以下场景尤其明显：

- 跨平台开发环境
- 大仓库中的轻量文件导航
- README、配置、脚本和普通文本的小改动
- 需要保留精确行号证据的 review 或修复任务
- Agent 需要节省上下文预算
- 不希望默认写文件带来不可见副作用

## 一个非常现实的对比

没有 `Vulcan File` 时，Agent 常常会退回到临时命令：

- `Get-ChildItem` / `find`
- `Select-String` / `grep`
- `sed -n`
- `awk`
- 临时脚本写文件

这些命令不是不能用，而是每次都要重新决定：

- 当前平台是什么
- 是否需要递归
- 是否该遵守 ignore
- 输出会不会太长
- 行号格式能否直接引用
- 编辑前有没有足够清晰的预览

有 `Vulcan File` 时，这些问题被固定成三个稳定入口：

- 文件候选：`list`
- 原文证据：`read`
- 小文本修改：`edit`

这不是把 shell 能力简单换个名字，而是把常见文件动作整理成 Agent 更容易安全调用的协议。

## 适合谁

- 正在构建 AI Coding Agent 的团队
- 想给 MCP 工具体系补充基础文件操作层的平台
- 需要跨平台稳定文件读写行为的本地 Agent Runtime
- 经常处理 README、配置、脚本和资源文件的自动化工作流
- 希望把“先预览、再写入”作为默认编辑纪律的工程团队

## 当前包含的工具

- `vulcan-file-list`
- `vulcan-file-read`
- `vulcan-file-edit`

## 独立仓库说明

当前仓库是 `vulcan-file` LuaSkill 的独立源码仓库，内容对应 LuaSkills 运行时中的正式 skill 包：

- `runtime/`：LuaSkill 工具入口
- `help/`：严格帮助流与各工具说明
- `overflow_templates/`：预留的本地超限模板目录
- `resources/`：预留资源目录
- `licenses/`：第三方声明
- `scripts/`：校验、打包和发布辅助脚本

仓库不再作为 demo skill 维护，而是作为 `vulcan-file` 的发布源。发布时会生成标准 LuaSkill 包：

- `vulcan-file-v{version}-skill.zip`
- `vulcan-file-v{version}-checksums.txt`

压缩包内部的顶层目录必须是运行时技能名：

- `vulcan-file/`

## 依赖与发布产物

`dependencies.yaml` 负责声明运行时依赖。当前 `vulcan-file` 不声明外部 tool、Lua 或 FFI 依赖。

这意味着它的运行时能力完全由 LuaSkills 宿主提供的文件系统、路径和运行时接口承载，不需要额外安装 `rg`、原生动态库或第三方 Lua 包。

本地校验：

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

可选 source metadata：

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

生成的 metadata 默认指向 `LuaSkills/vulcan-file` 对应版本的 GitHub Release 资产；如需其他分发渠道，可以传入 `--base-url`。

## 发布流程

本仓库遵循 LuaSkills 的 GitHub Release 安装规则。推送匹配 `v*` 的标签会触发发布工作流，且标签版本必须与 `skill.yaml.version` 保持一致。

推荐本地发布步骤：

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
.\scripts\tag_release.ps1 0.1.1
```

Unix-like shell：

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.1
```

## 一句话总结

**如果说 CodeKit 回答的是“结构在哪里、owner 是谁”，那么 `Vulcan File` 回答的是“文件在哪里、原文是什么、这次文本改动是否已经被预览确认”。**

**CodeKit 帮 Agent 理解代码结构，File 帮 Agent 安全处理原文文件。**

这就是它为什么不只是一个文件工具集，而是一层给 Agent 时代准备的轻量文件操作基础设施。
