-- 中文：这是主工具入口示例，演示一个 skill 下的第一个 tool 应如何组织。
-- English: This is the primary tool entry example, showing how the first tool inside a skill can be organized.

return function(args)
  -- 中文：读取可选参数，并在缺失时提供稳定默认值。
  -- English: Read optional arguments and provide stable defaults when omitted.
  local display_name = tostring((args and args.name) or "demo-user")
  local emphasis = tostring((args and args.emphasis) or "replace this section with your own business logic")

  -- 中文：返回结构化结果，便于直接观察工具输出形态。
  -- English: Return a structured result so the tool output shape is easy to inspect.
  return {
    ok = true,
    message = "Hello from the primary __demo tool. Copy this directory, rename the skill, and replace each sample file with your real logic.",
    name = display_name,
    emphasis = emphasis,
    template_hint = "This file is the primary tool entry. Use another Lua file when you add a second tool with different behavior.",
  }
end
