@echo off
rem luarocks.bat — wrapper to use luarocks with project-local LuaJIT
rem Usage: scripts\luarocks.bat install <package>

setlocal
set "PROJECT_DIR=%~dp0.."

rem Set LuaJIT path
set "LUAJIT_DIR=%PROJECT_DIR%\third_party\luajit"

rem Set luarocks path
set "LUAROCKS_DIR=%PROJECT_DIR%\third_party\luarocks"

rem Set packages path
set "LUA_PACKAGES_DIR=%PROJECT_DIR%\third_party\lua_packages"

rem Add LuaJIT to PATH
set "PATH=%LUAJIT_DIR%;%PATH%"

rem Run luarocks
cd /d "%LUAROCKS_DIR%"
luarocks.exe --tree="%LUA_PACKAGES_DIR%" --lua-dir="%LUAJIT_DIR%" %*
