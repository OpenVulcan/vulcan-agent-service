return function(args)
  local raw_params = (args and args.params) or {}
  local topic = tostring((raw_params and raw_params.topic) or "entrypoints")

  local path = "."
  local recursive = false
  local ignore = true
  local ext = "rs,ts,js"
  local reason = "Start shallow so you can identify top-level entrypoints without flooding the client with nested implementation details."
  local next_step = "Open the top-level files with the clearest ownership signals and then continue with targeted AST calls."

  if topic == "services" then
    path = "src"
    recursive = true
    ext = "rs"
    reason = "Service-layer exploration benefits from recursive traversal, but keeping the extension filter focused prevents unnecessary noise."
    next_step = "Use the returned line spans to inspect only the service modules that own the requested behavior."
  elseif topic ~= "entrypoints" then
    path = tostring(topic)
    ext = "rs,ts,js,lua"
    reason = "Unknown topics fall back to a conservative scan that still encourages focused exploration."
    next_step = "Refine the target path or extension filter after reviewing the first AST summary."
  end

  local text = table.concat({
    "# codeview_ast Example: " .. topic,
    "",
    "Recommended call:",
    string.format("`codeview_ast(path=\"%s\", recursive=%s, ignore=%s, ext=\"%s\")`", path, tostring(recursive), tostring(ignore), ext),
    "",
    "Why this shape:",
    reason,
    "",
    "Next step after the AST pass:",
    next_step,
  }, "\n")

  return {
    contents = {
      {
        uri = tostring((args and args.uri) or "skill://codeview_ast/example/" .. topic),
        mimeType = "text/markdown",
        text = text,
      }
    }
  }
end
