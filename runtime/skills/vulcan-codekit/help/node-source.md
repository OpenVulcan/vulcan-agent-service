# `vulcan-codekit-node-source`

Use this workflow after `ast-detail` or `rg` has already identified one or more owning functions or methods, but before editing or replacing them.

Best for:

- reading exact function or method bodies without opening the whole file
- reviewing implementation details after structural owner discovery
- preparing a safe full-function replacement for `vulcan-codekit-patch`

Typical route:

1. Locate the owner with `tree`, `rg`, or `ast-detail`.
2. Extract the exact function or method source with `node-source`.
3. Review or prepare the replacement.
4. Use `patch` only when a full-function replacement is needed.

Structural path syntax:

- `structural_path` is a slash-separated structural path suffix, not a regex or glob
- examples: `main`, `UserService/get_user`, `impl MyType/new`, `impl MyTrait for MyType/run`
- a short suffix such as `get_user` is allowed only when it resolves to one function or method
- if a path is ambiguous, the result returns candidates; use one returned candidate path for the next call

Input:

- use `nodes = [{ file, structural_path }, ...]`
- every node item must carry its own `file` and `structural_path`
- for same-file batches, repeat the same `file` in multiple node items
- one node item's `structural_path` may also contain newline-separated structural paths when that is more compact

The tool processes each node independently and returns partial results instead of failing the whole call when one structural path is missing or ambiguous.
Per-node validation errors, such as a missing file or invalid structural path, are returned as `status: error` items with `node_index` and do not abort the rest of the batch.

Batch behavior:

- `max_nodes` defaults to 20
- repeated structural paths that resolve to the same node are reported as `duplicate`
- requests beyond `max_nodes` are reported as `skipped`
- same-file successful nodes are ordered by source line for easier reading
- successful nodes include `node_hash` and `file_hash` for later patch stale checks

The tool returns content with host-managed `truncate` overflow mode and includes `overflow_mode: truncate` in the rendered metadata.
