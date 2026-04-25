# Rule File Workflow

When a user chooses to use Vulcan WorkMem in a project, ask whether to save the generated `VULCAN_WORKMEM_ID` into the highest-level rule file:

- `AGENTS.md`
- `CLAUDE.md`
- a user-specified host rule file

When this marker already exists, use its `VULCAN_WORKMEM_ID` directly. Do not ask to save it again.

Suggested marker block:

```markdown
<!-- VULCAN_WORK_MEMORY_START -->
## Vulcan Work Memory

- VULCAN_WORKMEM_ID: vwm_...
- rule: Use this workmem_id for Vulcan WorkMem in this project.
- rule: Call `vulcan-workmem-run` with `action=task-create` and this workmem_id when starting a remembered task.
- rule: Use `set`, `list`, `get`, `get-all`, `del`, and `task-close` for task memory operations.
- rule: Do not ask to save this ID again unless this block is missing or the user requests regeneration.
<!-- VULCAN_WORK_MEMORY_END -->
```

Plugin-managed marker block:

```markdown
<!-- VULCAN_WORK_MEMORY_START -->
## Vulcan Work Memory

- mode: plugin-managed
- rule: The plugin injects the active workmem_id automatically.
- rule: Do not ask for or preserve the raw workmem_id.
- rule: Use `set`, `list`, `get`, `get-all`, `del`, and `task-close` for task memory operations.
<!-- VULCAN_WORK_MEMORY_END -->
```

Never edit rule files without explicit user approval. `task-close` does not remove this marker block.
