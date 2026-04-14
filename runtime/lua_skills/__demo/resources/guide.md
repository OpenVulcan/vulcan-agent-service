# __demo Skill Guide

This directory is intentionally prefixed with `__`, so the loader skips it.

Copy workflow:
1. Duplicate the `__demo` directory.
2. Rename the directory to a real skill name without the `__` prefix.
3. Update `skill.json`.
4. Replace the sample Lua files, resources, prompts, and templates with your actual logic.

Template coverage:
- `main.lua`: sample tool entry
- `resources/guide.md`: static resource example
- `resources/dynamic_guide.lua`: Lua-generated resource example
- `templates/example.md`: static resource-template example
- `templates/example_generator.lua`: Lua-generated resource-template example
- `prompts/static_prompt.md`: static prompt example
- `prompts/dynamic_prompt.lua`: Lua-generated prompt example
