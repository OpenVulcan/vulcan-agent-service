# Vulcan File

`Vulcan File` 的核心定位不是“再包一层文件系统 API”，而是给 Agent 提供一套稳定、低噪声、可继续执行的原文文件操作协议。

**CodeKit 帮 Agent 理解代码结构，File 帮 Agent 安全处理原文文件。**

它解决的不是“有没有办法读文件”这种基础问题，而是 Agent 在真实工程任务里经常遇到的几个反复成本：

- 不想为了列文件写一段平台相关 shell
- 已经知道目标文件和行号，只想精确读取一小段
- 需要一次读取多个不相邻片段进行对比
- 想直接做一个很小的文本改动，或先拿到仅预览结果
- 不希望生成物、依赖目录和忽略文件把上下文预算吃光

当前 LuaSkills 新命名采用 `skill_id-entry_name` 的 canonical 形式，因此推荐直接使用：

- `vulcan-file-list`
- `vulcan-file-read`
- `vulcan-file-create`
- `vulcan-file-edit`
- `vulcan-file-delete`

在部分 MCP 客户端或宿主绑定里，工具名可能会被转写成下划线形式，例如 `vulcan_file_read`。这只是暴露层命名差异，语义上仍对应同一组 File 入口。

它更像一层专门给 Agent 和自动化工程流准备的“文本文件工作台”：

- 先用低 token 文件地图缩小候选范围
- 文件已存在时，再用明确行号读取原文
- 再基于已确认上下文创建新文件或执行小范围编辑
- 在需要清理文件生命周期时，建议先用 `delete no_apply=true` 预览普通文件删除
- 需要先审查时，显式传入 `no_apply=true` 获取预览

一句话：

**先定位文件，再读取证据；随后创建、编辑或删除；需要先预览时传入 `no_apply=true`。**

路径公约：

- 根级 `PWD` 用作相对 `file` 或 `path` 参数共享的项目/工作区根目录。
- 当 `PWD` 未传、为空，或不是一个已存在目录时，File 不会再回退到运行时 `cwd`；这时必须直接传绝对路径。

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
- 对高风险写入提供显式仅预览控制

`Vulcan File` 解决的正是这层空缺：

- 它不只是告诉你“目录里有什么”
- 它更关心“哪些候选文件值得下一步读取”
- 它不只是把全文塞给你
- 它更关心“按明确行段返回可引用的原文证据”
- 它不只是写文件
- 它更关心“按需生成可审查预览，并在真实写入时返回结构化结果”

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

它返回按目录分组的紧凑文件列表，支持 basename-only 文件名 glob，并默认遵守 `.gitignore` / `.ignore` 常见规则子集和内建高噪声目录忽略规则。

适合场景：

- 进入陌生目录前快速建立文件地图
- 已知道扩展名或文件名形状，想先筛候选文件
- 避免手写 shell 循环、排序和 ignore 处理
- 在读取正文前先降低搜索空间

典型参数：

- `path`：扫描根目录，应该尽量传最小可能目录；路径值可以包含 `${env:NAME}` 占位符
- `PWD`：可选的共享项目/工作区根目录。只有当它指向已存在目录时，相对 `path` 才会基于它解析；否则 `path` 必须本身就是绝对路径。不传 `path` 时会直接从 `PWD` 开始扫描。
- `pattern`：basename-only 文件名 glob，例如 `*.lua`、`*.md`、`Cargo.*`；只匹配文件名本身，不能传 `src/*.lua` 或 `**/*.md`，需要缩小目录时使用 `path`
- `recursive`：默认递归；只看直接子项时设为 `false`
- `noignore`：只有确实需要看生成物或被忽略目录时才设为 `true`；设为 `true` 会同时关闭 ignore 文件规则和内建高噪声目录忽略
- `limit`：控制最多返回多少候选文件；默认 1000，最大 100000

它不是内容搜索工具。如果你手里有日志、错误串、函数名或文本锚点，应该先用 `vulcan-codekit-rg`。

ignore 处理不是完整 Git ignore 引擎；复杂转义和部分高级否定场景不保证与 Git 完全一致。

### `vulcan-file-read`

当文件路径和大致行号已经明确后，用它读取精确原文。

它支持一个根级单文件请求，也支持最多 10 个条目的 `files` 批量读取。批量模式下，根级 `numbered` 会作为所有条目的默认值，除非某个条目单独覆盖。

