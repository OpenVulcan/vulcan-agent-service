# `vulcan-file-edit`

当你已经确认目标文件和目标行，需要做小范围文本编辑时，使用这个工具。

适合使用：

- 已经用 `vulcan-file-read` 看过目标上下文。
- 只需要覆盖、追加、替换行范围或在某行前后插入文本。
- 希望先得到预览，再决定是否写入。
- `file` 路径需要引用环境变量时，可使用 `${env:NAME}` 占位符。

不适合使用：

- 需要替换完整函数或方法时，优先使用 `vulcan-codekit-patch`。
- 需要理解源码结构后再编辑时，先使用 `vulcan-codekit`。
- 还没确认目标行时，不要直接编辑，先读取上下文。

支持模式：

- `overwrite`：覆盖整个文件。
- `append`：追加到文件尾。
- `replace_range`：替换指定 1-based 行范围。
- `insert_before`：插入到指定行之前。
- `insert_after`：插入到指定行之后。

默认 `apply=false`，只返回预览 diff。只有显式传入 `apply=true` 时才会写入文件。

`append`、`insert_before`、`insert_after` 需要非空 `content`，避免无变化编辑被误判为已完成。
