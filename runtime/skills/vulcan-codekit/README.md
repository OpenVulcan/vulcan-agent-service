# Vulcan CodeKit

Languages: English | [简体中文](README.zh-CN.md)

`Vulcan CodeKit` is not positioned as "yet another AST tool". Its core purpose is to turn code structure into context that an Agent can reason about and continue acting on.

**Traditional AST tools help humans navigate code. CodeKit helps Agents understand code.**

It is not a generic toolkit that glues together `grep`, `AST`, and `LSP`. It is an Agent-native intermediate layer for code understanding.

The current LuaSkills naming scheme uses the canonical `skill_id-entry_name` form, so the recommended tool names are:

- `vulcan-codekit-ast-tree`
- `vulcan-codekit-ast-detail`
- `vulcan-codekit-rg`
- `vulcan-codekit-markdown-menu`
- `vulcan-codekit-node-source`
- `vulcan-codekit-patch`

In some MCP clients or host bindings, tool names may be exposed with underscores, such as `vulcan_codekit_ast_tree`. This is only a naming difference at the exposure layer. Semantically, it still maps to the same CodeKit entry points.

CodeKit is closer to a "structured code understanding protocol" designed for Agents and advanced development workflows:

- Build a structured map first
- Inspect symbol views next
- Use text anchors to trace back to owner context
- Modify safely only after structural boundaries are clear

If you have worked on large repositories, you know the real time sink is usually not "typing code", but things like:

- Not knowing which directory to inspect
- Finding a keyword without knowing who owns it
- Opening a 4000-line file and getting lost while scrolling
- Reading a pile of source files before realizing the change point was elsewhere

The goal of `Vulcan CodeKit` is direct:

**Upgrade code understanding from blind searching to "read the structure map first, then work".**

## What Problem Does It Solve?

Traditional tools are powerful:

- `grep` is fast
- `LSP` is precise
- `AST` is structured
- IDEs are excellent

But most of them are designed for humans exploring step by step. Humans have IDE context, long-term project memory, and spatial navigation ability, so traditional tools tend to provide point capabilities such as jumping, completion, diagnostics, and references.

In Agent scenarios, the missing piece is usually not more raw information. The missing piece is a higher-density, more reason-friendly, execution-ready structured intermediate result. Agents need to build a project map first, then judge file responsibility, symbol ownership, reading priority, and modification boundaries.

`Vulcan CodeKit` fills exactly that gap:

- It does not merely tell you "which line matched"
- It cares about "which function, which `impl`, and which structural context owns this line"
- It does not merely give you a file list
- It cares about "which files in this directory are worth inspecting next"
- It does not merely dump source code
- It cares about "extract the structural outline first, then decide what to read or edit"

In one sentence:

**It is not designed for manual click-by-click browsing. It is designed for programmable, reason-friendly, execution-ready code understanding workflows.**

In other words, CodeKit does not dump raw AST directly into the model. Raw AST nodes are too fine-grained, too noisy, and not task-oriented enough. CodeKit compresses them into directory maps, symbol skeletons, owner context, and safe patch targets that are easier for Agents to consume.

## Why It Feels Like Next-generation Development Infrastructure

Because its output is not just something humans can look at. It is something Agents can continue working with.

That makes it naturally suited for:

- AI Coding Agents
- MCP Tooling
- Automated code review
- Structured code navigation
- Precise reading under context budgets
- On-demand analysis of large repositories
- Safe function-level replacement

In other words, CodeKit is not a simple replacement for:

- `grep`
- `rg`
- `ctags`
- An IDE panel

It reorganizes these capabilities into a workflow protocol that is easier for Agents to consume.

## Core Capabilities

### `vulcan-codekit-ast-tree`

Read the map before reading the details.

It returns a directory-level grouped view with compact metrics for each candidate file, such as:

- Line count
- Type count
- `impl` count
- Function count
- Method count
- Top-level symbol summary

Useful when:

