# codeview_ast Guide

`codeview_ast` should be the first inspection step when you need structural code context.

Recommended workflow:
1. Start with the narrowest path that still covers the task.
2. Keep `recursive` disabled unless subdirectories are required.
3. Prefer `ext` filters for large repositories.
4. Keep `comment=false` unless comment context is necessary.
5. Reuse `cache_id + page` when the response is paginated.

Important limits:
- Matched files are capped at 5000.
- Explicit file paths are capped at 20.
- Do not mix explicit file paths and directory paths in one request.
- `truncate_chars` defaults to 20000 and should stay unchanged unless the user explicitly asks for a different budget.

Interpretation tips:
- Use line spans from the AST summary to decide which files deserve deeper reads.
- Treat pagination as part of normal usage for large repositories.
- When truncation is reported by the client, request the remaining pages before making architecture-level decisions.