如果客户端支持完整 JSON Schema，优先使用结构化 `segments` 数组：

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "segments": [
    { "start": 5, "count": 10 },
    { "start": 25, "count": 30 }
  ]
}
```

这表示从第 5 行读取 10 行，再从第 25 行读取 30 行。多段读取会按请求顺序输出，不会擅自合并重叠区间。

`lines_rule` 仍保留为不能发送数组参数的客户端提供旧版兼容，格式是 `start,count`：

```text
5,10
25,30
```

在 JSON 参数中，多段规则必须使用字符串里的 `\n` 分隔：

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "lines_rule": "5,10\n25,30"
}
```

这里的分隔符是真实换行符，不是字面字符串 `"newline"`。`segments` 与 `lines_rule` 不能同时传；只要客户端支持数组 schema，就应优先使用 `segments`。

它会返回：

- 文件路径
- 总行数
- 字节数
- 换行风格
- 当前展示范围
- 片段数量
- 是否因为超过文件尾部而截断
- 多文件批量读取时的分隔区块

默认会保留 `L12:` 这类稳定行号前缀，便于后续 review、引用或调用 `vulcan-file-edit`。如果只需要无行号前缀的正文行，可以把 `numbered` 设为 `false`；文件头和多段分隔线仍会保留。

边界行为也很明确：

- `start` 与 `count` 必须是正整数
- `segments` 必须是非空数组，且每一项都要提供正整数 `start` 与 `count`
- `start` 超过文件总行数时返回参数错误
- `count` 超过文件尾部时自动截到 EOF，并在 header 中标记
- 不传 `lines_rule` 时读取文件开头，行数来自宿主 `file_read` 预算；宿主未提供预算时默认 200 行
- `lines_rule` 中出现字面字符串 `"newline"` 会返回 `invalid_lines_rule`，需要改为 JSON 字符串中的 `\n`
- 同时传 `segments` 与 `lines_rule` 会返回 `conflicting_range_arguments`
- 同时传根级单文件参数和 `files` 会返回 `conflicting_batch_arguments`
- 目录路径只用于快速查看直接子项名称，递归找文件应使用 `vulcan-file-list`
- 路径值可以包含 `${env:NAME}` 占位符，工具会在访问文件系统前用 Lua `os.getenv` 展开

它不是“翻页猜文件”的工具。还不知道文本在哪里时，应先搜索或列候选文件。

### `vulcan-file-create`

当你需要创建一个原本不存在的文件，或者一小批全新文件，并且可能需要仅预览模式时，使用这个工具。

适合场景：

- 目标文件当前还不存在
- 已经知道最终文件路径和完整文件内容
- 希望明确区分“创建新文件”和“修改旧文件”
- 当宿主开启结构化结果时，希望返回 canonical `change_set` 创建记录

典型参数：

- `PWD`：可选的共享项目/工作区根目录。只有当它指向已存在目录时，相对 `file` 才会基于它解析；否则 `file` 必须本身就是绝对路径
- `file`：精确目标文件路径；支持 `${env:NAME}` 占位符
- `content`：新文件的完整内容；`""` 合法，可创建空文件
- `files`：可选批量形式，最多 10 个 `{ file, content }` 对象；不能和根级 `file` / `content` 同时传
- `no_apply`：默认省略或为 `false` 时立即创建；只有需要不写入预览时才设为 `true`

边界行为：

- 目标已存在时返回 `file_already_exists`，不会静默覆盖
- 父目录不存在时返回 `parent_directory_not_found`
- 父路径存在但不是目录时返回 `parent_path_not_directory`
- 结果会以仅包含新增行的 diff 形式展示，最多展示 80 行
- 批量模式最多接受 10 个条目，且整批共用一个 `no_apply` 标记

### `vulcan-file-edit`

当目标文件和目标行已经确认后，用它做小范围文本编辑。

它支持一个根级单文件编辑请求，也支持最多 10 个条目的 `files` 批量编辑。批量模式适合在已确认上下文后，对多个已知文件做协同的小范围文本修改。

它默认写入。只有需要不写入预览时，才显式传入 `no_apply=true`。

根级 `PWD` 可以指向当前项目或工作区根目录。只有当 `PWD` 有效时，相对 `file` 才会基于它解析；否则 `file` 必须本身就是绝对路径。`${env:NAME}` 占位符仍会在访问文件系统前用 Lua `os.getenv` 展开。

支持模式：

