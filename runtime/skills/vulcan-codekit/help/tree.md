# `vulcan-codekit-ast-tree`

Use this workflow first when the repository or source directory is still unfamiliar.

Best for:

- building the project map
- choosing candidate files
- identifying the heavy modules before deep reads

Typical route:

1. Run `ast-tree` on the most relevant directory.
2. Pick candidate files from the grouped output.
3. Follow with `ast-detail` or `rg` on a narrower target.
