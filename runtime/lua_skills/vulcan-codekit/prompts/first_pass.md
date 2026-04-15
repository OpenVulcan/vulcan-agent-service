Use `codekit-ast` before any raw file reads or grep-style searches.

Target: {{target}}
Goal: {{goal}}
Constraints: {{constraints}}

Execution plan:
1. Run `codekit-ast` on the target with the smallest safe scope.
2. Review the returned symbol map and line spans before choosing files to inspect.
3. If the result is large, prefer `export_md_path` or a narrower scope instead of rerunning the same broad scan.
4. Only after the structural pass is complete should you open specific files or search text.
