# `vulcan-curl-request`

Use this workflow when you want Linux curl-style argv semantics without depending on the shell's quoting rules.

Best for:

- complex headers and flags
- advanced TLS or proxy options
- unusual request combinations
- cases where `get` and `post` are too restrictive

You pass raw curl-style arguments, but execution still happens inside the Lua runtime layer.
