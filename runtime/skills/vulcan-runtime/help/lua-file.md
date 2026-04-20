# `vulcan-runtime-lua-file`

Use this workflow when the logic is easier to maintain in an existing `.lua` file.

Recommended situations:

- reusable scripts
- multi-step runtime logic
- scripts that rely on file-relative paths
- scripts that are clearer in standalone source form

Typical route:

1. Read `vulcan-runtime` main help first.
2. Prepare `{ file, args?, timeout_ms? }`.
3. Let the runtime switch cwd to the script directory automatically.

Notes:

- `vulcan.context.entry_file` and `vulcan.context.entry_dir` point to the target script
- the tool always returns one Markdown string
- this is the better choice once the inline version starts growing too large
