# `vulcan-runtime-lua-exec`

Use this workflow when you need short inline Lua code and do not want to create a standalone file first.

Recommended situations:

- one-off loops
- quick data conversion
- temporary file generation
- lightweight network probing
- small command orchestration

Typical route:

1. Read `vulcan-runtime` main help first.
2. Prepare `{ code, args?, timeout_ms? }`.
3. Keep the script focused on one task and return one final value.

Notes:

- the runtime captures `print(...)`
- the tool always returns one Markdown string
- call `vulcan.process.exec` only when Lua itself is not enough
