# `vulcan-codekit-rg`

Use this workflow when you already have a text anchor and need to find the owning structure.

Best for:

- log strings
- regex clues
- function or method names
- owner-context discovery

Typical route:

1. Start from the clue.
2. Run `rg`.
3. Use the returned owner context to decide whether `ast-detail` is needed next.
