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

- `overwrite`：覆盖整个文件；这是唯一允许在文件不存在时创建新文件的模式，`content=""` 会写成空文件。
- `append`：追加到文件尾；如果原文件非空且末尾没有换行，会先补一个文件换行符，让追加内容从新行开始。
- `replace_range`：替换指定既有 1-based 闭区间行范围；`content=""` 表示删除这段行。
- `insert_before`：插入到指定既有行之前。
- `insert_after`：插入到指定既有行之后。

默认 `apply=false`，只返回预览 diff。只有显式传入 `apply=true` 时才会写入文件。

`append`、`insert_before`、`insert_after` 需要非空 `content`，避免无变化编辑被误判为已完成。

边界行为：

- 只有 `overwrite` 可以创建不存在的文件；其他模式遇到不存在文件会返回 `file_not_found`。
- `insert_before` / `insert_after` 的 `line` 必须满足 `1 <= line <= 文件总行数`。
- 空文件没有可锚定行，插入模式会返回 `line_out_of_bounds`；需要创建内容时使用 `overwrite`，需要文件尾新增时使用 `append`。
- `insert_after` 允许 `line` 等于文件总行数，表示锚定最后一行之后插入；大于文件总行数仍返回 `line_out_of_bounds`，不会自动追加。
- 预览最多展示 80 行变更上下文，超过时会在 diff 中标记 `preview truncated`。
