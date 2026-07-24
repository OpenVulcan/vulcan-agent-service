# `vulcan-codekit-ast-detail`

Use this workflow when you already know the exact file paths and need structural detail instead of raw text.

Best for:

- symbol inventories
- function and impl ownership
- file-level AST structure
- narrowing the exact edit target before patching

Typical route:

1. Confirm the file path first.
2. Run `ast-detail`.
3. Decide whether to keep reading, switch to `rg`, or move to `patch`.
