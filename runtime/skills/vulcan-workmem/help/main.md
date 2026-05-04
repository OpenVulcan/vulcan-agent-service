# Vulcan WorkMem Workflow

Vulcan WorkMem is project-scoped working memory for AI coding agents. Treat it as an explicit checkpoint and recovery layer, not as automatic memory for every long task.

Read this workflow through the `vulcan-workmem` main help flow before using WorkMem. In strict host help wrappers that expose `vulcan-help-detail`, call it with `skill=vulcan-workmem` and `flow=main`; otherwise read this main flow directly.

Use WorkMem when the user explicitly asks for it, project instructions contain `LUASKILL_SID`, a task is being resumed from a known LuaSkills managed identity, a handoff is requested, or the host/user indicates that context compression is about to happen.

Do not trigger WorkMem only because a task is complex, multi-file, or long. MCP tools generally cannot inspect remaining model context, and the agent should not guess context pressure.

## Minimal Workflow

1. Start or resume only after explicit user/project/handoff intent with `vulcan-workmem-task-create`.
2. Save compact checkpoints with `vulcan-workmem-set`.
3. Inspect available memory with `vulcan-workmem-list`.
4. Recall selected details with `vulcan-workmem-get`.
5. Use `vulcan-workmem-get-all` only for full recovery, handoff, audit, or explicit full recall requests.
6. Finish with `vulcan-workmem-task-close`.

## Start

Use `vulcan-workmem-task-create` at the start of a remembered task.

Parameters:

- `LUASKILL_SID`: optional existing LuaSkills managed identity. Omit it only when generating a new public identity.
- `task_name`: required stable task name.
- `detail`: required concise task goal, boundary, known focus, and current starting point.

After every successful create call, visibly tell the user the active ID in the conversation. Mark it strongly, for example:

```text
Active LUASKILL_SID: `vwm_...`
Keep this identity for compression, resume, or handoff.
```

This is required for public identities that are not host-managed. If the host injects `LUASKILL_SID` in managed mode, do not ask for, print, or persist the raw identity. If the tool generated a new public identity, ask whether to save it into `AGENTS.md`, `CLAUDE.md`, or another project rule file.

## Save

After a WorkMem task has been explicitly started, call `vulcan-workmem-set` for durable checkpoints:

- Finding a root cause.
- Making a design or implementation decision.
- Discovering a risk, blocker, or important todo.
- After validation, tests, build output, or review findings.
- Before requested handoff, known context compression, resume boundary, or a long pause.

Store compact facts only: paths, symbols, decisions, risks, progress, validation summaries, and next steps. Do not store full logs, full source files, or long narrative notes.

Prefer one `vulcan-workmem-set` call per meaningful phase. Use `delete_tags` to replace stale progress while writing new progress in the same call.

Each node in `list` must contain:

- `tag`
- `type`
- `title`
- `content`

Supported node types:

- `note`: compact miscellaneous facts that do not fit a narrower type.
- `file_summary`: important file or symbol summaries.
- `decision`: choices that should survive compression.
- `todo`: remaining work.
- `progress`: phase status.
- `risk`: known hazards or blockers.
- `checkpoint`: stable recovery points before compression or handoff.
- `tool_result`: compact validation, build, test, or review results.

## Recall

On resume or after compression, call `vulcan-workmem-list` first. Then call `vulcan-workmem-get` with selected tags for normal recall.

When `vulcan-workmem-get` omits `tags`, it returns a latest compact node preview with at most 8 nodes. Treat that as a quick orientation view, not as a full task summary.

Use `vulcan-workmem-get-all` only for full recovery, handoff, audit, or when the user explicitly asks for all memory.

Use `vulcan-workmem-task-list` when you know a `LUASKILL_SID` but need to discover which task names exist under it.

## Delete And Close

Use `vulcan-workmem-delete` to remove selected stale tags from a task.

Call `vulcan-workmem-task-close` only when the task is genuinely complete. Closing a task removes task-scoped nodes and may clean the internal empty identity row when no tasks remain, but it does not invalidate a saved long-lived `LUASKILL_SID`.

## Rule File Marker

When a user chooses to persist a generated ID, use a marker block like this:

```markdown
<!-- VULCAN_WORK_MEMORY_START -->
## Vulcan Work Memory

- LUASKILL_SID: vwm_...
- rule: Use this LUASKILL_SID for Vulcan WorkMem in this project.
- rule: Call `vulcan-workmem-task-create` with this LUASKILL_SID when starting a remembered task.
- rule: Use `vulcan-workmem-set`, `vulcan-workmem-list`, `vulcan-workmem-get`, `vulcan-workmem-get-all`, `vulcan-workmem-delete`, `vulcan-workmem-task-list`, and `vulcan-workmem-task-close` for task memory operations.
- rule: Do not ask to save this ID again unless this block is missing or the user requests regeneration.
<!-- VULCAN_WORK_MEMORY_END -->
```

Never edit rule files without explicit user approval. `vulcan-workmem-task-close` does not remove this marker block.

## Parameter Rules

- `LUASKILL_SID`: single-line stable identity, 20 to 128 characters when caller-provided.
- `task_name`: lowercase letters, digits, `_`, or `-`; maximum 96 characters; must start and end with a letter or digit.
- `tag`: lowercase letters, digits, `_`, or `-`; maximum 96 characters; must start and end with a letter or digit.
- `tag_prefix`: lowercase letters, digits, `_`, or `-`; maximum 96 characters; must start with a letter or digit and may end with `_` or `-` because it is a prefix filter.
- `detail`: maximum 1200 characters.
- `title`: maximum 160 characters.
- `content`: maximum 2000 characters.
- `vulcan-workmem-set` may write at most 30 nodes per call.
