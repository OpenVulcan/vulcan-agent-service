# `vulcan-codekit-node-source`

Use this workflow after `ast-detail` or `rg` has already identified one or more owning functions or methods, and before `vulcan-codekit-patch` replaces any of them as whole nodes.

Best for:

- reading exact function or method bodies without opening the whole file
- reviewing implementation details after structural owner discovery
- preparing a safe full-function replacement for `vulcan-codekit-patch`
- confirming the current source before an optional stale-checked replacement

Typical route:

1. Locate the owner with `tree`, `rg`, or `ast-detail`.
2. Extract the exact function or method source with `node-source`.
3. Review the returned source and prepare the complete replacement.
4. Use `patch` only when a whole-function or whole-method replacement is needed.

Structural path syntax:

- `structural_path` is a slash-separated structural path suffix, not a regex or glob
- examples: `main`, `UserService/get_user`, `impl MyType/new`, `impl MyTrait for MyType/run`
- a short suffix such as `get_user` is allowed only when it resolves to one function or method
- if a path is ambiguous, the result returns candidates; use one returned candidate path for the next call

Boundaries:

- only function or method nodes are returned
- non-function symbols such as enum, enum variant/member, struct field, type alias, and statement-level nodes are outside this workflow
- use this as the read step for whole-node replacement, not as a generic structural extractor

Input:

- use `nodes = [{ file, structural_path }, ...]`
- every node item must carry its own `file` and `structural_path`
- for same-file batches, repeat the same `file` in multiple node items
- one node item's `structural_path` may also contain newline-separated structural paths when that is more compact

The tool processes each node independently and returns partial results instead of failing the whole call when one structural path is missing or ambiguous.
Per-node validation errors, such as a missing file or invalid structural path, are returned as `status: error` items with `node_index` and do not abort the rest of the batch.

Batch behavior:

- `max_nodes` defaults to 20 and is an advanced batch safety limit
- repeated structural paths that resolve to the same node are reported as `duplicate`
- requests beyond `max_nodes` are reported as `skipped`
- same-file successful nodes are ordered by source line for easier reading
- when a structural path resolves to a non-function symbol, the tool reports that the path is outside the function/method patch workflow

Successful output contains the file, structural path, canonical target, line range, signature, and complete current source. Runtime limits, request indexes, overflow metadata, and source hashes are not included. Node-source does not calculate hashes; patch accepts optional stale-check hashes only when the caller already has trusted values.
