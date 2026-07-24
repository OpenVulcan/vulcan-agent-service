# `vulcan-file-create`

Use this tool when you want to create one file that does not exist yet, or a small batch of brand-new files, with an optional preview-only mode.

Good fits:

- The target file does not exist yet.
- The full target path and full file content are already known.
- You want a clear separation between creating a new file and editing an existing one.
- The host should receive structured `change_set` create records when supported.
- The `file` path may use `${env:NAME}` placeholders.
- Batch mode should create at most 10 files in one call.

Not a good fit:

- The target file already exists; use `vulcan-file-edit` instead.
- You only need to append or replace a small amount of text in one existing file; use `vulcan-file-edit` instead.
- You do not know where the file should be created yet; use `vulcan-file-list` or another search tool first.

Parameter choices:

- Single-file mode: send root-level `file` and `content`.
- Batch mode: send `files` as an array of `{ "file": "...", "content": "..." }` objects.
- Batch mode accepts at most 10 items.
- Root-level `PWD` is optional. When it points to an existing directory, relative `file` values resolve from that root; otherwise every `file` must already be absolute.
- `no_apply=false` by default, so the tool writes immediately. Pass `no_apply=true` only when you need a preview without writing, and in batch mode that one flag applies to every item.

Boundary behavior:

- Existing targets return `file_already_exists`; they are never overwritten silently.
- Missing parent directories return `parent_directory_not_found`.
- Parent paths that exist but are not directories return `parent_path_not_directory`.
- `content=""` is valid and creates an empty file.
- The preview shows at most 80 lines of context per file; larger previews are marked with `preview truncated`.
