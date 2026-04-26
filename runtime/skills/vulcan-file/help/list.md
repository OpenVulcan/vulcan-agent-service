# `vulcan-file-list`

当你还不知道目标文件在哪，或者需要先缩小候选文件集合时，使用这个工具。

适合使用：

- 进入陌生目录前，先看低 token 文件名地图。
- 已知道文件扩展名或文件名形状，需要快速筛候选文件。
- 想避免手写 shell 循环、排序、忽略规则处理。

不适合使用：

- 需要搜索文件内容时，使用 `vulcan-codekit-rg`。
- 需要函数、类、Markdown 标题等结构信息时，使用 `vulcan-codekit`。
- 已经知道具体文件和行号时，直接用 `vulcan-file-read`。

参数选择：

- `path`：尽量传最小可能目录；不传表示从当前目录做项目级扫描。
- `pattern`：知道扩展名或文件名形状时传 glob，例如 `*.rs`、`*.lua`、`Cargo.*`。
- `recursive`：默认递归；只想看当前目录直接子项时设为 `false`。
- `noignore`：只有确实需要看生成物或被忽略目录时才设为 `true`。
- `limit`：结果被截断且仍需要更多候选文件时再调大。

输出按目录分组，并在每组中按文件名排序：

```text
src/ config.rs main.rs server.rs
src/discover/ mod.rs provider.rs registry.rs report.rs
```

默认会读取每层目录下的 `.gitignore` 与 `.ignore`，并跳过 `.git`、`target`、`node_modules`、`output`、`dist`、`build` 等内建高噪声目录。
