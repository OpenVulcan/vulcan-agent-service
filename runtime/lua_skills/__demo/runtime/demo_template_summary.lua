-- This is the second copy-ready tool entry, showing how to split multiple tools inside one skill.
-- 这是第二个可复制的示例 tool 入口，用于演示同一个 skill 下的多工具拆分写法。

return function(args)
  -- Read a smaller parameter set to show that each tool can define its own parameter contract.
  -- 读取一个更简化的参数集合，突出“不同 tool 可以拥有不同参数契约”。
  local display_name = tostring((args and args.name) or "summary-user")

  -- Demonstrate the optional multi-return overflow protocol while still returning plain text content.
  -- 演示可选的多返回值超限协议，同时仍以纯文本内容作为第一返回值。
  local content = table.concat({
    "# Demo Template Summary",
    "",
    "This is the secondary `__demo` tool entry.",
    "",
    "- target_name: `" .. display_name .. "`",
    "",
    "Use this file when you want a separate Lua implementation for another tool.",
    "If your real tool may produce large output, you can opt into host overflow handling by returning additional values.",
  }, "\n")

  return content, vulcan.runtime.overflow_type.truncate
end
