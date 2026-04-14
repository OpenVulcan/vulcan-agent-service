-- Windows override / Windows 覆盖版
-- 说明 / Notes:
-- 使用项目预编译依赖中的静态 PCRE2 库名，避免 LuaRocks 只查找动态库/非 static 后缀导致安装失败。
-- Use the static PCRE2 library name shipped in the project's prebuilt dependencies so LuaRocks does not fail while looking only for non-static Windows library names.

source = {
  url = "git+https://github.com/rrthomas/lrexlib.git",
  tag = "rel-2-9-2",
}

description = {
  summary = "Regular expression library binding (PCRE2 flavour).",
  detailed = "Lrexlib is a regular expression library for Lua 5.1-5.4, which\
provides bindings for several regular expression libraries.\
This rock provides the PCRE2 bindings.",
  license = "MIT/X11",
  homepage = "https://github.com/rrthomas/lrexlib",
}

dependencies = {
  "lua >= 5.1",
}

package = "Lrexlib-PCRE2"
version = "2.9.2-1"

build = {
  type = "builtin",
  modules = {
    rex_pcre2 = {
      sources = {
        "src/common.c",
        "src/pcre2/lpcre2.c",
        "src/pcre2/lpcre2_f.c",
      },
      defines = {
        "VERSION=\"2.9.2\"",
        "PCRE2_CODE_UNIT_WIDTH=8",
        "PCRE2_STATIC",
      },
      incdirs = {
        "$(PCRE2_INCDIR)",
      },
      libdirs = {
        "$(PCRE2_LIBDIR)",
      },
      libraries = {
        "pcre2-8-static",
      },
    },
  },
}

external_dependencies = {
  PCRE2 = {
    header = "pcre2.h",
    library = "pcre2-8-static",
  },
}