- You do not know the project structure
- You do not know which directory to start from
- You only want to quickly build a "code map"

### `vulcan-codekit-ast-detail`

Once a specific file is known, this does not dump the whole file at you. It expands the structural skeleton first.

It returns:

- Top-level `struct` / `enum` / `type`
- `impl` blocks
- Method lists
- Function signatures
- Line ranges

Useful when:

- Pre-reading the structure of very long files
- Judging module responsibility
- Locating modification boundaries precisely
- Inspecting the skeleton before reading actual source

### `vulcan-codekit-rg`

This is the part of the toolset that tends to feel immediately addictive.

Normal search tells you:

- "The keyword appears on this line"

`vulcan-codekit-rg` tells you:

- "The keyword appears on this line"
- "This line belongs to this function"
- "This function belongs to this `impl` or structural context"

That means:

- A log message
- An error string
- A function name
- A configuration key
- A protocol field

can all be traced back to the real owner very quickly.

For large repositories, this is not a small improvement. It is a change in efficiency class.

`rg_pattern` uses ripgrep's Rust regex engine by default. Set `regex_engine` to `pcre2` only when the pattern needs PCRE2 features such as look-around or backreferences. The optional `extensions` filter accepts comma-separated extensions or language names; when omitted, CodeKit scans the exact default source-code set declared in `skill.yaml`, excluding non-core formats such as css, html, json, yaml/yml, hcl/tf/tfvars, and md.

### `vulcan-codekit-markdown-menu`

Read the document heading tree first, then decide which body text to open.

It is useful for:

- Helping Agents locate the right document quickly
- Navigating large Markdown trees
- Avoiding dumping a whole pile of Markdown bodies into context at the start

### `vulcan-codekit-node-source`

After `ast-detail` or `rg` has confirmed a target function or method, this reads the complete source of one or more nodes by `structural_path`, including cross-file batches.

It returns:

- Matched file
- Structural path count
- Each `structural_path`
- Function or method signature
- Line range
- Complete node source
- Per-node `ok` / `missing` / `ambiguous` / `duplicate` / `skipped` / `error` status
- `node_hash` and `file_hash`
- `overflow_mode: truncate`

Useful for:

- Carefully reading the current implementation before patching
- Reviewing one or more owner functions instead of a whole file
- Avoiding a fallback to full-file reads just to get a function body

Node reads consistently use `nodes[]`:

- Each node item carries its own `file` and `structural_path`
- For multiple nodes in the same file, repeat the same `file`
- For a more compact form, put multiple structural paths in one node item's `structural_path`, separated by lines
- For cross-file reads, use different `file` values in different node items

`structural_path` is a slash-separated structural path suffix, not a regex or glob. Examples include `main`, `UserService/get_user`, `impl MyType/new`, and `impl MyTrait for MyType/run`.

It returns partial success. A missing structural path, ambiguous structural path, missing file, or invalid structural path format does not discard every successful node. Single-node issues are reported with `status: error` and `node_index`.

By default, it processes up to 20 nodes. Repeated hits to the same node are marked as `duplicate`, and requests beyond the limit are marked as `skipped`.

### `vulcan-codekit-patch`

Once the target is known at function level, this replaces one or more functions or methods structurally.

It is not "casual text replacement". It performs complete function or method replacement around AST targets: locate the target by `structural_path`, write the replacement, rescan the AST, and reject results that introduce parse-error nodes. Single mode and batch mode are mutually exclusive: use either top-level `file`/`structural_path`/`replacement` or non-empty `patches[]`, not both. In batch mode, `atomic=true` is the default. If any patch is missing, ambiguous, stale, not a complete function replacement, or overlaps another range in the same file, the entire batch is rejected before writing.

Useful for:

- Replacing one or more complete functions after the owner is clear
- Fixing a handler, helper, or test in one batch
- Avoiding manual line drift in large files
- Making function-level changes more controlled

Its boundaries are also clear:

