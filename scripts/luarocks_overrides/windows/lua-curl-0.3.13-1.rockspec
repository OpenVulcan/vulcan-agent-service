-- Windows override / Windows 覆盖版
-- Windows 下项目通过预编译依赖提供 libcurl 头文件与导入库，当前导入库名称为 libcurl_imp。
-- This Windows override points LuaRocks at the project-provided curl headers/import library,
-- and maps the Windows import library name to `libcurl_imp`.

package = "Lua-cURL"
version = "0.3.13-1"

source = {
  url = "https://github.com/Lua-cURL/Lua-cURLv3/archive/v0.3.13.zip",
  dir = "Lua-cURLv3-0.3.13",
}

description = {
  summary = "Lua binding to libcurl",
  detailed = [[
  ]],
  homepage = "https://github.com/Lua-cURL",
  license  = "MIT/X11"
}

dependencies = {
  "lua >= 5.1, < 5.5"
}

external_dependencies = {
  CURL = {
    header  = "curl/curl.h",
    library = "libcurl_imp",
  }
}

build = {
  copy_directories = {'doc', 'examples', 'test'},

  type = "builtin",

  modules = {
    ["cURL"           ] = "src/lua/cURL.lua",
    ["cURL.safe"      ] = "src/lua/cURL/safe.lua",
    ["cURL.utils"     ] = "src/lua/cURL/utils.lua",
    ["cURL.impl.cURL" ] = "src/lua/cURL/impl/cURL.lua",

    lcurl = {
      libraries = {"libcurl_imp", "ws2_32"},
      sources = {
        "src/l52util.c",    "src/lceasy.c", "src/lcerror.c",
        "src/lchttppost.c", "src/lcurl.c",  "src/lcutils.c",
        "src/lcmulti.c",    "src/lcshare.c", "src/lcmime.c",
        "src/lcurlapi.c",
      },
      incdirs   = { "$(CURL_INCDIR)" },
      libdirs   = { "$(CURL_LIBDIR)" }
    },
  }
}
