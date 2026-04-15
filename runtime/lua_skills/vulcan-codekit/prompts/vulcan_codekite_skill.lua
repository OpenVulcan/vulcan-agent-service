--[[
中文：根据调用方传入的 task 参数动态生成 CodeKit 最佳实践提示词，
并在末尾追加“用户当前指令”，让支持 MCP prompts 的客户端拿到带上下文的公共工作流文本。
English: Build the CodeKit best-practice prompt dynamically from the optional task argument
and append it as the user's current instruction so MCP prompt-capable clients receive contextual guidance.
]]

local function trim(text)
    return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

return function(args)
    local prompt_args = (type(args) == "table" and type(args.arguments) == "table") and args.arguments or {}
    local task = trim(prompt_args.task or "")
    if task == "" then
        task = "Analyze the current project, quickly understand its structure, and prepare for the user's next instruction."
    end

    local base_prompt = [[# Vulcan CodeKit

Use this skill to choose the right `codekit-*` tool from your current state.

The main agent should build the project map first, then decide whether deeper inspection, text narrowing, Markdown navigation, patching, or subagent delegation is needed.

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

If the task is only:

- finding file names
- doing a lightweight string search
- reading one small file
- making line-level edits such as comments or tiny renames

prefer standard tools such as `glob`, `grep_search`, `read_file`, or ordinary edits instead of CodeKit.

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

### `codekit-rg`

Use when:

- there is already a text anchor
- the answer depends on which function or class owns the match

Remember:

- this is not the first-pass exploration tool
- keep `show_full_function=false` unless the exact body is needed

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

Do not reach for CodeKit when plain tools are enough.

Prefer lighter tools for:

- simple file discovery
- simple string search
- reading a small known file
- line-based edits
- very small repositories where reading files directly is cheaper

CodeKit is most valuable when the task depends on **function-, class-, impl-, or type-level structure**.]]

    return table.concat({
        base_prompt,
        "",
        "## Current User Instruction",
        "",
        task,
    }, "\n")
end
