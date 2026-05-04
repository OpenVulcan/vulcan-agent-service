# vulcan-testkit

AI-native validation router for Vulcan coding agents.

Chinese version: [README.zh-CN.md](README.zh-CN.md)

`vulcan-testkit` helps AI agents run bounded build, test, lint, check, and typecheck workflows without flooding the conversation with raw terminal output. It accepts explicit validation commands or an existing log, applies profile guards, parses noisy diagnostics, and returns a compact Markdown report focused on root causes and next actions.

## When To Use

Use `vulcan-testkit` when an agent needs validation feedback that is accurate, bounded, and easy to act on:

- Run build, test, lint, check, or typecheck commands after code edits.
- Compress long validation logs into root diagnostics.
- Extract failed tests, source references, collapsed noise, and next actions.
- Avoid pasting huge stdout/stderr logs back into the model context.
- Prevent validation calls from quietly becoming app servers, installers, watchers, debuggers, benchmark runs, fuzzers, or write/fix commands.

Use a normal shell command instead for short, low-noise commands where the raw output is the desired answer. Use dedicated file, Git, or deployment tools for non-validation work.

## Tool

### `vulcan-testkit-run`

Use this entry when the next action is validation.

Run mode requires a direct allowlisted executable and arguments:

```yaml
program: cargo
args:
  - test
cwd: path/to/project
phase: test
tool_hint: cargo
```

Analyze-only mode accepts an existing log:

```yaml
log: "<existing validation output>"
phase: check
tool_hint: cargo
```

Run mode and analyze-only mode are mutually exclusive. Provide `program` for a real validation run or `log` for existing output, but not both.

The returned Markdown report is designed for AI reading and usually includes:

- `Status`
- `Run`
- `Summary`
- `Root Diagnostics`
- `Failed Tests`
- `Source Refs`
- `Collapsed Noise`
- `Next Actions`

## Supported Validation Profiles

TestKit is intentionally profile-limited. It allows validation-oriented command shapes and rejects command shapes that can mutate files, start long-running processes, install packages, publish artifacts, or open interactive tools.

Supported families include:

- Rust: `cargo check`, `cargo test`, `cargo clippy`, `cargo build`
- Go: `go test`, `go vet`, `go build`
- Python: `python -m pytest`, `python -m unittest`, `python -m mypy`, `python -m ruff`, direct `pytest`, direct `mypy`, direct `ruff check`, direct `ruff format --check`
- Node: `node --check <file>`, `node --test ...`
- TypeScript: `tsc --noEmit ...`
- JavaScript test runners: `vitest run`, `vitest --run`, `jest`
- Package managers: constrained `npm`, `pnpm`, and `yarn` validation scripts
- Analyze-only logs: routed through detector and adapter heuristics

Blocked examples include app/runtime commands, package installs, project creation, formatter write/fix modes, browser-opening modes, watchers, dev servers, benchmarks, fuzz runs, and interactive debugging.

## Skill Package Layout

```text
vulcan-testkit/
├─ skill.yaml
├─ dependencies.yaml
├─ README.md
├─ README.zh-CN.md
├─ runtime/
│  ├─ vulcan-testkit-run.lua
│  ├─ adapters/
│  ├─ detectors/
│  └─ profiles/
├─ help/
│  ├─ help.md
│  └─ run.md
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

- `vulcan-testkit-v<version>-skill.zip`
- `vulcan-testkit-v<version>-checksums.txt`

Optional source metadata:

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

The generated metadata points to the matching `LuaSkills/vulcan-testkit` GitHub release assets unless `--base-url` is provided.

## Release Flow

Releases are tag-driven. A pushed tag matching `v*` triggers the release workflow, and the tag must match `skill.yaml.version`.

Recommended local release steps:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
.\scripts\tag_release.ps1 0.1.1
```

Or on Unix-like shells:

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.1
```

## Notes

- The repository root is the skill root.
- The installed skill id is derived from the package root directory name: `vulcan-testkit`.
- Runtime code does not bundle external validation tools; it routes tools already available in the caller environment.
- Runtime output is designed for AI agents: compact, source-oriented, and explicit about blocked or malformed validation calls.
