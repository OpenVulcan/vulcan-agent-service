-- This is the primary tool entry example, showing how the first tool inside a skill can be organized.
-- 这是主工具入口示例，演示一个 skill 下的第一个 tool 应如何组织。

return function(args)
  -- Read optional arguments and provide stable defaults when omitted.
  -- 读取可选参数，并在缺失时提供稳定默认值。
  local display_name = tostring((args and args.name) or "demo-user")
  local emphasis = tostring((args and args.emphasis) or "replace this section with your own business logic")

  -- Return one Markdown string because normal tools must no longer return Lua tables directly.
  -- 返回一段 Markdown 字符串，因为普通工具现在不应再直接返回 Lua table。
  return table.concat({
    "# Demo Template Tool",
    "",
    "Hello from the primary `__demo` tool.",
    "",
    "- name: `" .. display_name .. "`",
    "- emphasis: `" .. emphasis .. "`",
    "",
    "This template demonstrates the current rule set:",
    "",
    "- tool output should be a string",
    "- Markdown is the most natural default return format",
    "- if you need overflow handling, return extra values such as `return content, vulcan.runtime.overflow_type.page, \"overflow_page.md\"`",
  }, "\n")
end
