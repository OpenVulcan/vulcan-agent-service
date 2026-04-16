---
name: vulcan-codekit
description: Use the Vulcan CodeKit MCP tools when Codex needs to choose the right code files before deep inspection, map a text clue back to the owning function or class, navigate Markdown docs by headings, or replace an entire function safely. Trigger especially when the target file is not yet known, when exact files need AST detail, when a symbol/log/regex clue must be narrowed to structure, or when a whole function must be replaced with AST-based targeting.
---

# Vulcan CodeKit

Use this skill to choose the right `codekit-*` tool from your current state.

The main agent should build the project map first, then decide whether deeper inspection, text narrowing, Markdown navigation, patching, or subagent delegation is needed.

## Behavior Principles

### Think before coding

- State assumptions when they materially affect the implementation.
- If multiple interpretations would change the solution, surface them before coding.
- Prefer the simplest approach that fully satisfies the request.
- If something important is unclear, pause and name the uncertainty instead of guessing silently.

### Simplicity first

- Write the minimum code that solves the requested problem.
- Do not add speculative features, abstractions, or configurability.
- Avoid defensive branches for unsupported or purely hypothetical scenarios.
- If the solution feels heavier than the problem, simplify it.

### Keep changes surgical

- Touch only the files and lines required by the request.
- Do not refactor adjacent code unless the task requires it.
- Match the surrounding style and structure.
- Remove only the unused code created by your own change, not unrelated pre-existing dead code.

### Verify against a concrete outcome

- Translate the request into a checkable outcome before implementing.
- After each meaningful step, verify with the best available check.
- Prefer tests, focused reads, or concrete command checks over vague confidence.

## Quick Decision Tree

When analyzing code, ask these questions in order:

1. **I do not know the target file yet**
   Use `codekit-ast-tree`.
   Start from one directory and build a compact map before reading details.

2. **I already have exact file paths**
   Use `codekit-ast-detail`.
   Inspect file-level AST structure, symbols, and signatures.

3. **I have a function name, keyword, log string, or regex clue**
   Use `codekit-rg`.
   Narrow text matches back to the owning function, method, impl, or class context.

4. **I need to find the right Markdown doc or section**
   Use `codekit-markdown-menu`.
   Read heading structure first, then open body text only when needed.

5. **I need to replace an entire function or method**
   Use `codekit-patch`.
   Only do this after the target function is already confirmed.

For source-code analysis, the decision tree above is the default route unless one of the narrow fallback cases below explicitly applies.

## Mandatory Tool Routing

CodeKit is the required default path for source-code analysis.

1. **Project mapping**
   For unfamiliar repositories or non-trivial source directories, `codekit-ast-tree` is required before deep inspection.
   Do not start codebase exploration with plain file listing or plain file reading tools.

2. **Structural search**
   When searching source code for symbols, functions, methods, classes, log strings, or regex anchors, `codekit-rg` replaces plain grep-style search.
   Owner context is required unless the task is a pure literal lookup.

3. **File inspection**
   When inspecting source files, `codekit-ast-detail` replaces plain file reading unless the file is trivial and structure is irrelevant.
   You should prefer signatures, owners, and symbol boundaries over raw text.

4. **Standard-tool fallback is allowed only if**
   - the target is a pure config file such as `.json`, `.yaml`, `.toml`, or `.env`
   - the task is pure filename or extension discovery
   - the task is a literal text lookup where ownership does not matter
   - the edit is a tiny non-structural text change
   - the directory is explicitly tiny and already understood

## Output Prediction

- `codekit-ast-tree` returns a grouped Markdown tree with compact metrics such as lines, types, impl blocks, and functions.
- `codekit-rg` returns matched lines together with the owning function, method, impl, or class context.
- `codekit-ast-detail` returns a structured symbol tree with nesting, signatures, and line ownership.
- If output exceeds the safe inline limit, you will receive a `raw_file` pointer together with host-safe `offset` / `limit` read chunks. Follow that chunk plan directly.

