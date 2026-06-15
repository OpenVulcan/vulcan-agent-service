# Codex Skill Attachment: Vulcan WorkMem Workflow

This attachment mirrors the intended Codex workflow skill so other IDEs, plugins, and agents can reuse the same explicit WorkMem checkpoint rules.

## Trigger

Use WorkMem when:

- Project instructions contain `LUASKILL_SID`.
- The user asks to use WorkMem.
- A task is being resumed from a known LuaSkills managed identity.
- Handoff or checkpoint behavior is requested.
- The host or user indicates that context compression is about to happen.

Do not trigger WorkMem only because a task is complex, multi-file, or long. MCP hosts generally cannot expose remaining model context, and the agent should not guess context pressure.

## Start

1. Call `vulcan-workmem-task-create` with a stable `task_name` and a concise `detail`.
2. If the host exposes a `LUASKILL_SID` field and a saved identity exists, pass that value through the field. If the field is exposed and no identity exists yet, omit it so the tool can generate a new public identity.
3. If the host hides `LUASKILL_SID`, treat that as host-managed mode and let the host inject it automatically.
4. After every successful create call with a public identity, visibly tell the user the active `LUASKILL_SID` and mark it strongly so it survives context compression.
5. When a new public identity is generated, ask whether to save it in `AGENTS.md` or `CLAUDE.md`. In host-managed mode, do not ask for, print, or persist the raw `LUASKILL_SID`.
6. If the tool is unavailable, continue normally and do not invent memory calls.

## Save

After a WorkMem task has been explicitly started, call `vulcan-workmem-set` for durable checkpoints:

- Root cause findings.
- Decisions and implementation direction.
- Progress changes.
- Risks, blockers, or important todos.
- Validation or review results.
- Requested handoff, known compression, resume boundary, or long-pause preparation.

Use `vulcan-workmem-set` with compact nodes only. Store paths, symbols, decisions, risks, progress, validation summaries, and next steps. Do not store full logs, full source files, or long narrative notes.

Prefer one `vulcan-workmem-set` call per meaningful phase. Use `delete_tags` to replace stale progress while writing new progress in the same call.

## Recall

1. Use `vulcan-workmem-list` first.
2. Use `vulcan-workmem-get` with selected tags for normal recall.
3. Use `vulcan-workmem-get-all` only for full recovery, handoff, audit, or explicit user requests.

## Close

Call `vulcan-workmem-task-close` only when the task is genuinely complete. Closing the last task may clean the internal empty identity row, but it does not invalidate the saved remembered identity.
