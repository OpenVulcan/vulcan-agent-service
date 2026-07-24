# Skill Debugging Guide

## 1. Purpose

`--call-tools` is a local debugging entry that initializes configuration, shared cache, LuaEngine, and Lua skills **without starting HTTP or gRPC services**, then directly invokes a target tool.

This mode is useful for:

- verifying whether a new Lua skill loads correctly
- checking whether skill dependencies are initialized correctly
- validating tool argument parsing, result shape, and error handling
- debugging local analysis tools such as `vmcp-ast` and `vmcp-rg`

It is **not** a replacement for full MCP integration testing. If you need to validate HTTP, Streamable, or gRPC transport behavior, use the normal service mode.

## 2. Command Format

General format:

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools <tool_name> '<json_arguments>'
```

Meaning:

- `--call-tools`: enter local tool debug mode
- `<tool_name>`: tool name, for example `vmcp-ast` or `vmcp-rg`
- `<json_arguments>`: JSON object passed to the tool

If the tool takes no arguments, the third segment can be omitted:

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools current_time
```

## 3. Recommended Workflow

### 3.1 Build the debug output first

```powershell
.\make.ps1 build
```

This syncs:

- `output/debug/vulcan-agent-service.exe`
- `output/lua_runtime/skills/`
- `output/lua_runtime/lua_packages/`
- `output/configs/`

For local tool debugging, use `output/debug/vulcan-agent-service.exe`.

### 3.2 Run the tool directly

Example for `vmcp-rg`:

These examples assume the current working directory is the repository root.

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\runtime\\lua_runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

Example for `vmcp-ast`:

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-ast '{"path":".\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua","comment":false}'
```

## 4. Argument Format

### 4.1 Arguments must be valid JSON

Correct:

```powershell
'{"path":".\\src","recursive":true,"ext":"rs"}'
```

Incorrect:

```powershell
'{path:"src"}'
```

Why incorrect:

- keys are missing double quotes
- backslashes are not escaped correctly

### 4.2 On Windows, wrap the whole JSON payload in single quotes

Recommended PowerShell form:

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

This reduces escaping problems.

## 5. Common Examples

### 5.1 Debug a declaration hit in `vmcp-rg`

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\src","ext":"rs","rg_pattern":"struct ExecRequest"}'
```

Expected behavior:

- `rg` finds the declaration
- only the related structural node is returned

### 5.2 Debug a function-body hit in `vmcp-rg`

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-rg '{"dir":".\\runtime\\skills\\vulcan-codekit","ext":"lua","rg_pattern":"invalid_ext_argument"}'
```

Expected behavior:

- `rg` matches text inside a function body
- the nearest enclosing function structure is returned
- the matched lines are attached below that structure

### 5.3 Debug structural output in `vmcp-ast`

```powershell
.\output\debug\vulcan-agent-service.exe --call-tools vmcp-ast '{"path":".\\src\\main.rs","comment":false}'
```

Expected behavior:

- structural output is returned directly
- no HTTP or gRPC services are started

## 6. Output Shape

`--call-tools` prints the tool result directly to stdout, usually as JSON.

Example `vmcp-rg` result:

```json
{
  "files_scanned": 1,
  "files_with_matches": 1,
  "items_found": 1,
  "rg_matches": 3,
  "files": [
    {
      "file": "<repo_root>\\runtime\\skills\\vulcan-codekit\\runtime\\codekit-ast-tree.lua",
      "lines": 2087,
      "content": "local function validate_extension_argument(value) ... L744-803"
    }
  ]
}
```

Important fields:

- `files_scanned`: number of files analyzed by AST
- `files_with_matches`: number of files that produced final structural matches
- `rg_matches`: raw ripgrep match count
- `files[].content`: final rendered structural output

## 7. Troubleshooting

### 7.1 `Unknown Lua skill tool`

Possible reasons:

- the tool name is wrong
- the tool is not registered in `skill.json`
- `output/lua_runtime/skills` is not synced to the latest version

Fix:

```powershell
.\make.ps1 build
```

Then retry.

### 7.2 Missing dependency errors

If a skill declares `dependencies.yaml`, the loader checks its versioned tool dependencies during load under:

- `output/lua_runtime/dependencies/tools/`

If the required binary is missing, make sure the build and dependency sync flow has completed.

Also keep dependency tools and the host controller separate:

- `output/lua_runtime/dependencies/tools/`
  - versioned skill command-line tools such as `rg` and `ast-grep`
- `output/lua_runtime/bin/vldb-controller.exe`
  - the database controller executable
  - this is not part of the versioned dependency tool tree

If the skill being debugged touches SQLite or LanceDB, also verify that:

- you have already run `make deps` and `make build`
- `output/lua_runtime/bin/vldb-controller.exe` exists
- when `space_controller.auto_spawn=true`, `space_controller.endpoint` is a locally spawnable address
- when using a remote controller, `auto_spawn=false` is set and the remote controller is already running
- if you manually replace the controller binary, its release tag still matches the `vldb-controller-client` version locked by the current repository

### 7.3 Results differ from the currently running service

`--call-tools` uses the **current local build output**, which may not match a separately running `output/bin` process.

Recommended distinction:

- `output/debug/vulcan-agent-service.exe --call-tools ...`: local functional debugging
- `output/bin/vulcan-agent-service.exe`: the running service instance

### 7.4 Why does this mode avoid HTTP and gRPC startup?

Because its purpose is to shorten the debugging path and validate only:

- configuration loading
- LuaEngine initialization
- skill loading
- tool return values

This avoids transport-layer noise, port conflicts, and unrelated external service dependencies.

## 8. Debugging Advice

Recommended order:

1. validate the tool logic first with `--call-tools`
2. then validate protocol and client integration in normal MCP service mode

For AST / rg-based tools, prefer:

- a small target directory
- an explicit extension filter
- a specific regex

That makes it easier to tell whether the issue comes from:

- `rg` finding nothing
- AST recognition missing the structure
- or the hit-to-structure mapping not behaving as expected
