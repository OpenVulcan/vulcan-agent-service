# `vulcan-curl-get`

Use this workflow for simple GET requests where you want structured arguments instead of raw curl argv.

Best for:

- straightforward HTTP reads
- query params
- simple auth headers
- small downloads

Output defaults:

- pass `flags` as a comma-separated string, for example `{"flags":"response-header"}` or `{"flags":"request-header,response-header"}`
- spaces around commas are allowed, for example `{"flags":"request-header , response-header"}`
- request details are hidden unless `flags` contains `request-header`
- response headers are hidden unless `flags` contains `response-header`
- unknown flags are ignored
- `include_headers=true` remains supported as a compatibility switch for response headers

Switch to `vulcan-curl-request` when you need supported curl-style flags or request control beyond this structured GET schema.
