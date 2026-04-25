# Vulcan WorkMem

Vulcan WorkMem is project-scoped working memory for AI coding agents. It stores short task facts in SQLite so the agent can recover important context after long tool chains, context compression, or task handoff.

Use it as the task's external scratchpad when the user enables WorkMem, when a project rule file contains `VULCAN_WORKMEM_ID`, or when a task is long enough that repeated file analysis or context compression is likely.

For IDEs or plugins that cannot load Codex skills directly, reuse the workflow attachment at `attachments/codex-skill-vulcan-workmem-workflow.md`.

## When To Use

- At the start of a remembered task.
- After reading several important files and forming a useful project map.
- After a key decision, risk, blocker, or implementation direction changes.
- Before and after core code edits.
- After validation produces important results.
- Before context compression, task handoff, or a long pause.

Avoid using WorkMem for tiny one-shot questions unless the user explicitly asks for the WorkMem workflow.

## Minimal Loop

1. Start or resume with `task-create`.
2. Save compact findings with `set`.
3. Inspect available memory with `list`.
4. Recall selected details with `get`.
5. Use `get-all` only for full recovery, handoff, audit, or explicit user requests.
6. Finish with `task-close`.

Use `vulcan-workmem-run` with one `action`:

- `task-create`: create or reuse a task space.
- `set`: batch write or update nodes.
- `list`: list node tags without full content.
- `get`: read selected node content.
- `get-all`: recall every node for one task.
- `del`: delete selected tags.
- `task-list`: list tasks for a VULCAN_WORKMEM_ID.
- `task-close`: close and clean one task.

Use `workmem_id` as the stable project ID. Display it to users as `VULCAN_WORKMEM_ID`. Caller-provided IDs may use host, plugin, or session-derived formats, but they must be single-line and 20-128 characters.

When no `workmem_id` exists, call `task-create` without it. The tool generates a new ID and tells the assistant to ask the user whether to persist it in `AGENTS.md` or `CLAUDE.md`.

Store compact facts only: decisions, progress, file summaries, risks, todos, checkpoints, paths, symbols, and concise tool results. Do not store full logs, full source files, or long narrative notes.
