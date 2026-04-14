# Build script for vulcan-mcp (PowerShell)
# Usage:
#   .\build.ps1           # debug build
#   .\build.ps1 release   # release build
#   .\build.ps1 -Release  # release build

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

$BinName = "vulcan-mcp"

if ($Release -or ($args.Count -gt 0 -and $args[0] -eq "release")) {
    $OutDir = "output\bin"
    $CargoProfile = "release"
    $CargoArgs = @("--release")
    Write-Host "==> Building release..."
} else {
    $OutDir = "output\debug"
    $CargoProfile = "debug"
    $CargoArgs = @()
    Write-Host "==> Building debug..."
}

# Build
& cargo build @CargoArgs

# Determine binary path
$BinExe = "target\$CargoProfile\$BinName.exe"
if (-not (Test-Path $BinExe)) {
    $BinExe = "target\$CargoProfile\$BinName"
}
if (-not (Test-Path $BinExe)) {
    Write-Error "Build failed: binary not found at $BinExe"
    exit 1
}

# Output directories
$BaseOutDir = "output"
$LibsOut = "$BaseOutDir\libs"
$SkillsOut = "$BaseOutDir\lua_skills"
$PkgOut = "$BaseOutDir\lua_packages"
$ConfigOut = "$BaseOutDir\configs"

# Ensure output directories exist
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir\"

# Sync C dependency DLLs to output/libs/
if (-not (Test-Path $LibsOut)) { New-Item -ItemType Directory -Path $LibsOut -Force | Out-Null }
if (Test-Path "third_party\deps") {
    Get-ChildItem -Recurse -Path "third_party\deps" -Include "*.dll","*.so","*.dylib" -ErrorAction SilentlyContinue | ForEach-Object {
        Copy-Item -Force $_.FullName "$LibsOut\$($_.Name)"
    }
    Write-Host "==> C runtime DLLs synced to $LibsOut\"
} else {
    Write-Host "==> No third_party/deps found"
}

# Copy LuaJIT runtime DLL (lua51.dll) — required by luarocks-built C modules like lfs.dll
if (Test-Path "third_party\luajit\lua51.dll") {
    Copy-Item -Force "third_party\luajit\lua51.dll" "$LibsOut\"
    Write-Host "==> LuaJIT lua51.dll synced to $LibsOut\"
}

# Sync runtime config files to output/configs/
if (Test-Path "runtime\configs") {
    if (-not (Test-Path $ConfigOut)) { New-Item -ItemType Directory -Path $ConfigOut -Force | Out-Null }
    Copy-Item -Force -Recurse "runtime\configs\*" "$ConfigOut\"
    Write-Host "==> Runtime configs synced to $ConfigOut\"
} else {
    Write-Host "==> No runtime/configs directory found"
}

# Sync runtime Lua skills to output/lua_skills/
if (Test-Path "runtime\lua_skills") {
    if (-not (Test-Path $SkillsOut)) { New-Item -ItemType Directory -Path $SkillsOut -Force | Out-Null }
    Copy-Item -Force -Recurse "runtime\lua_skills\*" "$SkillsOut\"
    Write-Host "==> Runtime Lua skills synced to $SkillsOut\"
} else {
    Write-Host "==> No runtime/lua_skills directory found"
}

# Sync third-party Lua packages to output/lua_packages/
# Only copy runtime-relevant directories: lib/lua/, share/lua/
$ThirdPartyPackages = "third_party\lua_packages"
if (Test-Path $ThirdPartyPackages) {
    $pkgSrcDirs = @("lib\lua", "share\lua")
    foreach ($dir in $pkgSrcDirs) {
        $src = Join-Path $ThirdPartyPackages $dir
        $dst = Join-Path $PkgOut $dir
        if (Test-Path $src) {
            New-Item -ItemType Directory -Path (Split-Path $dst -Parent) -Force | Out-Null
            Copy-Item -Force -Recurse "$src\*" $dst
        }
    }
    Write-Host "==> Third-party Lua packages synced to $PkgOut\"
} else {
    Write-Host "==> No third_party/lua_packages found (run scripts/install_lua_deps.ps1 first)"
}

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
