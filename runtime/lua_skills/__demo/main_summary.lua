-- 中文：这是第二个可复制的示例 tool 入口，用于演示同一个 skill 下的多工具拆分写法。
-- English: This is the second copy-ready tool entry, showing how to split multiple tools inside one skill.

return function(args)
  -- 中文：读取一个更简化的参数集合，突出“不同 tool 可以拥有不同参数契约”。
  -- English: Read a smaller parameter set to show that each tool can define its own parameter contract.
  local display_name = tostring((args and args.name) or "summary-user")

  -- 中文：返回不同于主入口的字段结构，帮助开发者快速识别“多入口不是同一个结果换名字”。
  -- English: Return a shape different from the primary entry so developers can see that multi-entry tools are not just renamed duplicates.
  return {
    ok = true,
    summary = "This is the secondary __demo tool entry. Copy this file when you want a separate Lua implementation for another tool.",
    target_name = display_name,
    next_step = "Rename the tool, adjust the parameters, and replace the returned fields with your business data.",
  }
end
