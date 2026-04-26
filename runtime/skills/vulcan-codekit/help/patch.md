# `vulcan-codekit-patch`

Use this workflow only when the target functions or methods are already confirmed and every replacement is a full-function replacement.

Best for:

- AST-safe whole-function replacement
- batch patching handler/helper/test changes in one call
- avoiding stale line-based edits
- keeping selector-based targeting precise

Input options:

- legacy mode: pass `file`, `selector`, and `replacement`
- batch mode: pass `patches = [{ file, selector, replacement }, ...]`
- optional stale checks: `expected_node_hash`, `expected_source_hash`, `expected_file_hash`, `expected_range`

Batch rules:

- `atomic` defaults to `true`
- any missing, ambiguous, stale, invalid, or overlapping patch rejects the whole atomic batch before writing
- set `atomic=false` only when partial application is explicitly desired
- same-file patches are applied in descending line order
- overlapping same-file target ranges are rejected
- `max_patches` defaults to 20
- applied results report `previous_node_hash` and `new_node_hash`; use `new_node_hash` for later stale checks
- stale rejections report expected/actual diagnostics such as `expected_node_hash` and `actual_node_hash`

Typical route:

1. Confirm owners with `tree`, `rg`, or `ast-detail`.
2. Read exact current implementations with `node-source`.
3. Submit one `patches[]` batch with full replacement functions.
4. Validate with TestKit or the project-specific check.
