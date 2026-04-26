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

Input:

- use `nodes = [{ file, selector }, ...]`
- every node item must carry its own `file` and `selector`
- for same-file batches, repeat the same `file` in multiple node items
- one node item's `selector` may also contain newline-separated selectors when that is more compact

The tool processes each node independently and returns partial results instead of failing the whole call when one selector is missing or ambiguous.
Per-node validation errors, such as a missing file or invalid selector, are returned as `status: error` items with `node_index` and do not abort the rest of the batch.

Batch behavior:

- `max_nodes` defaults to 20
- repeated selectors that resolve to the same node are reported as `duplicate`
- requests beyond `max_nodes` are reported as `skipped`
- same-file successful nodes are ordered by source line for easier reading
- successful nodes include `node_hash` and `file_hash` for later patch stale checks

The tool returns content with host-managed `truncate` overflow mode and includes `overflow_mode: truncate` in the rendered metadata.
