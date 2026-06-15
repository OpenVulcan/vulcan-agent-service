# Vulcan File

Simplified Chinese: [README.zh-CN.md](README.zh-CN.md)

`Vulcan File` is not "just another filesystem API wrapper." Its core purpose is to give agents a stable, low-noise, execution-friendly protocol for working with raw text files.

**CodeKit helps agents understand code structure; File helps agents handle raw files safely.**

It is not solving the basic question of whether files can be read. It solves the repeated costs agents run into during real engineering work:

- Avoid writing platform-specific shell snippets just to list files.
- Read a precise slice after the target file and line area are known.
- Read multiple non-adjacent snippets in one call for comparison.
- Make a small text edit only after seeing a preview.
- Avoid wasting context budget on generated files, dependency directories, and ignored paths.

The current LuaSkills naming model uses the canonical `skill_id-entry_name` form, so the recommended tool names are:

- `vulcan-file-list`
- `vulcan-file-read`
- `vulcan-file-create`
- `vulcan-file-edit`
- `vulcan-file-delete`

Some MCP clients or host bindings may expose the same tools with underscores, such as `vulcan_file_read`. That is only a naming difference at the exposure layer; the semantics still map to the same File entries.

It is closer to a small text-file workbench built for agents and automated engineering flows:

- First shrink the candidate set with a low-token file map.
- Then read raw text with explicit line numbers when the file already exists.
- Then preview new files with `create` or small changes with `edit`.
- Then preview regular-file removals with `delete` when lifecycle cleanup is needed.
- Finally apply only when the preview matches the intended change.

In one sentence:

**Find the file, read the evidence, preview the create, edit, or delete step, then apply.**

Path convention:

- Root-level `PWD` is the shared project or workspace root for relative `file` or `path` arguments.
- When `PWD` is omitted, empty, or not an existing directory, File does not fall back to the runtime cwd; pass absolute paths instead.

## What Problem It Solves

Traditional tools can absolutely do these jobs:

- `ls` can list files.
- `find` can enumerate directories.
- `sed`, `awk`, and `PowerShell` can extract line ranges.
- Shell redirection can modify files.

But those tools are mostly shaped for humans at a command line. Humans know the current shell, platform differences, encoding boundaries, and contextual risks. They can also remember the file positions they just inspected.

In agent workflows, the missing piece is not more raw capability. It is a more stable operation shape. Agents need:

- Compact output that remains useful for follow-up reasoning.
- Stable line numbers for citations and later edits.
- Clear parameter errors that can be corrected automatically.
- Ignore-aware defaults that reduce context pollution.
- Preview-first editing to avoid accidental writes.

`Vulcan File` fills that gap:

- It does not only say "what is in this directory."
- It cares about which candidate files are worth reading next.
- It does not only dump whole files into context.
- It cares about returning line-addressable raw evidence.
- It does not only write files.
- It cares about producing an auditable preview before an explicit write.

## Boundary With CodeKit

`Vulcan File` does not replace `Vulcan CodeKit`.

Their relationship is more like this:

- CodeKit handles structure understanding, owner lookup, function-level source extraction, and structural patching.
- File handles normal text-file discovery, precise raw reads, and small text edits.

When a task needs functions, classes, `impl` blocks, Markdown heading trees, AST structure, or whole-function replacement, prefer CodeKit.

When the task is already clear enough to say "read these lines from this file" or "make this small text edit in this file," File is more direct and lightweight.

In short:

**CodeKit is for understanding structure; File is for handling raw text.**

## Core Capabilities

### `vulcan-file-list`

Use this when the target file is not known yet and you need a low-token file map first.

It returns a compact directory-grouped file list, supports basename-only filename globs, and respects a common `.gitignore` / `.ignore` subset plus built-in high-noise directory ignores by default.

Good fits:

- Build a quick file map before entering an unfamiliar directory.
- Filter candidates when the extension or filename shape is known.
- Avoid ad hoc shell loops, sorting, and ignore-rule handling.
- Narrow the search space before reading file contents.