- It is not for scattered local text replacements
- `replacement` must be complete function or method source
- Batch input uses `patches = [{ file, structural_path, replacement }, ...]`
- You can pass `precondition = { node_hash, file_hash, range }` for stale checks
- Successful results distinguish `previous_node_hash` from `new_node_hash`; later stale checks should use `new_node_hash`
- Stale rejections return expected and actual diagnostic fields so callers can judge the current source state
- If a structural path matches multiple candidates, candidates are returned instead of modifying blindly

## A More Agent-friendly Code Workflow

In `Vulcan CodeKit`, the recommended path is usually not:

1. Search full text first
2. Open files everywhere
3. Scroll while guessing

Instead, it is:

1. Build a map with `ast-tree`
2. Inspect skeletons with `ast-detail`
3. Use anchors with `rg` to trace owner context
4. Fetch precise node source with `node-source`
5. Apply structural batch replacement with `patch`

That is:

**Shrink the problem space before reading. Confirm structural ownership before editing.**

This approach is especially valuable for:

- Unfamiliar repositories
- Large repositories
- Multi-layer modular projects
- Very long single files
- Languages where structural boundaries matter, such as Rust, Go, and TypeScript
- Agents that need to save context budget

## A Very Practical Comparison

In a real comparison test, an AI was asked to analyze the Codex source code, understand the full project, and find an issue.

| Scenario | Total time | API interactions | Tool calls |
| --- | ---: | ---: | ---: |
| With CodeKit AST tools | About 4 minutes | 11 | 10+ |
| Without CodeKit AST tools | About 20 minutes | 54 | 110 |

The difference is not merely "search is a bit faster". Without CodeKit, Agents often have to repeat this loop:

- Use `rg` and get several hits
- Open each file one by one
- Remain unsure about ownership
- Switch back and forth
- Scroll repeatedly
- Locate the true entry point only at the end

With CodeKit, the workflow becomes:

- Know the directory and candidate files first
- Know the structural outline inside a file next
- Know which function and which `impl` owns a keyword
- Read only the few code regions that truly matter

This kind of improvement is often not just "a bit faster". It changes the workflow:

**From manual puzzle assembly to structured navigation.**

More importantly, it reduces context pollution. Agents no longer need to ingest large amounts of irrelevant source code just to build a project map, and later judgments stay cleaner.

## This Is Not Another grep Wrapper

If all you want is text search, many tools can do that.

The real difference in `Vulcan CodeKit` is that it splits code understanding into stages that are better suited for automation:

- Map stage
- Structure stage
- Owner location stage
- Safe replacement stage

That is why it is better suited as:

- Infrastructure for Agent Runtime
- An advanced code tool layer for MCP platforms
- A structured code navigation backend for IDEs and AI assistants
- Part of automated review and patch workflows

## Why We Are Confident In It

Because in real development, the most expensive things are not CPU, AST, or search speed. They are:

- Context budget
- Attention
- Direction
- Avoiding wrong assumptions

`Vulcan CodeKit` saves those expensive resources.

It helps Agents read less irrelevant code, edit fewer wrong places, get lost less often in huge files, and avoid wandering through project structure.

This is not a small user-experience tweak. It is a structural upgrade to the code development workflow.

## Who It Is For

- Teams building AI Coding Agents
- Platforms that want to enhance local Agents with code understanding
- Developers who want to strengthen the code analysis layer in MCP tool systems
- Engineering teams that want to upgrade code search into "structured search"
- Senior developers working with large repositories, multi-module systems, and long files

## Included Tools

- `vulcan-codekit-ast-tree`
- `vulcan-codekit-ast-detail`
- `vulcan-codekit-rg`
- `vulcan-codekit-markdown-menu`
- `vulcan-codekit-node-source`
- `vulcan-codekit-patch`

## Standalone Repository Notes

This repository is the standalone source repository for the `vulcan-codekit` LuaSkill. Its contents correspond to the official skill package in the LuaSkills runtime:

