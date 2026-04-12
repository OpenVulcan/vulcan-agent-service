@echo off
setlocal enabledelayedexpansion

:: vulcan-mcp build wrapper
:: Usage:
::   make                 - debug build
::   make release         - release build
::   make clean           - remove output directory

set "SCRIPT_DIR=%~dp0"
set "MODE=%~1"

if /i "%MODE%"=="release" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\build.ps1" -Release
) else if /i "%MODE%"=="clean" (
    echo ==^> Cleaning output directory...
    if exist "%SCRIPT_DIR%output" rmdir /s /q "%SCRIPT_DIR%output"
    echo ==^> Done.
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\build.ps1"
)
