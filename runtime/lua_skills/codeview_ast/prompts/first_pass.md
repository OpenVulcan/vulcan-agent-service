Use `codeview_ast` before any raw file reads or grep-style searches.

Target: {{target}}
Goal: {{goal}}
Constraints: {{constraints}}

Execution plan:
1. Run `codeview_ast` on the target with the smallest safe scope.
2. Review the returned symbol map and line spans before choosing files to inspect.
3. If the result is paginated, continue with `cache_id` and `page` instead of rerunning the scan.
4. Only after the structural pass is complete should you open specific files or search text.
