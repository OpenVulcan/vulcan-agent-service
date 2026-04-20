# `vulcan-codekit-patch`

Use this workflow only when the target function or method is already confirmed and the replacement is a full-function replacement.

Best for:

- AST-safe whole-function replacement
- avoiding stale line-based edits
- keeping selector-based targeting precise

Typical route:

1. Confirm the owner with `tree`, `rg`, or `ast-detail`.
2. Inspect the current function structure.
3. Replace the full function with `patch`.
