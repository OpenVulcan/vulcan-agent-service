# `vulcan-testkit-run`

Use this entry when the next action is validation: build, test, check, lint, or typecheck.

Prefer this over a normal shell command when output may be long. The returned content is a Markdown report designed for AI reading, not JSON and not raw stdout/stderr.

Preferred run mode:

- `program`: `cargo`
- `args`: `test`
- `cwd`: project root
- `phase`: `test`
- `tool_hint`: `cargo`

Analyze-only mode:

- `log`: existing validation output
- `phase`: `check`
- `tool_hint`: `cargo`

Local debug note:

- With `--call-tools`, prefer `program` + `args` for real validation fixtures.
- Inline `log` is useful, but shell quoting can mutate diagnostics that contain quotes or newlines.
- When testing a newly added source skill before syncing `output/skills`, call with `--runtime-root runtime`.
- Run local `--call-tools` fixture checks sequentially when they share the same runtime root.

Returned Markdown sections:

- `Status`
- `Run`
- `Summary`
- `Root Diagnostics`
- `Failed Tests`
- `Source Refs`
- `Collapsed Noise`
- `Next Actions`

Design rules:

- Prefer `program` + `args`; this MVP does not accept arbitrary shell text.
- `program` must be a bare executable name such as `cargo`, `npm`, `node`, or `tsc`; path-like program values are rejected.
- Custom `env` is rejected because environment variables can alter validation tool execution semantics.
- Shell-like programs such as `powershell`, `cmd`, `bash`, and `sh` are blocked; call validation tools directly.
- App/runtime profiles such as `cargo run`, `go run`, `node app.js`, `npm start`, dev servers, and watchers are blocked even when hidden behind global options, workspaces, filters, or `--` passthrough args.
- Mutation, installation, publishing, project creation, interactive debugging, browser-opening, benchmark, fuzz, fix, and write modes are outside TestKit validation scope.
- Options are intentionally profile-aware and case-sensitive; examples include `go -C <dir> test` as allowed routing and `go -c`, `python -i`, `node -e`, `tsc --init`, and `ruff format` without `--check` as blocked profiles.
- Unsupported tools should use analyze-only `log` mode until an adapter profile is added.
- The tool does not return full stdout/stderr by default.
- Raw output is parsed inside the same call, then collapsed into root diagnostics and source refs.
- Use `tool_hint` only when automatic detection is insufficient; unknown or path-like hints are rejected before adapter loading.

Allowed profile shape:

- Rust: `cargo check`, `cargo test`, `cargo clippy`, or `cargo build`; `run`, `bench`, `install`, `clean`, `new`, `update`, `--config`, and `--fix` are blocked.
- Go: `go test`, `go vet`, or `go build`; `run`, `install`, `generate`, `env`, `mod`, benchmark, fuzz, output-binary, and external executor modes are blocked.
- Python: `python -m pytest`, `python -m unittest`, `python -m mypy`, or `python -m ruff`; `-c`, `-i`, bytecode-writing modules such as `py_compile` and `compileall`, unknown modules, pytest debug/watch/coverage/report modes, mypy install-types, and ruff mutation modes are blocked.
- Node: `node --check <file>` or `node --test ...`; direct script execution, eval, inspect, run-script, reporter destination, snapshot update, and watch modes are blocked.
- TypeScript: `tsc --noEmit ...` or version checks; explicit noEmit false values, init, build, watch, and trace-output modes are blocked.
- JS package managers: `npm|pnpm|yarn test`, allowlisted `run` scripts (`build`, `check`, `lint`, `test`, `typecheck`, `type-check`, `test:unit`), and constrained `exec` tools (`tsc --noEmit`, `vitest run`, `jest`, `eslint`); install, dev/start/serve/preview/watch, snapshot update, coverage/output-file, eslint init/cache/output-file, fix/write/open, and unknown exec tools are blocked.

Adapter extension:

- Command profiles live in `runtime/profiles/*.lua` and are registered by `runtime/profiles/index.lua`.
- Keep language/tool-specific allowlists, blocklists, and custom validation logic inside the matching profile file.
- Log detectors live in `runtime/detectors/*.lua` and are registered by `runtime/detectors/index.lua`.
- Keep language/tool-specific log routing heuristics inside the matching detector file.
- Keep `runtime/vulcan-testkit-run.lua` focused on request normalization, profile dispatch, detector dispatch, execution, adapter dispatch, and Markdown rendering.
- Parsers live in `runtime/adapters/*.lua` and are loaded only when selected.
- Add a new tool family by adding a profile file, a detector file when analyze-only logs need auto-routing, listing both in their indexes, registering any new adapter key in the core adapter allowlist, mapping its parser file when needed, and returning a parser function from the adapter module.