- `runtime/`: LuaSkill tool entry points and shared runtime code
- `rules/`: ast-grep structural matching rules split by language
- `help/`: strict help flows and per-tool documentation
- `skills/`: Codex skill instructions and Agent usage guidance
- `ast-grep-ffi/`: Rust-based ast-grep FFI dynamic library project
- `scripts/`: validation and packaging scripts for skill packages and FFI release artifacts

This repository is no longer maintained as a demo skill. It is the release source for `vulcan-codekit`. Releases produce two types of artifacts:

- LuaSkill package: includes `runtime/`, `rules/`, `help/`, `skills/`, `dependencies.yaml`, and other runtime files
- FFI component package: includes the platform-specific `vulcan_codekit_ast_grep_ffi` dynamic library

## Dependencies and Release Artifacts

`dependencies.yaml` declares runtime dependencies. Current dependencies are split into two categories:

- `rg`: still provided as a tool dependency for text search and Markdown file enumeration
- `ast-grep-ffi`: provided as an FFI dependency for AST structure scanning, structural matching, and patch validation

The current implementation no longer calls the raw `ast-grep` CLI. Instead, it uses the dynamic library built from `ast-grep-ffi/`. The Lua runtime loads the platform-specific dynamic library from the FFI dependency directory injected by LuaSkills. During local development, it also falls back to `ast-grep-ffi/target/release` and `ast-grep-ffi/target/debug`.

The current release workflow builds and publishes FFI components only for these platforms:

- `macos-arm64`
- `macos-x64`
- `linux-arm64`
- `linux-x64`
- `windows-x64`

The `windows-arm64` component is not published for now, and that platform is not declared in `dependencies.yaml`. Adding a new platform requires updating the release matrix, `dependencies.yaml`, validation scripts, and README together.

## Release Flow

This repository follows the GitHub Release installation rules used by LuaSkills. The skill package is a standard LuaSkill package, with release asset names:

- `vulcan-codekit-v{version}-skill.zip`
- `vulcan-codekit-v{version}-checksums.txt`

The top-level directory inside the zip archive must be the runtime skill name:

- `vulcan-codekit/`

`ast-grep-ffi` is not bundled into the skill package itself. It is installed as a GitHub Release dependency through `dependencies.yaml`. The current accurate Release repository is:

```yaml
repo: LuaSkills/vulcan-codekit
```

The `Release Vulcan CodeKit LuaSkill` GitHub Actions workflow supports tag pushes and manual runs. The release version is read from `skill.yaml`; an optional manual `version` input may be provided only when it matches `v{skill.yaml.version}`.

- `build_luaskill=on/off`: whether to build and upload the LuaSkill package
- `luaskill_runner`: runner used to build the skill package
- Platform-specific `*_runner` values: runner for each FFI platform, or `off` to skip that platform

LuaSkill package builds and FFI native component builds can be run separately. As long as the release tag matches `skill.yaml.version`, all enabled artifacts are uploaded to the same GitHub Release. During runtime installation of FFI components, the LuaSkills dependency manager resolves the matching asset from the same Release according to the `version`, `repo`, and platform `asset_name` values in `dependencies.yaml`.

The Rust FFI dependency license report is generated automatically by `cargo-deny`:

```powershell
python .\scripts\generate_cargo_deny_notices.py
```

The result is written to `THIRD_PARTY_LICENSES.md`. CI runs `cargo deny check -c deny.toml --exclude-dev licenses` under `ast-grep-ffi/` to check license policy, and verifies that the report still matches the current dependency graph.

## One-sentence Summary

**If traditional code tools answer "where is this text", `Vulcan CodeKit` cares more about "where is the structure, who owns it, and what should be read or edited next".**

**Traditional AST tools help humans navigate code. CodeKit helps Agents understand code.**

That is why it is not just a toolset, but a layer of code understanding infrastructure prepared for the Agent era.
