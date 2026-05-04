# `vulcan-codekit-rg`

Use this workflow when you already have a text anchor and need to find the owning structure.

Best for:

- log strings
- regex clues
- function or method names
- owner-context discovery

Regex behavior:

- `rg_pattern` uses ripgrep's Rust regex engine by default
- set `regex_engine = "pcre2"` only when the pattern needs PCRE2 features such as look-around or backreferences
- `extensions` is optional; when omitted, the default source-code set is exactly the list declared in `skill.yaml` for this tool
- the default source-code set excludes css, html, json, yaml/yml, hcl/tf/tfvars, and md

Typical route:

1. Start from the clue.
2. Run `rg`.
3. Use the returned owner context to decide whether `ast-detail` is needed next.
