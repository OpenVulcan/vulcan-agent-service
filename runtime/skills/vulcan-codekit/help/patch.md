# `vulcan-codekit-patch`

Use this workflow only when the target functions or methods are already confirmed and every replacement is a full-function replacement.

Best for:

- AST-safe whole-function replacement
- batch patching handler/helper/test changes in one call
- avoiding stale line-based edits
- keeping structural_path-based targeting precise

Structural path syntax:

- `structural_path` is a slash-separated structural path suffix, not a regex or glob
- examples: `main`, `UserService/get_user`, `impl MyType/new`, `impl MyTrait for MyType/run`
- ambiguous paths are rejected with candidate paths; retry with a returned candidate path

Input options:

- single mode: pass `file`, `structural_path`, and `replacement`
- batch mode: pass `patches = [{ file, structural_path, replacement }, ...]`
- single mode and batch mode are mutually exclusive; non-empty `patches[]` must not be combined with top-level `file`, `structural_path`, `replacement`, or `precondition`
- mixed single/batch input is rejected with `mixed_patch_modes`
- optional stale checks: pass `precondition = { node_hash, file_hash, range }`

Batch rules:

- `atomic` defaults to `true`
- any missing, ambiguous, stale, invalid, or overlapping patch rejects the whole atomic batch before writing
- set `atomic=false` only when partial application is explicitly desired
- same-file patches are applied in descending line order
- overlapping same-file target ranges are rejected
- `max_patches` defaults to 20
- applied results report `previous_node_hash` and `new_node_hash`; use `new_node_hash` for later stale checks
- stale rejections report expected/actual diagnostics such as `expected_node_hash` and `actual_node_hash`
- `precondition.node_hash` checks the current matched node source, `precondition.file_hash` checks the whole file, and `precondition.range` checks the current node line range

Typical route:

1. Confirm owners with `tree`, `rg`, or `ast-detail`.
2. Read exact current implementations with `node-source`.
3. Submit one `patches[]` batch with full replacement functions.
4. Validate with TestKit or the project-specific check.
