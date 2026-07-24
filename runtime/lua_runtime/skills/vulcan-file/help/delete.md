# `vulcan-file-delete`

Use this tool when you want to remove or preview one regular file or a small batch of regular files, and the host should receive explicit delete metadata when supported.

Because deletion is destructive, previewing with `no_apply=true` before deleting is recommended.

Good fits:

- The target path already points to one regular file.
- You want either an immediate delete or a preview-only delete result.
- The host should receive structured `change_set` delete records when supported.
- The `file` path may use `${env:NAME}` placeholders.
- Batch mode should delete at most 10 regular files in one call.

Not a good fit:

- Directory removal is not supported.
- Recursive cleanup is not supported.
- If you are still searching for candidate files, use `vulcan-file-list` first.

Parameter choices:

- Single-file mode: send root-level `file`.
- Batch mode: send `files` as an array of `{ "file": "..." }` objects.
- Batch mode accepts at most 10 items.
- Root-level `PWD` is optional. When it points to an existing directory, relative `file` values resolve from that root; otherwise every `file` must already be absolute.
- `no_apply=false` by default, so the tool deletes immediately. Pass `no_apply=true` only when you need a preview without deleting, and in batch mode that one flag applies to every item.

Boundary behavior:

- Missing targets return `file_not_found`.
- Directory paths return `directory_delete_unsupported`; `delete` only accepts regular files.
- Text-like files return line-oriented delete previews and host delete content.
- Host `change_set` delete records use `content_mode="full"` for up to 500 lines and `content_mode="truncated"` beyond that, with `total_line_count` plus `content_head` and `content_tail` holding the first and last 50 lines.
- Binary or non-text files do not pretend to return real line content. They use the stable English placeholder `Binary file` and report one removed line for preview purposes.
- The preview shows at most 80 lines of changed context per file; larger previews are marked with `preview truncated`.
