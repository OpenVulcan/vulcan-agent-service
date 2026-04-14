-- 中文：这是一个可复制的示例 skill 入口，用于演示最小工具逻辑。
-- English: This is a copy-ready demo skill entry that shows the minimum tool logic.

return function(args)
  -- 中文：读取可选参数，并在缺失时提供稳定默认值。
  -- English: Read optional arguments and provide stable defaults when omitted.
  local display_name = tostring((args and args.name) or "demo-user")
  local emphasis = tostring((args and args.emphasis) or "replace this with your own business logic")

  -- 中文：返回结构化结果，便于直接观察工具输出形态。
  -- English: Return a structured result so the tool output shape is easy to inspect.
  return {
    ok = true,
    message = "Hello from __demo. Copy this directory, rename the skill, and replace the sample files.",
    name = display_name,
    emphasis = emphasis,
  }
end
