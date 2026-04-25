# Examples

## Minimal Operating Loop

```text
task-create(workmem_id?, task_name, detail)
set(workmem_id, task_name, list=[...])
list(workmem_id, task_name)
get(workmem_id, task_name, tags=[...])
task-close(workmem_id, task_name)
```

Use `get-all` only when recovering after compression, handing off work, auditing the task, or when the user explicitly asks for full recall.

## Start A Task With No Saved ID

```text
action: task-create
task_name: implement-workmem
detail: Implement the first SQLite-backed Vulcan WorkMem skill.
```

The response contains a generated `VULCAN_WORKMEM_ID` and asks the assistant to ask the user whether to save it.

## Start A Task With A Saved ID

```text
action: task-create
workmem_id: vwm_...
task_name: implement-workmem
detail: Continue the WorkMem implementation using the saved project ID.
```

## Save Progress

```text
action: set
workmem_id: vwm_...
task_name: implement-workmem
delete_tags: ["progress-old"]
list:
  - tag: progress-implementation
    type: progress
    title: Implementation
    content: Added the skill package, SQLite schema, and Markdown renderers. Next step is call-tools validation.
```

## Save Findings After Reading Key Files

```text
action: set
workmem_id: vwm_...
task_name: implement-workmem
list:
  - tag: file-runtime-summary
    type: file_summary
    title: Runtime entry behavior
    content: runtime/vulcan-workmem.lua owns action dispatch, SQLite schema, validation, and Markdown rendering. Important functions: dispatch, action_set, action_get_all.
  - tag: decision-single-entry
    type: decision
    title: Single entry API
    content: Keep one exposed tool entry and route by action to reduce MCP tool-list context.
```

## Recall Tags Then Details

```text
action: list
workmem_id: vwm_...
task_name: implement-workmem
type: progress
```

```text
action: get
workmem_id: vwm_...
task_name: implement-workmem
tags: ["progress-implementation"]
```

## Full Recall

```text
action: get-all
workmem_id: vwm_...
task_name: implement-workmem
```

## Close A Task

```text
action: task-close
workmem_id: vwm_...
task_name: implement-workmem
```
