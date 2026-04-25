# Codex Skill Attachment: Vulcan WorkMem Workflow

This attachment mirrors the intended Codex workflow skill so other IDEs, plugins, and agents can reuse the same WorkMem timing rules.

## Trigger

Use WorkMem for long, complex, or resumable coding work when:

- Project instructions contain `VULCAN_WORKMEM_ID`.
- The user asks to use WorkMem.
- The task spans multiple files or code areas.
- Several important files have been read.
- Root cause, design direction, risk, blocker, or key progress has been found.
- Core edits are about to happen or just happened.
- Validation, tests, build output, or review findings produced important results.
- Context compression, handoff, resume, or a long pause is likely.
- The agent would otherwise need to rediscover code structure.

Avoid WorkMem for tiny one-shot questions unless the user explicitly asks for it.

## Start

1. If a saved `VULCAN_WORKMEM_ID` exists, call `vulcan-workmem-run` with `action=task-create`, that `workmem_id`, a stable `task_name`, and a concise `detail`.
2. If WorkMem is enabled but no ID exists, call `task-create` without `workmem_id`.
3. When a new ID is generated, ask whether to save it in `AGENTS.md` or `CLAUDE.md`.
4. If the tool is unavailable, continue normally and do not invent memory calls.

## Save

Call `set` after meaningful state changes:

- Key file summaries.
- Root cause findings.
- Decisions and implementation direction.
- Progress changes.
- Risks, blockers, or important todos.
- Validation or review results.
- Context compression or handoff preparation.

Use compact nodes only. Store paths, symbols, decisions, risks, progress, validation summaries, and next steps. Do not store full logs, full source files, or long narrative notes.

Prefer one `set` call per meaningful phase. Use `delete_tags` to replace stale progress while writing new progress in the same call.

## Recall

1. Use `list` first.
2. Use `get` with selected tags for normal recall.
3. Use `get-all` only for full recovery, handoff, audit, or explicit user requests.

## Close

Call `task-close` only when the task is genuinely complete. Closing a task does not invalidate the saved `VULCAN_WORKMEM_ID`.
