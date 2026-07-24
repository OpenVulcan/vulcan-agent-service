# vulcan-curl

AI-native HTTP request skill for Vulcan agents.

Chinese version: [README.zh-CN.md](README.zh-CN.md)

`vulcan-curl` provides an AI-friendly HTTP request layer for Vulcan agents. It offers simple structured `GET` and `POST` entries for common API calls, webhook checks, and download diagnostics, plus a lower-level supported curl-style argv entry for advanced request shapes, all without requiring agents to assemble platform-specific shell commands.

## When To Use

Use `vulcan-curl` when an agent needs to make one API/debug HTTP request with predictable inputs and readable Markdown output:

- Inspect API responses, webhook deliveries, headers, auth behavior, or downloadable files from an HTTP endpoint.
- Send structured query params and headers without hand-built URL strings.
- Use Bearer or Basic Auth shortcuts.
- Send JSON, form, multipart, or raw POST bodies.
- Save response bodies or response headers to local files.
- Use supported curl-style argv semantics for advanced request shapes while avoiding shell quoting problems.

Do not use `vulcan-curl` as a webpage-fetching or scraping tool when the real goal is to read rendered page content or convert HTML into Markdown. Use a browser tool for interactive page flows, JavaScript-rendered UI testing, screenshots, or webpage inspection. Use a normal shell command only when raw terminal curl behavior is the actual thing being tested.

## Tools

### `vulcan-curl-get`

Use this entry for simple API/debug HTTP reads.

Common inputs include:

- `url`
- `params` or `params_list`
- `headers` or `header_lines`
- `bearer`, `basic`, or `basic_text`
- `timeout_ms`
- `follow_location`
- `download_to`
- `save_headers_to`
- `flags`

Example:

```yaml
url: https://api.example.com/items
params:
  q: lua
headers:
  Accept: application/json
flags: response-header
```

### `vulcan-curl-post`

Use this entry for simple write-style requests with one main payload family.

Supported payload families:

- `json`
- `form` / `form_lines`
- `files` / `file_lines`
- `body`

Only one payload family should be used per request. Switch to `vulcan-curl-request` when the request shape no longer fits the structured POST schema.

Example:

```yaml
url: https://api.example.com/items
json:
  name: example
bearer: "${TOKEN}"
flags: request-header,response-header
```

### `vulcan-curl-request`

Use this entry when you need supported curl-style argv semantics without depending on platform-specific shell quoting.

Example:

```yaml
args:
  - -X
  - POST
  - https://api.example.com/items
  - -H
  - "Content-Type: application/json"
  - -d
  - '{"name":"example"}'
flags: response-header
```

This entry is best for advanced TLS, proxy, multipart upload, retry, and unusual API/debug request combinations covered by the runtime parser. It still executes inside the Lua runtime layer rather than through a shell.

`args` is a supported curl-style subset parsed by the Lua runtime, not full curl CLI compatibility. Shell expansion, stdin/TTY interaction, interactive prompts, curl config files, and unsupported dash-prefixed curl options are not available; unknown curl options fail fast.

## TLS And Proxy Certificates

On Windows, `vulcan-curl` enables libcurl native CA lookup for target TLS and HTTPS proxy TLS when the request does not provide explicit CA files. This lets proxy certificates chain to the Windows trust store without disabling verification.

On Linux and macOS, libcurl uses the CA bundle or trust backend selected by the host build. If an HTTPS proxy uses a private or enterprise CA that is not visible to that backend, pass `--proxy-cacert` or `--proxy-capath` through `vulcan-curl-request`. Use `--cacert` or `--capath` for the target server certificate chain, and reserve `--proxy-insecure` for temporary diagnostics.

## Output Controls

`flags` is a comma-separated render flag string:

- `request-header`: include request details
- `response-header`: include response headers

Unknown flags are ignored. Response bodies are shown inline unless `download_to` is used, and the inline body is always the raw HTTP response rather than rendered webpage content or Markdown-converted page text. Response headers can be saved with `save_headers_to`.

## Runtime Requirements

`vulcan-curl` expects the host Lua runtime to provide:

- `lcurl.safe`
- `socket`

These runtime modules are not bundled in this repository. The skill package contains only the LuaSkill runtime code, help content, and release metadata.

## Skill Package Layout

```text
vulcan-curl/
├─ skill.yaml
├─ dependencies.yaml
├─ README.md
├─ README.zh-CN.md
├─ runtime/
│  ├─ shared_http.lua
│  ├─ vulcan-curl.lua
│  ├─ vulcan-curl-get.lua
│  └─ vulcan-curl-post.lua
├─ schemas/
│  ├─ get.input.schema.json
│  ├─ post.input.schema.json
│  └─ request.input.schema.json
├─ help/
│  ├─ help.md
│  ├─ get.md
│  ├─ post.md
│  └─ request.md
├─ overflow_templates/
├─ resources/
├─ licenses/
├─ scripts/
└─ .github/workflows/
```

## Validation

Local repository validation:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

The packaging script generates release artifacts under `dist/`:

- `vulcan-curl-v<version>-skill.zip`
- `vulcan-curl-v<version>-checksums.txt`

Optional source metadata:

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

The generated metadata points to the matching `LuaSkills/vulcan-curl` GitHub release assets unless `--base-url` is provided.

## Release Flow

Releases are tag-driven. A pushed tag matching `v*` triggers the release workflow, and the tag must match `skill.yaml.version`.

Recommended local release steps:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
.\scripts\tag_release.ps1 0.1.6
```

Or on Unix-like shells:

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.6
```

## Notes

- The repository root is the skill root.
- The installed skill id is derived from the package root directory name: `vulcan-curl`.
- Runtime output is designed for AI agents: structured, concise by default, and explicit about request/response rendering choices.
