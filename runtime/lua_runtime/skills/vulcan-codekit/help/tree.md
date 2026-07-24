# `vulcan-codekit-ast-tree`

Use this workflow first when the repository or source directory is still unfamiliar.

Best for:

- building the project map
- choosing candidate files
- identifying the heavy modules before deep reads

Typical route:

1. Run `ast-tree` with `dir` set to the most relevant directory.
2. Pick candidate files from the grouped output.
3. Follow with `ast-detail` or `rg` on a narrower target.

Input notes:

- `dir` must be exactly one existing directory path
- files, newline-separated path lists, and multiple directories are rejected
- `extensions` is optional; when omitted, the default source-code set is exactly the list declared in `skill.yaml` for this tool
- the default source-code set excludes css, html, json, yaml/yml, hcl/tf/tfvars, and md
