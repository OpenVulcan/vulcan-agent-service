-- Windows override / Windows 覆盖版
-- 说明 / Notes:
-- Windows 下项目通过 vcpkg 提供 libcurl 头文件与库目录，且静态/导入库名称使用 libcurl。
-- This Windows override wires LuaRocks to the vcpkg-provided libcurl include and library directories,
-- and uses the Windows library name `libcurl` instead of the Unix-style `curl`.

package = "LuaCURL"
version = "1.2.1-1"

source = {
   url = "http://luaforge.net/frs/download.php/3342/luacurl-1.2.1.zip",
   md5 = "4c83710a0fc5ca52818e5ec0101c4395"
}

description = {
   summary = "Lua module binding CURL",
   detailed = [[
      LuaCURL is Lua 5.x compatible module providing Internet browsing
      capabilities based on the CURL library. The module interface
      follows strictly the CURL architecture and is very easy to use
      if the programmer has already experience with CURL.
   ]],
   homepage = "http://luaforge.net/projects/luacurl/",
   license = "MIT/X11"
}

dependencies = {
   "lua >= 5.1"
}

external_dependencies = {
   CURL = {
      header = "curl/curl.h",
      library = "libcurl"
   }
}

build = {
   type = "builtin",
   modules = {
      luacurl = {
         sources = { "luacurl.c" },
         libraries = { "libcurl" },
         incdirs = { "$(CURL_INCDIR)" },
         libdirs = { "$(CURL_LIBDIR)" }
      }
   }
}
