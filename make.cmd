@echo off
setlocal enabledelayedexpansion

:: vulcan-mcp build wrapper
:: Usage:
::   make                 - debug build
::   make release         - release build
::   make run             - run debug build
::   make run release     - run release build
::   make clean           - remove output directory

set "SCRIPT_DIR=%~dp0"
set "MODE=%~1"
set "SUBMODE=%~2"

if /i "%MODE%"=="release" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\build.ps1" -Release
    pause
) else if /i "%MODE%"=="run" (
    if /i "%SUBMODE%"=="release" (
        set "BinPath=%SCRIPT_DIR%output\bin\vulcan-mcp.exe"
    ) else (
        set "BinPath=%SCRIPT_DIR%output\debug\vulcan-mcp.exe"
    )
    if not exist "!BinPath!" (
        echo ==^> Binary not found: !BinPath!
        echo ==^> Run 'make' or 'make release' first.
        pause
        exit /b 1
    )
    echo ==^> Running vulcan-mcp...
    echo.
    pushd "%SCRIPT_DIR%"
    "!BinPath!"
    popd
    echo ==^> Process exited.
) else if /i "%MODE%"=="clean" (
    echo ==^> Cleaning output directory...
    if exist "%SCRIPT_DIR%output" rmdir /s /q "%SCRIPT_DIR%output"
    echo ==^> Done.
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\build.ps1"
    pause
)
