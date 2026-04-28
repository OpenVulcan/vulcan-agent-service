# vulcan-lua

AI-native Lua execution bridge for Vulcan agents.

Chinese version: [README.zh-CN.md](README.zh-CN.md)

`vulcan-lua` provides a controlled Lua runtime bridge for small, explicit tasks. It lets agents run either inline Lua code or one existing Lua script file with structured arguments, predictable validation, and concise Markdown output.

## When To Use

Use `vulcan-lua` when an agent needs a bounded Lua task that benefits from the host runtime context:

- Run a short inline Lua snippet for one-off data processing.
- Execute an existing `.lua` script as a reusable task.
- Pass structured `args` into a Lua script without assembling shell-specific quoting.
- Inspect the current client context and result limits exposed by Vulcan.
- Prototype or debug small LuaSkill helper logic through the same controlled runtime bridge.

Use dedicated file, CodeKit, TestKit, curl, or shell tools when the task is primarily file editing, code structure navigation, validation, HTTP, or general command execution.

## Tool

### `vulcan-lua-run`

Use this entry to run exactly one Lua task. Provide exactly one of `code` or `file`.

Inline mode:

```yaml
task: summarize numbers
code: |
  local total = 0
  for _, value in ipairs(args.values) do
    total = total + value
  end
  return { total = total, count = #args.values }
args:
  values: [1, 2, 3]
```

File mode:

```yaml
task: run local helper
file: scripts/helper.lua
args:
  input: example
timeout_ms: 60000
```

`file` mode switches the execution working directory to the script directory and injects:

- `vulcan.context.entry_file`
- `vulcan.context.entry_dir`

## Runtime Behavior

The tool validates the request before execution. Invalid inputs return a `Runtime Input Error` report instead of entering the Lua runtime.

Key behavior:

- `code` and `file` are mutually exclusive and exactly one is required.
- `args` is exposed inside the script as the local variable `args`.
- `print(...)` output is captured in the returned Markdown.
- Table returns are rendered as formatted JSON text.
- Multiple return values are displayed in order.
- Results include a `Current Client Context` section with caller metadata and active result/read limits.

Runtime guardrails:

- `vulcan.runtime.lua.exec`, `vulcan.runtime.log`, and `vulcan.cache.*` are disabled inside the executed environment.
- The tool cannot recursively call itself through the current execution bridge.
- `vulcan.call(name, args)` remains available for composition, but it should not become the default path for general automation.

## Skill Package Layout

```text
vulcan-lua/
├─ skill.yaml
├─ dependencies.yaml
├─ README.md
├─ README.zh-CN.md
├─ runtime/
│  └─ vulcan-lua-run.lua
├─ help/
│  └─ help.md
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

- `vulcan-lua-v<version>-skill.zip`
- `vulcan-lua-v<version>-checksums.txt`

Optional source metadata:

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

The generated metadata points to the matching `LuaSkills/vulcan-lua` GitHub release assets unless `--base-url` is provided.

## Release Flow

Releases are tag-driven. A pushed tag matching `v*` triggers the release workflow, and the tag must match `skill.yaml.version`.

Recommended local release steps:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
.\scripts\tag_release.ps1 0.1.0
```

Or on Unix-like shells:

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.0
```

## Notes

- The repository root is the skill root.
- The installed skill id is derived from the package root directory name: `vulcan-lua`.
- Runtime code has no bundled external tool dependency.
- Runtime output is designed for AI agents: bounded, explicit, and stable enough to use as a small Lua execution primitive.
