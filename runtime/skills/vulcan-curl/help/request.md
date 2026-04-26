# `vulcan-curl-request`

Use this workflow when you want Linux curl-style argv semantics without depending on the shell's quoting rules.

Best for:

- complex headers and flags
- advanced TLS or proxy options
- unusual request combinations
- cases where `get` and `post` are too restrictive

Output defaults:

- pass `flags` as a comma-separated string outside `args`, for example `{"args":["https://example.com"],"flags":"request-header,response-header"}`
- spaces around commas are allowed, for example `{"flags":"request-header , response-header"}`
- request details are hidden unless `flags` contains `request-header`
- response headers are hidden unless `flags` contains `response-header`
- unknown flags are ignored
- `-i` and `--include` remain supported as compatibility switches for response headers

You pass raw curl-style arguments, but execution still happens inside the Lua runtime layer.
