--[[
Log detector middleware registry for Vulcan TestKit.
Vulcan TestKit 的日志检测中间件注册表。
]]

-- Detector files stay ordered from high-confidence language signals to generic fallback.
-- detector 文件按从高置信语言信号到通用兜底的顺序排列。
local DETECTOR_FILES = {
    "cargo.lua",
    "pytest.lua",
    "go.lua",
    "python.lua",
    "node.lua",
    "tsc.lua",
    "js-test.lua",
}

--- Load one detector file from the detector directory.
--- 从 detector 目录加载单个 detector 文件。
--- @param detector_dir string Detector directory.
--- @param filename string Detector filename.
--- @return table
local function load_detector(detector_dir, filename)
    local detector_path = vulcan.path.join(detector_dir, filename)
    local chunk, load_error = loadfile(detector_path)
    if not chunk then
        error("failed to load TestKit detector `" .. filename .. "`: " .. tostring(load_error))
    end

    local ok, detector = pcall(chunk)
    if not ok then
        error("failed to initialize TestKit detector `" .. filename .. "`: " .. tostring(detector))
    end
    if type(detector) ~= "table" or type(detector.detect) ~= "function" then
        error("TestKit detector `" .. filename .. "` must return a table with a detect function")
    end
    return detector
end

--- Load all log detectors.
--- 加载全部日志 detector。
--- @param detector_dir string Detector directory.
--- @return table
return function(detector_dir)
    local detectors = {}
    for _, filename in ipairs(DETECTOR_FILES) do
        table.insert(detectors, load_detector(detector_dir, filename))
    end
    return detectors
end
