return function(args)
  local params = {}
  local raw_params = args and args.params or {}
  local topic = tostring((raw_params and raw_params.topic) or "entrypoints")

  if topic == "entrypoints" then
    params.path = "."
    params.recursive = false
    params.ignore = true
    params.ext = "rs,ts,js"
    params.reason = "Start shallow so you can identify top-level entrypoints without flooding the client with nested implementation details."
    params.next_step = "Open the top-level files with the clearest ownership signals and then continue with targeted AST calls."
  elseif topic == "services" then
    params.path = "src"
    params.recursive = true
    params.ignore = true
    params.ext = "rs"
    params.reason = "Service-layer exploration benefits from recursive traversal, but keeping the extension filter focused prevents unnecessary noise."
    params.next_step = "Use the returned line spans to inspect only the service modules that own the requested behavior."
  else
    params.path = tostring(topic)
    params.recursive = false
    params.ignore = true
    params.ext = "rs,ts,js,lua"
    params.reason = "Unknown topics fall back to a conservative scan that still encourages focused exploration."
    params.next_step = "Refine the target path or extension filter after reviewing the first AST summary."
  end

  params.topic = topic
  return { params = params }
end
