# Vulcan File

当你需要直接处理原文文件，而不是理解代码结构或搜索文本时，使用 `vulcan-file`。

优先这样选择：

- 已经知道文件路径和大致行号：使用 `vulcan-file-read`。
- 还不知道目标文件在哪：先使用 `vulcan-file-list` 获取低 token 文件名地图。
- 需要做小范围文本改动：使用 `vulcan-file-edit`，先预览，再 `apply=true`。

不要这样选择：

- 需要搜索文本或定位锚点时，不要用 `vulcan-file-read` 猜行号，先用 `vulcan-codekit-rg`。
- 需要理解函数、类或源码结构时，优先使用 `vulcan-codekit`。
- 需要替换完整函数或方法时，使用 `vulcan-codekit-patch`。
- 需要大规模重构时，不要用文本级编辑硬改，应先读取结构上下文。
