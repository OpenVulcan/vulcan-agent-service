# `vulcan-file-read`

Use this tool when you already know the target file path and approximate lines and need exact raw text.

Good fits:

- You already found one candidate file with `vulcan-file-list` or `vulcan-codekit-rg`.
- You need the exact text near one known line range.
- You want to read multiple non-adjacent snippets in one call for comparison.
- You want to batch up to 10 known file reads in one call.
- You only need a quick direct-child name listing and can pass a directory path as `file`.

Not a good fit:

- Do not guess through pages when you do not know where the text is; search with `vulcan-codekit-rg` first.
- Prefer `vulcan-codekit` when you need source structure, function boundaries, or class information.
- Use `vulcan-file-list` when you need recursive file discovery.

Parameter choices:

- Single-file mode: send root-level `file`, plus optional `segments`, `lines_rule`, or `numbered`.
- Batch mode: send `files` as an array of objects. Each item must include `file` and may override `segments`, `lines_rule`, or `numbered`.
- Batch mode accepts at most 10 items.
- Root-level `PWD` is optional. When it points to an existing directory, relative `file` values resolve from that root; otherwise every `file` must already be absolute.
- In batch mode, root `numbered` acts as the default for all items unless an item overrides it.
- `segments`: prefer this structured array parameter; each item uses `{ "start": start_line, "count": line_count }`; when omitted, the tool reads from the beginning of the file using the host `file_read` budget, with a 200-line fallback.
- `lines_rule`: legacy fallback for clients that cannot send arrays; it still uses `start,count` and separates multiple rules with `\n` inside the string; do not send it together with `segments`.

Preferred structured array form:

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "segments": [
    { "start": 5, "count": 10 },
    { "start": 25, "count": 30 }
  ]
}
```

Batch form:

```json
{
  "PWD": "/workspace/project",
  "numbered": true,
  "files": [
    {
      "file": "src/example.lua",
      "segments": [
        { "start": 5, "count": 10 }
      ]
    },
    {
      "file": "README.md",
      "lines_rule": "1,20"
    }
  ]
}
```

Legacy `lines_rule` format:

```text
start,count
```

For multiple segments, use one rule per line:

```text
5,10
25,30
```

In JSON arguments, multi-segment rules must use the escaped newline `\n` inside the string:

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "lines_rule": "5,10\n25,30"
}
```

The separator is a real newline in the decoded JSON string, not the literal word `"newline"`. `segments` and `lines_rule` cannot be sent together; prefer `segments` whenever the client supports array schemas.

Boundary behavior:

- `segments` must be a non-empty array and every item must provide positive-integer `start` and `count`.
- `start` and `count` must be positive integers.
- A `start` beyond the total file line count returns `range_out_of_bounds`.
- A `count` that extends past EOF is clipped automatically and marked in the header as `Clipped:true`.
- Overlapping segments are not merged; they are rendered in request order.
- The literal word `"newline"` returns `invalid_lines_rule`; use `\n` in the JSON string instead.
- Sending both `segments` and `lines_rule` returns `conflicting_range_arguments`.
- Sending both root-level single-file arguments and `files` returns `conflicting_batch_arguments`.
- Directory reads include the directory path, entry count, and one `Name` column list.

The response includes the file path, total line count, byte count, newline style, displayed line ranges, segment count, and whether the request clipped at EOF.
