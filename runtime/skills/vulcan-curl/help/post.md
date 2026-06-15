# `vulcan-curl-post`

Use this workflow for simple API/debug POST-style requests with one main payload family.

Best for:

- JSON POST bodies
- multipart form uploads
- simple raw body requests
- authenticated API writes
- webhook delivery tests

Output defaults:

- pass `flags` as a comma-separated string, for example `{"flags":"response-header"}` or `{"flags":"request-header,response-header"}`
- spaces around commas are allowed, for example `{"flags":"request-header , response-header"}`
- request details are hidden unless `flags` contains `request-header`
- response headers are hidden unless `flags` contains `response-header`
- unknown flags are ignored
- `include_headers=true` remains supported as a compatibility switch for response headers
- response bodies are returned as raw HTTP content; this workflow does not render webpages or convert HTML to Markdown

Switch to `vulcan-curl-request` when the request shape needs supported curl-style flags beyond the structured POST schema. Use a browser-oriented tool instead when the goal is webpage fetching, rendered page inspection, or page-content extraction.
