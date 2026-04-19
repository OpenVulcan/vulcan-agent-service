-- English: Demonstrate how one `.lua` help file can generate one help topic or workflow node without host-supplied arguments.
-- 中文：演示如何通过 `.lua` help 文件在不依赖宿主传参的情况下生成一个帮助 topic 或 workflow 节点。

return function()
  return table.concat({
    "# Dynamic Help Example",
    "",
    "This node demonstrates one Lua-generated help topic.",
    "",
    "Target: the selected target",
    "Goal: complete the requested task",
    "",
    "Use this pattern when the help text needs runtime-generated content but does not depend on host-supplied help arguments.",
  }, "\n")
end
