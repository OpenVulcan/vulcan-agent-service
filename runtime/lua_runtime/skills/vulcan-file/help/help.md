# Vulcan File

Use `vulcan-file` when you need to work with raw file contents directly instead of understanding code structure or searching text.

Prefer these choices:

- Need to create one brand-new file or a small batch of brand-new files: use `vulcan-file-create`; pass `no_apply=true` only when you need a preview without writing.
- Already know the file path and approximate lines: use `vulcan-file-read`.
- Do not know where the target file is yet: use `vulcan-file-list` first to get one low-token file map.
- Need one small text change or a few coordinated text changes: use `vulcan-file-edit`; pass `no_apply=true` only when you need a preview without writing.
- Need to remove regular files and the host should receive delete metadata: use `vulcan-file-delete`; pass `no_apply=true` only when you need a preview without deleting.
- If the host or caller provides root-level `PWD`, relative `file` or `path` values resolve from that directory. Without a valid `PWD`, pass absolute paths.

Batch limits:

- `create`, `read`, `edit`, and `delete` accept either one root-level file request or one `files` array.
- Batch mode supports at most 10 items per call.
- `no_apply` is shared by the whole batch.

Avoid these choices:

- Do not guess line numbers with `vulcan-file-read` when text search or anchor discovery is needed; use `vulcan-codekit-rg` first.
- Prefer `vulcan-codekit` when you need functions, classes, or source structure.
- Use `vulcan-codekit-patch` for whole-function or whole-method replacement.
- Do not force large refactors through text-only edits before reading structural context.
- `vulcan-file-delete` does not support directory removal; pass only regular files.
