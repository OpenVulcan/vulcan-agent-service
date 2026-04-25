# Action Reference

Use actions as a workflow, not as isolated database commands:

```text
task-create -> set -> list/get -> get-all when recovering -> task-close
```

Call `set` only when there is durable value: key file summaries, decisions, progress changes, risks, validation results, or next steps. Do not call it for every tiny observation.

## `task-create`

Create or reuse a task.

Required:

- `task_name`
- `detail`

Optional:

- `workmem_id`

If `workmem_id` is omitted, a new compact `VULCAN_WORKMEM_ID` is generated. If a caller provides an ID, it may use a host, plugin, or session-derived format, but it must be single-line and 20-128 characters. The response asks the assistant to visibly ask the user whether to save newly generated IDs in `AGENTS.md` or `CLAUDE.md`.

Use this when starting a remembered task, resuming with a saved ID, or re-establishing task context after compression.

## `set`

Batch write nodes.

Required:

- `workmem_id`
- `task_name`
- `list`

Optional:

- `delete_tags`

Each `list` item must contain:

- `tag`
- `type`
- `title`
- `content`

Supported types: `note`, `file_summary`, `decision`, `todo`, `progress`, `risk`, `checkpoint`, `tool_result`.

Use `delete_tags` to replace stale progress in the same call. Prefer one `set` call after a meaningful phase instead of many small calls.

## `list`

List tags for one task without expanding content.

Required:

- `workmem_id`
- `task_name`

Optional:

- `type`
- `tag_prefix`

Use this before `get` when you do not know the exact tags to recall.

## `get`

Read selected nodes.

Required:

- `workmem_id`
- `task_name`

Optional:

- `tags`

When `tags` is omitted, the response is a compact task summary, not a full recall.

Prefer passing exact tags selected from `list`.

## `get-all`

Recall every node for one task.

Required:

- `workmem_id`
- `task_name`

Use only after context compression, during handoff, for review, or when the user explicitly asks for full recall. Do not use it as the normal first read path.

## `del`

Delete selected tags.

Required:

- `workmem_id`
- `task_name`
- `tags`

## `task-list`

List all tasks under one `workmem_id`.

Required:

- `workmem_id`

## `task-close`

Close one task and remove its nodes.

Required:

- `workmem_id`
- `task_name`

This does not invalidate the long-lived `VULCAN_WORKMEM_ID`.
