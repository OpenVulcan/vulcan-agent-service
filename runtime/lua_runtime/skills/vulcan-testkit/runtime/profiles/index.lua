--[[
Validation profile middleware registry for Vulcan TestKit.
Vulcan TestKit 的验证 profile 中间件注册表。
]]

-- Profile files are intentionally explicit so contributors add one bounded tool family at a time.
-- profile 文件保持显式列出，方便贡献者按有界工具族逐个扩展。
local PROFILE_FILES = {
    "cargo.lua",
    "go.lua",
    "pytest.lua",
    "ruff.lua",
    "mypy.lua",
    "vitest.lua",
    "jest.lua",
    "python.lua",
    "node.lua",
    "tsc.lua",
    "npm.lua",
}

--- Load one profile file from the profile directory.
--- 从 profile 目录加载单个 profile 文件。
--- @param profile_dir string Profile directory.
--- @param filename string Profile filename.
--- @return table
local function load_profile(profile_dir, filename)
    local profile_path = vulcan.path.join(profile_dir, filename)
    local chunk, load_error = loadfile(profile_path)
    if not chunk then
        error("failed to load TestKit profile `" .. filename .. "`: " .. tostring(load_error))
    end

    local ok, profile = pcall(chunk)
    if not ok then
        error("failed to initialize TestKit profile `" .. filename .. "`: " .. tostring(profile))
    end
    if type(profile) ~= "table" or type(profile.key) ~= "string" then
        error("TestKit profile `" .. filename .. "` must return a table with a string key")
    end
    return profile
end

--- Load all validation profiles.
--- 加载全部验证 profile。
--- @param profile_dir string Profile directory.
--- @return table
return function(profile_dir)
    local profiles = {}
    for _, filename in ipairs(PROFILE_FILES) do
        table.insert(profiles, load_profile(profile_dir, filename))
    end
    return profiles
end