## Main-Agent Rule

For unfamiliar repositories, the main agent should call `codekit-ast-tree` first and build the global map itself.

Do this before:

- deciding whether subagents are needed
- assigning subagents to specific files or modules
- asking any worker to "go understand the project"

Why:

- the main agent needs the map in its own context
- a subagent exploring first does not give the main agent the same durable global view
- precise delegation becomes much easier after the main agent already knows the structure

## Tool Notes

### `codekit-ast-tree`

Use when:

- the repository or subdirectory is unfamiliar
- the goal is to choose candidate files first

Remember:

- pass exactly one directory
- keep ignore rules enabled by default
- add `ext` only when the tree is too noisy
- after reading it, you should be able to explain which modules exist and what they appear to own

### `codekit-ast-detail`

Use when:

- the exact files are already known
- file-level structure matters more than raw text

Remember:

- pass explicit file paths only
- do not pass directories
- keep `comment=false` unless condensed note summaries truly help
- prefer this over plain file reading when signatures, owners, or symbol boundaries matter

### `codekit-rg`

Use when:

- there is already a text anchor
- the answer depends on which function or class owns the match

Remember:

- this is not the first-pass exploration tool
- keep `show_full_function=false` unless the exact body is needed
- prefer this over plain grep when a clue may need owner context

### `codekit-markdown-menu`

Use when:

- Markdown files must be triaged by headings
- the right doc or section is still unknown

Remember:

- it is for headings, not body summarization
- keep `recursive=false` for the first docs pass

### `codekit-patch`

Use when:

- a full function or method must be replaced safely

Remember:

- `replacement` must be the complete function source
- do not use it for partial edits or scattered tweaks

## Typical Workflows

### Unknown codebase

1. Run `codekit-ast-tree` on the most relevant source directory.
2. Pick candidate files from the grouped output.
3. Run `codekit-ast-detail` on those exact files.
4. If a symbol or keyword becomes important, switch to `codekit-rg`.

### Known symbol or keyword

1. Run `codekit-rg` with the text clue.
2. Confirm the owning function or class.
3. If more structure is needed, open the exact file with `codekit-ast-detail`.

### Safe function replacement

1. Use `codekit-rg` or `codekit-ast-tree` to locate the right function owner.
2. Use `codekit-ast-detail` to inspect the exact current implementation.
3. Use `codekit-patch` for the full-function replacement.
4. Re-check with `codekit-rg` or `codekit-ast-detail`.

## Subagent Boundary

Subagents are good for:

- precise edits on already identified files or functions
- isolated test/build/format tasks
- execution work that does not require the main agent to discover structure

Subagents are not good for:

- first-pass project understanding
- vague exploration
- any task where the main agent still lacks the architecture map

## Failure and Fallback

- If `codekit-ast-tree` fails because large-result cache writing fails, retry once with a smaller directory or narrower `ext`. If needed, fall back to file search plus direct reads.
- If `codekit-rg` returns too many matches, narrow the regex or shrink the directory scope before calling again.
- If `codekit-ast-detail` rejects the input, first confirm that the input is an explicit file list rather than a directory.

## Boundaries

Standard tools are exceptions, not the default path for code analysis.

Only fall back when CodeKit adds near-zero value, such as:

- pure config files like JSON, YAML, TOML, or env files
- exact literal search where ownership or symbol context does not matter
- direct reading of a very small known non-code file
- simple filename or extension discovery
- tiny non-structural text edits that do not benefit from AST ownership context

For source code:

- use `codekit-rg` instead of plain grep when searching for symbols, methods, logs, or patterns
- use `codekit-ast-detail` instead of plain file reading when inspecting code structure
- use `codekit-ast-tree` before deep inspection when the file set is not already known

CodeKit is most valuable when the task depends on **function-, class-, impl-, or type-level structure**, and that is the default assumption for code analysis.
