# `vulcan-codekit-patch`

Use this workflow only when the target functions or methods are already confirmed, `node-source` has supplied the current node source, and every replacement is a whole-function or whole-method replacement.

Best for:

- AST-safe whole-function replacement
- batch patching handler/helper/test changes in one call
- avoiding stale line-based edits
- keeping structural_path-based targeting precise
- returning the actual post-write source and local context after a replacement

Structural path syntax:

- `structural_path` is a slash-separated structural path suffix, not a regex or glob
- examples: `main`, `UserService/get_user`, `impl MyType/new`, `impl MyTrait for MyType/run`
- ambiguous paths are rejected with candidate paths; retry with a returned candidate path

Input options:

- single mode: pass `file`, `structural_path`, and `replacement`
- batch mode: pass `patches = [{ file, structural_path, replacement }, ...]`
- single mode and batch mode are mutually exclusive; non-empty `patches[]` must not be combined with top-level `file`, `structural_path`, `replacement`, or `precondition`
- mixed single/batch input is rejected with `mixed_patch_modes`
- optional advanced stale checks: pass `precondition = { node_hash, file_hash, range }`
- `replacement` must be the complete function/method source returned from the declaration line, not a body fragment

Batch rules:

- `atomic` defaults to `true`
- any missing, ambiguous, stale, invalid, or overlapping patch rejects the whole atomic batch before writing
- set `atomic=false` only when partial application is explicitly desired
- same-file patches are applied in descending line order
- overlapping same-file target ranges are rejected
- `max_patches` defaults to 20 and is an advanced batch safety limit
- successful results contain the actual post-write target lines and up to five lines of context before and after the target
- successful Markdown output does not include internal source hashes or runtime request indexes
- stale rejections explain the source-change action; structured host diagnostics may still include expected/actual verification values
- `precondition.node_hash` checks the current matched node source, `precondition.file_hash` checks the whole file, and `precondition.range` checks the current node line range

Boundaries:

- this is a whole function/method patch workflow, not a generic AST patch tool
- non-function symbols such as enum, enum variant/member, struct field, type alias, and statement-level nodes are not supported
- use text editing tools for scattered local edits instead of forcing them through whole-node replacement

Typical route:

1. Confirm owners with `tree`, `rg`, or `ast-detail`.
2. Read exact current implementations with `node-source`.
3. Prepare complete replacement source from the returned node bodies, keeping names and signatures aligned.
4. Submit one `patches[]` batch with full replacement functions.
5. Read the returned post-write code and context as the primary patch verification result.
6. Validate with TestKit or the project-specific check.
