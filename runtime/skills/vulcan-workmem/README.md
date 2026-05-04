# Vulcan WorkMem

Simplified Chinese: [README.zh-CN.md](README.zh-CN.md)

`Vulcan WorkMem` is an explicit checkpoint and recovery memory layer for AI coding agents.

It is not automatic long-term memory, and it does not try to guess how much model context remains. Its job is narrower and more practical: when a user, host, project rule, resume flow, or handoff explicitly asks for durable task state, WorkMem stores compact checkpoints that an agent can recall later.

**WorkMem helps agents recover task state without turning the active conversation into a memory dump.**

The current LuaSkills naming model uses the canonical `skill_id-entry_name` form, so the recommended tool names are:

- `vulcan-workmem-task-create`
- `vulcan-workmem-set`
- `vulcan-workmem-list`
- `vulcan-workmem-get`
- `vulcan-workmem-get-all`
- `vulcan-workmem-delete`
- `vulcan-workmem-task-list`
- `vulcan-workmem-task-close`

Some MCP clients or host bindings may expose the same tools with underscores, such as `vulcan_workmem_set`. That is only a naming difference at the exposure layer; the semantics still map to the same WorkMem entries.

In one sentence:

**Create an explicit task checkpoint, write compact facts, recall only what is needed, and close the task when it is done.**

## What Problem It Solves

AI coding agents often lose useful working state when:

- A conversation is compressed.
- A task is resumed after a long pause.
- Work is handed from one agent or session to another.
- A user wants a durable checkpoint before a risky or multi-step phase.
- A project has stable task memory rules outside the current prompt.

Without WorkMem, agents tend to recover by re-reading files, re-running searches, or relying on a compressed summary that may omit important decisions. That is wasteful and sometimes risky.

`Vulcan WorkMem` fills the gap with a small, explicit memory protocol:

- It stores project-scoped task nodes in SQLite through the LuaSkills runtime.
- It encourages compact facts instead of full logs or source dumps.
- It makes recall selective by listing tags before reading content.
- It separates task lifecycle from the long-lived `LUASKILL_SID`.
- It keeps memory use deliberate instead of automatic.

## When To Use

Use WorkMem when:

- The user explicitly asks to use WorkMem.
- Project instructions contain a saved `LUASKILL_SID`.
- A task is being resumed from a known LuaSkills managed identity.
- Handoff or checkpoint behavior is requested.
- The host or user indicates that context compression is about to happen.

Do not trigger WorkMem only because a task is complex, long, or multi-file. MCP hosts generally cannot expose remaining model context, and the agent should not guess context pressure.

## Core Tools

### `vulcan-workmem-task-create`

Create or resume one project-scoped WorkMem task.

Use this at the start of an explicitly remembered task. If an existing `LUASKILL_SID` is available, pass it through `LUASKILL_SID`; otherwise omit it only when a new public identity should be generated.

After every successful create call with a public identity, the agent must visibly tell the user the active `LUASKILL_SID` and mark it strongly enough to survive context compression. In host-managed mode, the host may inject and redact `LUASKILL_SID`; agents should not ask for, print, or persist the raw managed identity.

### `vulcan-workmem-set`

Write compact task nodes.

Good node content includes file summaries, decisions, risks, progress, todos, validation summaries, and stable recovery checkpoints. Do not store full source files, full command logs, or long narrative notes.

Supported node types:

- `note`
- `file_summary`
- `decision`
- `todo`
- `progress`
- `risk`
- `checkpoint`
- `tool_result`

### `vulcan-workmem-list`

List stored node tags without expanding full content.

Use this first on resume or after compression so the agent can choose which tags to recall instead of dumping everything into context.

### `vulcan-workmem-get`

Read selected tags, or omit `tags` when a latest compact node preview of up to 8 nodes is enough.

This is the normal recall path after `list`.

### `vulcan-workmem-get-all`

Recall every node for one task.

Use this only for full recovery, handoff, audit, or explicit full recall requests.

### `vulcan-workmem-delete`

Delete selected stale tags from one task.

### `vulcan-workmem-task-list`

List remembered task names under one `LUASKILL_SID`.

Use this when the ID is known but the task name is not.

### `vulcan-workmem-task-close`

Close one task and remove task-scoped nodes.

Closing a task may clean the internal empty identity row when no tasks remain, but it does not invalidate a saved long-lived `LUASKILL_SID`.

## Workflow

1. Read the WorkMem help flow when WorkMem is needed:

```text
skill=vulcan-workmem
flow=main
```

2. Start or resume an explicitly remembered task with `vulcan-workmem-task-create`.
3. Tell the user the active public `LUASKILL_SID`.
4. Save durable checkpoints with `vulcan-workmem-set`.
5. On resume, call `vulcan-workmem-list` before selected `vulcan-workmem-get`.
6. Use `vulcan-workmem-get-all` only for full recovery or explicit requests.
7. Close completed task memory with `vulcan-workmem-task-close`.

## Repository Notes

This repository is the standalone source repository for the `vulcan-workmem` LuaSkill package. It maps to the published skill package used by the LuaSkills runtime:

- `runtime/`: LuaSkill tool entries and shared WorkMem runtime logic
- `help/`: strict help flow for AI-facing WorkMem workflow guidance
- `attachments/`: Codex skill workflow attachment for hosts that import agent instructions
- `overflow_templates/`: reserved local overflow-template directory
- `resources/`: reserved resource directory
- `licenses/`: third-party notices
- `scripts/`: validation, packaging, and release helpers

This repository is no longer maintained as a demo skill. It is the release source for `vulcan-workmem`. Releases generate the standard LuaSkill artifacts:

- `vulcan-workmem-v{version}-skill.zip`
- `vulcan-workmem-v{version}-checksums.txt`
- `vulcan-workmem-v{version}-source.yaml`

The top-level directory inside the zip must be the runtime skill name:

```text
vulcan-workmem/
```

## Dependencies And Artifacts

`dependencies.yaml` declares runtime dependencies. Currently, `vulcan-workmem` declares no external tool, Lua, or FFI dependencies.

WorkMem relies on the SQLite capability provided by the LuaSkills host runtime.

Local validation:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

Optional source metadata:

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

The generated metadata points to the matching `LuaSkills/vulcan-workmem` GitHub Release assets unless `--base-url` is provided. The GitHub release workflow emits and uploads this metadata together with the zip and checksum assets.

## Release Flow

Releases are tag-driven. A pushed tag matching `v*` triggers the release workflow, and the tag must match `skill.yaml.version`.

Recommended local release steps:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py --emit-source-yaml
.\scripts\tag_release.ps1 0.1.1
```

Unix-like shell:

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py --emit-source-yaml
./scripts/tag_release.sh 0.1.1
```

## One-Sentence Summary

**If ordinary memory tries to remember everything, `Vulcan WorkMem` remembers only explicit checkpoints that help an agent resume, hand off, or recover after compression.**

**WorkMem is not automatic memory. It is deliberate task recovery infrastructure for the agent era.**
