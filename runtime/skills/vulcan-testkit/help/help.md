# `vulcan-testkit`

Vulcan TestKit is the AI validation router for noisy build, test, check, lint, and typecheck output.

Prefer it over shell when a validation command may produce long output. It executes or analyzes in one tool call, parses adapters internally, folds repeated or cascading noise, and returns a compact Markdown diagnostic report instead of full stdout/stderr.

Run mode is intentionally profile-limited. TestKit is for bounded validation commands, not app/runtime commands.
The profile guard checks global options, subcommands, package-manager scripts, `exec` targets, and passthrough args before execution so validation cannot quietly become a server, installer, project creator, formatter write, benchmark, fuzz run, or interactive debugger.

Use it:

- after code edits before claiming verification
- when build/test/check/lint/typecheck output may be long
- when an existing validation log needs root-cause compression
- when the next step should be derived from source refs and failed tests

Do not use it for general shell automation, git, file operations, installs, deployments, or destructive commands.
Do not use it for `cargo run`, `go run`, `node app.js`, `npm start`, dev servers, watchers, fix/write modes, browser-opening modes, fuzzing, benchmarks, or other commands that may mutate files or run indefinitely.

Available topic:

- `run`: Run one explicit validation executable or analyze one provided log, then return Markdown diagnostics.
