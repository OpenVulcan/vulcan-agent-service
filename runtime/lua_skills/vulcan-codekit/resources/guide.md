# Vulcan CodeKit Guide

`codekit-ast` should be the first inspection step when you need structural code context.

Recommended workflow:
1. Start with the narrowest path that still covers the task.
2. Keep `recursive` disabled unless subdirectories are required.
3. Prefer `ext` filters for large repositories.
4. Keep `comment=false` unless comment context is necessary.
5. Use `export_md_path` when you need a stable Markdown snapshot for later re-reading.

Important limits:
- Matched files are capped at 5000.
- Explicit file paths are capped at 20.
- Do not mix explicit file paths and directory paths in one request.
- Keep `ignore=true` unless you intentionally need ignored source trees.

Interpretation tips:
- Use line spans from the AST summary to decide which files deserve deeper reads.
- When the result is large, prefer exporting a Markdown snapshot instead of repeating the same broad scan.
- When truncation is reported by the client, narrow the scan or read the exported Markdown before making architecture-level decisions.