Typical parameters:

- `path`: scan root; pass the narrowest plausible directory. Path values may include `${env:NAME}` placeholders.
- `PWD`: optional shared project or workspace root. Relative `path` values are resolved from `PWD` when it points to an existing directory; otherwise `path` must already be absolute. If `path` is omitted, the scan starts from `PWD`.
- `pattern`: basename-only filename glob, such as `*.lua`, `*.md`, or `Cargo.*`; use `path` instead of `src/*.lua` or `**/*.md` when narrowing directories.
- `recursive`: recursive by default; set to `false` for direct children only.
- `noignore`: set to `true` only when ignored or generated files are intentionally needed; this disables both ignore files and built-in high-noise directory skips.
- `limit`: cap the number of returned candidate files; defaults to 1000 and accepts up to 100000.

This is not a content search tool. If you have a log line, error string, function name, or text anchor, use `vulcan-codekit-rg` first.

Ignore handling is not a full Git ignore engine; complex escapes and some advanced negation cases are not guaranteed to match Git exactly.

### `vulcan-file-read`

Use this after the file path and approximate line area are already known.

It supports one root-level file request or a `files` batch of up to 10 items. In batch mode, root `numbered` acts as the default for every item unless an item overrides it.

Prefer the structured `segments` array when the client supports full JSON Schema:

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "segments": [
    { "start": 5, "count": 10 },
    { "start": 25, "count": 30 }
  ]
}
```

Each segment reads `count` lines starting from the 1-based `start` line. Multi-segment reads are rendered in request order, and overlapping ranges are not merged implicitly.

`lines_rule` remains available as a legacy fallback for clients that cannot send arrays. It still uses `start,count` format:

```text
5,10
25,30
```

In JSON arguments, separate multiple segments with `\n` inside the string:

```json
{
  "PWD": "/workspace/project",
  "file": "src/example.lua",
  "lines_rule": "5,10\n25,30"
}
```

The separator is a real newline in the JSON string, not the literal word `"newline"`. Do not send `segments` and `lines_rule` together; when the client supports array schemas, prefer `segments`.

The result includes:

- File path
- Total line count
- Byte count
- Newline style
- Displayed line ranges
- Segment count
- Whether the request was clipped at EOF
- Batch separators when multiple file requests are executed in one call

By default, it keeps stable prefixes such as `L12:` so later review comments, citations, or `vulcan-file-edit` calls can refer to exact lines. Set `numbered=false` when plain raw text lines are more useful; the metadata header and multi-segment separators still remain.

Boundary behavior is explicit:

- `start` and `count` must be positive integers.
- `segments` must be a non-empty array and each item must contain positive-integer `start` and `count` fields.
- A `start` beyond the total line count returns a parameter error.
- A `count` that extends past EOF is clipped and marked in the header.
- Omitting `lines_rule` reads the beginning of the file using the host `file_read` budget, with a 200-line fallback when the host provides no budget.
- A literal `"newline"` token in `lines_rule` returns `invalid_lines_rule`; use `\n` in the JSON string instead.
- Sending both `segments` and `lines_rule` returns `conflicting_range_arguments`.
- Sending both root-level single-file arguments and `files` returns `conflicting_batch_arguments`.
- Directory paths are only for a quick direct-child name listing; recursive discovery belongs in `vulcan-file-list`.
- Path values may include `${env:NAME}` placeholders, which are expanded with Lua `os.getenv` before filesystem access.

This is not a tool for guessing through pages. If the text location is unknown, search or list candidates first.

### `vulcan-file-create`

Use this when you need to create one brand-new file or a small batch of brand-new files and want preview-first behavior instead of writing immediately.

Typical fits:

- The target file does not exist yet.
- The final file path and full file content are already known.
- The workflow should distinguish clearly between creating a new file and editing an existing one.
- The host should receive one canonical `change_set` create record when structured host results are enabled.

Typical parameters:

- `PWD`: optional shared project or workspace root. Relative `file` values are resolved from `PWD` when it points to an existing directory; otherwise `file` must already be absolute.
- `file`: exact target file path; `${env:NAME}` placeholders are supported.
- `content`: complete content of the new file; `""` is allowed and creates an empty file.
- `files`: optional batch form with up to 10 `{ file, content }` objects; do not send it together with root `file`/`content`.
- `apply`: leave false for preview, set true only when the preview is correct.

Boundary behavior is explicit:

- Existing targets return `file_already_exists`; they are never overwritten silently.
- Missing parent directories return `parent_directory_not_found`.
- Parent paths that exist but are not directories return `parent_path_not_directory`.
- Preview output shows the creation as a plus-only diff block and is truncated after 80 preview lines.
- Batch mode accepts at most 10 items, and one `apply` flag controls the whole batch.

### `vulcan-file-edit`

Use this for small text edits after the target file and target lines have been confirmed.

It supports one root-level edit request or a `files` batch of up to 10 items. Batch mode is useful when several known files need coordinated preview-first text changes in one call.

It previews by default and does not write. A write only happens when `apply=true` is passed explicitly.

The root `PWD` parameter may point to the current project or workspace root. When `PWD` is valid, relative `file` values resolve from it; otherwise `file` must already be absolute. `${env:NAME}` placeholders are still expanded with Lua `os.getenv` before filesystem access.

Supported modes:

- `overwrite`: replace the whole file. For backward compatibility it still creates the file when it does not exist, but `vulcan-file-create` is now the preferred entry for brand-new files; `content=""` creates or leaves an empty file.
- `append`: append at the end of the file as new lines; if the original file is non-empty and lacks a final newline, one is inserted before the appended content.
- `replace_range`: replace an existing 1-based closed line range; `content=""` deletes that range.
- `insert_before`: insert before an existing 1-based anchor line.
- `insert_after`: insert after an existing 1-based anchor line.
- `files`: optional batch form with up to 10 per-file edit objects; do not send it together with root single-file edit arguments.

`insert_before` and `insert_after` require `1 <= line <= total_lines`. Out-of-range anchors return `line_out_of_bounds`; they do not silently append. For empty files, use `overwrite` to create content or `append` for file-end additions.

The result includes:

- Whether the status is `PREVIEW_ONLY` or `APPLIED`
- Original and edited line counts
- Original affected span
- Edited affected span
- Operation-oriented diff preview
- A canonical host `change_set` when structured host results are enabled by the host
- Clear correction hints for parameter errors

It deliberately avoids complex structural reasoning. Use `vulcan-codekit-patch` for whole-function or whole-method replacement. Use CodeKit first when source structure must be understood before editing.

### `vulcan-file-delete`

Use this when you need to preview and remove one regular file or a small batch of regular files, and the host should receive canonical delete metadata when supported.

Typical parameters:

- `file`: exact regular-file path to remove; `${env:NAME}` placeholders are supported.
- `PWD`: optional shared project or workspace root. Relative `file` values are resolved from `PWD` when it points to an existing directory; otherwise `file` must already be absolute.
- `files`: optional batch form with up to 10 `{ file }` objects; do not send it together with root `file`.
- `apply`: leave false for preview, set true only when the preview is correct.

Boundary behavior is explicit:

- Missing targets return `file_not_found`.
- Directory removal is not supported; directory paths return `directory_delete_unsupported`.
- Text-like files return line-oriented delete previews and host delete content.
- Host `change_set` delete records follow the official content modes: up to 500 lines they return `content_mode="full"` with full `content`, and above 500 lines they proactively switch to `content_mode="truncated"` with `total_line_count`, `content_head`, and `content_tail` for the first and last 50 lines.
- Binary or non-text files use the stable placeholder `Binary file` and report one removed line for preview purposes.

## A Better File Workflow For Agents

In `Vulcan File`, the recommended path is usually not:

1. List directories with shell snippets.
2. Guess a file and open the whole thing.
3. Copy a small script to edit the file.
4. Check afterward whether the wrong thing changed.

Instead:

1. Use `list` to get a candidate file map.
2. Use `read` to inspect exact target lines when the file already exists.
3. Use `create` to preview brand-new files, `edit` to preview small changes to existing files, or `delete` to preview regular-file removal.
4. Use `create apply=true`, `edit apply=true`, or `delete apply=true` only after the preview is correct.

In other words:

**Turn file operations into reviewable steps, then let the agent continue.**

This is especially useful for:

- Cross-platform development environments
- Lightweight navigation in large repositories
- Small edits to README files, configuration files, scripts, and plain text assets
- Review or repair work that needs precise line-number evidence
- Agents that need to conserve context budget
- Workflows where invisible default writes are unacceptable

## A Practical Comparison

Without `Vulcan File`, agents often fall back to temporary commands:

- `Get-ChildItem` / `find`
- `Select-String` / `grep`
- `sed -n`
- `awk`
- Ad hoc file-writing scripts

Those commands work, but each call requires a fresh decision:

- What platform is this?
- Should the scan be recursive?
- Should ignore rules be respected?
- Will the output be too long?
- Can the line format be cited directly?
- Is there a clear enough preview before editing?

With `Vulcan File`, these concerns are organized into four stable entries:

- File candidates: `list`
- Raw evidence: `read`
- Brand-new files: `create`
- Small text changes: `edit`

This is not shell functionality with a different name. It is a small protocol that makes common file actions safer for agents to call.

## Who It Is For

- Teams building AI coding agents
- Platforms adding a basic file-operation layer to MCP tooling
- Local agent runtimes that need stable cross-platform file behavior
- Automated workflows that frequently touch README, config, script, and resource files
- Engineering teams that want preview-first editing as the default discipline

## Included Tools

- `vulcan-file-list`
- `vulcan-file-read`
- `vulcan-file-create`
- `vulcan-file-edit`

## Repository Notes

This repository is the standalone source repository for the `vulcan-file` LuaSkill package. It maps to the published skill package used by the LuaSkills runtime:

- `runtime/`: LuaSkill tool entries
- `schemas/`: AI-facing input schema files
- `help/`: strict help flows and per-tool guidance
- `overflow_templates/`: reserved local overflow-template directory
- `resources/`: reserved resource directory
- `licenses/`: third-party notices
- `scripts/`: validation, packaging, and release helpers

This repository is no longer maintained as a demo skill. It is the release source for `vulcan-file`. Releases generate the standard LuaSkill artifacts:

- `vulcan-file-v{version}-skill.zip`
- `vulcan-file-v{version}-checksums.txt`

The top-level directory inside the zip must be the runtime skill name:

- `vulcan-file/`

## Dependencies And Artifacts

`dependencies.yaml` declares runtime dependencies. Currently, `vulcan-file` declares no external tool, Lua, or FFI dependencies.

That means its runtime behavior is backed entirely by filesystem, path, and runtime interfaces provided by the LuaSkills host. It does not require `rg`, native dynamic libraries, or third-party Lua packages.

Local validation:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

Optional source metadata:

```powershell
python .\scripts\package_skill.py --emit-source-yaml
```

The generated metadata points to the matching `LuaSkills/vulcan-file` GitHub Release assets by default. Pass `--base-url` for another distribution channel.

## Release Flow

This repository follows the LuaSkills GitHub Release installation rules. Pushing a tag matching `v*` triggers the release workflow, and the tag version must match `skill.yaml.version`.

Recommended local release steps:

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
.\scripts\tag_release.ps1 0.1.3
```

Unix-like shell:

```bash
python ./scripts/validate_skill.py
python ./scripts/package_skill.py
./scripts/tag_release.sh 0.1.3
```

## One-Sentence Summary

**If CodeKit answers "where is the structure, and who owns this code," then `Vulcan File` answers "where is the file, what is the raw text, and has this edit been previewed."**

**CodeKit helps agents understand code structure; File helps agents handle raw files safely.**

That is why it is not just a set of file tools. It is a lightweight file-operation layer for the agent era.
