# `vulcan-curl-request`

Use this workflow when you want supported Linux curl-style argv semantics for advanced API/debug HTTP requests without depending on the shell's quoting rules.

Best for:

- complex headers and flags
- advanced TLS or proxy options
- multipart form uploads
- unusual request combinations
- cases where `get` and `post` are too restrictive
- API diagnostics that need closer curl-style control

Argument support:

- `args` is a runtime-parsed supported subset, not full curl CLI compatibility
- shell expansion, stdin/TTY interaction, interactive prompts, and curl config files are not available
- unsupported dash-prefixed curl options fail fast instead of being passed through to a system `curl`
- this workflow returns raw HTTP responses and does not render webpages or convert HTML to Markdown

Supported option families:

- method, URL, headers, data/json, `--data-urlencode`, and multipart `-F/--form`
- auth, user-agent, referer, cookie, output, header dump, timeout, retry, proxy, and TLS certificate options
- compatibility switches such as location, insecure, head, get, compressed, fail, silent, show-error, and include

TLS and proxy certificate notes:

- use `--cacert` or `--capath` for the target server certificate chain
- use `--proxy-cacert` or `--proxy-capath` for HTTPS proxy certificate chains
- use `--proxy-insecure` only for temporary proxy TLS diagnostics
- Windows native CA lookup is enabled separately for target TLS and HTTPS proxy TLS when no explicit CA option overrides it

Output defaults:

- pass `flags` as a comma-separated string outside `args`, for example `{"args":["https://example.com"],"flags":"request-header,response-header"}`
- spaces around commas are allowed, for example `{"flags":"request-header , response-header"}`
- request details are hidden unless `flags` contains `request-header`
- response headers are hidden unless `flags` contains `response-header`
- unknown flags are ignored
- `-i` and `--include` remain supported as compatibility switches for response headers

You pass supported curl-style arguments, and execution happens inside the Lua runtime layer. Use a browser-oriented tool instead when the goal is webpage crawling, JS-rendered page inspection, or page-content extraction.