- `overwrite`：覆盖整个文件；为了兼容旧调用，它仍允许在文件不存在时创建新文件，但新建文件时现在优先使用 `vulcan-file-create`，`content=""` 会写成空文件
- `append`：以新行形式追加到文件尾；如果原文件非空且末尾没有换行，会先补一个文件换行符
- `replace_range`：替换指定既有 1-based 闭区间行范围；`content=""` 表示删除这段行
- `insert_before`：插入到指定既有 1-based 锚点行之前
- `insert_after`：插入到指定既有 1-based 锚点行之后
- `files`：可选批量形式，最多 10 个逐文件编辑对象；不能和根级单文件编辑参数同时传
- `no_apply`：默认省略或为 `false` 时立即写入；只有需要不写入预览时才设为 `true`

`insert_before` 与 `insert_after` 要求 `1 <= line <= 文件总行数`。越界会返回 `line_out_of_bounds`，不会自动追加到文件末尾。空文件没有可锚定行，需要创建内容时使用 `overwrite`，需要文件尾新增时使用 `append`。

返回结果会包含：

- 当前状态是 `PREVIEW_ONLY` 还是 `APPLIED`
- 原始行数与编辑后行数
- 原始影响范围
- 编辑后影响范围
- 面向操作的 diff 预览
- 当宿主开启结构化结果时返回 canonical `change_set`
- 参数错误时的明确修正提示

它刻意不做复杂结构判断。如果目标是完整函数或方法替换，应使用 `vulcan-codekit-patch`；如果需要先理解源码结构，应先使用 CodeKit。

### `vulcan-file-delete`

当你需要删除或预览一个普通文件，或者一小批普通文件，并且希望宿主在支持时收到 canonical 删除元数据时，使用这个工具。

删除是破坏性操作，建议正式删除前先用 `no_apply=true` 预览。

典型参数：

- `file`：要删除的精确普通文件路径；支持 `${env:NAME}` 占位符
- `PWD`：可选的共享项目/工作区根目录。只有当它指向已存在目录时，相对 `file` 才会基于它解析；否则 `file` 必须本身就是绝对路径
- `files`：可选批量形式，最多 10 个 `{ file }` 对象；不能和根级 `file` 同时传
- `no_apply`：默认省略或为 `false` 时立即删除；只有需要不删除预览时才设为 `true`

边界行为：

- 目标不存在时返回 `file_not_found`
- 不支持目录删除；目录路径会返回 `directory_delete_unsupported`
- 文本类文件会返回按行删除预览和宿主删除内容
- 宿主 `change_set` 删除记录遵循官方内容模式：不超过 500 行时返回 `content_mode=\"full\"` 和完整 `content`；超过 500 行时会主动切换为 `content_mode=\"truncated\"`，并返回 `total_line_count`、前 50 行 `content_head` 与后 50 行 `content_tail`
- 二进制或非文本文件不会伪造真实行内容，而是使用稳定占位符 `Binary file`，并在预览层按 1 行删除处理

## 一套更适合 Agent 的文件工作流

在 `Vulcan File` 里，推荐路径通常不是：

1. 用 shell 到处列目录
2. 猜一个文件打开全文
3. 复制一段脚本改文件
4. 事后再检查有没有改错

而是：

1. `list` 获取候选文件地图
2. 文件已存在时，用 `read` 精确读取目标行段
3. 新文件用 `create`，小改动用 `edit`，普通文件移除建议先用 `delete no_apply=true` 预览
4. 删除预览确认无误后，再去掉 `no_apply` 重新执行 `delete`；`create` 或 `edit` 需要写入前审查时传入 `no_apply=true`

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

有 `Vulcan File` 时，这些问题被固定成四个稳定入口：

- 文件候选：`list`
- 原文证据：`read`
- 新文件创建：`create`
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
- `vulcan-file-create`
- `vulcan-file-edit`

## 独立仓库说明

当前仓库是 `vulcan-file` LuaSkill 的独立源码仓库，内容对应 LuaSkills 运行时中的正式 skill 包：

- `runtime/`：LuaSkill 工具入口
- `schemas/`：面向 AI 的输入 schema 文件
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
.\scripts\tag_release.ps1 0.1.3
```

Unix-like shell：

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.3
```

## 一句话总结

**如果说 CodeKit 回答的是“结构在哪里、owner 是谁”，那么 `Vulcan File` 回答的是“文件在哪里、原文是什么、这次文本改动是否已经被预览确认”。**

**CodeKit 帮 Agent 理解代码结构，File 帮 Agent 安全处理原文文件。**

这就是它为什么不只是一个文件工具集，而是一层给 Agent 时代准备的轻量文件操作基础设施。
