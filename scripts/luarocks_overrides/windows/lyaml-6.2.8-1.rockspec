-- Windows override / Windows 覆盖版
-- 说明 / Notes:
-- lyaml 6.2.x 默认使用的 luke 构建器依赖 Unix shell 语义，在 Windows 下会在编译探测阶段失败。
-- 这里改为 LuaRocks builtin 构建器，直接编译 ext/yaml 下的 C 源文件并安装 Lua 模块。
-- The default luke-based build used by lyaml 6.2.x depends on Unix shell semantics and fails during compiler probing on Windows.
-- This override switches to LuaRocks' builtin builder, directly compiling the C sources under ext/yaml and installing the Lua modules.

package = "lyaml"
version = "6.2.8-1"

description = {
  summary = "libYAML binding for Lua",
  detailed = "Read and write YAML format files with Lua.",
  homepage = "https://github.com/gvvaughan/lyaml",
  license = "MIT/X11",
}

source = {
  url = "https://github.com/gvvaughan/lyaml/archive/refs/tags/v6.2.8.zip",
  dir = "lyaml-6.2.8",
}

dependencies = {
  "lua >= 5.1, < 5.5",
}

external_dependencies = {
  YAML = {
    library = "yaml",
  },
}

build = {
  type = "builtin",
  modules = {
    yaml = {
      sources = {
        "ext/yaml/yaml.c",
        "ext/yaml/emitter.c",
        "ext/yaml/parser.c",
        "ext/yaml/scanner.c",
      },
      defines = {
        "VERSION=\"6.2.8\"",
        "YAML_DECLARE_STATIC",
      },
      incdirs = {
        "ext/include",
        "$(LUA_INCDIR)",
        "$(YAML_INCDIR)",
      },
      libdirs = {
        "$(YAML_LIBDIR)",
      },
      libraries = {
        "yaml",
      },
    },
    ["lyaml"] = "lib/lyaml/init.lua",
    ["lyaml.explicit"] = "lib/lyaml/explicit.lua",
    ["lyaml.functional"] = "lib/lyaml/functional.lua",
    ["lyaml.implicit"] = "lib/lyaml/implicit.lua",
  },
}
