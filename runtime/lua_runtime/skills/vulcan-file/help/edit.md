# `vulcan-file-edit`

Use this tool when the target file and target lines are already confirmed and you need one small text edit or a small batch of coordinated text edits.

Good fits:

- You already inspected the target context with `vulcan-file-read`.
- You only need to overwrite, append, replace one line range, or insert before or after one existing line.
- You want either an immediate write or a preview-only edit result.
- The host should receive structured `change_set` edit records when supported.
- The `file` path may use `${env:NAME}` placeholders.
- Batch mode should edit at most 10 files in one call.

Not a good fit:

- Prefer `vulcan-file-create` when you need to create one file that does not exist yet.
- Prefer `vulcan-codekit-patch` when whole-function or whole-method replacement is needed.
- Use `vulcan-codekit` first when source structure must be understood before editing.
- Do not edit yet if the target lines are still uncertain; read the context first.

Supported modes:

- `overwrite`: replace the whole file. For backward compatibility it still allows creation when the file does not exist, but `vulcan-file-create` is now the preferred entry for brand-new files; `content=""` creates or leaves an empty file.
- `append`: append at the end of the file; when the original file is non-empty and lacks a final newline, one file newline is inserted first so appended content starts on a new line.
- `replace_range`: replace one existing 1-based closed line range; `content=""` deletes that range.
- `insert_before`: insert before one existing line.
- `insert_after`: insert after one existing line.

Parameter choices:

- Single-file mode: send root-level `file`, `mode`, `content`, and any mode-specific line arguments.
- Batch mode: send `files` as an array of edit objects. Each item can carry its own `mode`, `content`, `start_line`, `end_line`, or `line`.
- Batch mode accepts at most 10 items.
- Root-level `PWD` is optional. When it points to an existing directory, relative `file` values resolve from that root; otherwise every `file` must already be absolute.
- `no_apply=false` by default, so the tool writes immediately. Pass `no_apply=true` only when you need a preview diff without writing, and in batch mode that one flag applies to every item.

Boundary behavior:

- Only `overwrite` still supports creating a missing file for backward compatibility; other modes return `file_not_found` when the file does not exist.
- `append`, `insert_before`, and `insert_after` require non-empty `content` so no-op edits are not misreported as completed work.
- `insert_before` and `insert_after` require `1 <= line <= total_lines`.
- Empty files have no anchor lines, so insert modes return `line_out_of_bounds`; use `overwrite` to create content or `append` for file-end additions.
- `insert_after` allows `line == total_lines`, which means insert after the last existing line; values beyond the total line count still return `line_out_of_bounds` and never append silently.
- The preview shows at most 80 lines of changed context per file; larger previews are marked with `preview truncated`.
