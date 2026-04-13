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

# Determine base output directory
$BaseOutDir = "output"

# Ensure base output directory exists
if (-not (Test-Path $BaseOutDir)) {
    New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null
}

# Ensure bin output directory exists
if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir\"

# Sync runtime config files to output/configs
if (Test-Path "runtime\configs") {
    if (-not (Test-Path "$BaseOutDir\configs")) {
        New-Item -ItemType Directory -Path "$BaseOutDir\configs" -Force | Out-Null
    }
    Copy-Item -Force -Recurse "runtime\configs\*" "$BaseOutDir\configs\"
    Write-Host "==> Runtime configs synced to $BaseOutDir\configs\"
} else {
    Write-Host "==> No runtime/configs directory found"
}

# Sync runtime Lua skills to output/lua_skills
if (Test-Path "runtime\lua_skills") {
    if (-not (Test-Path "$BaseOutDir\lua_skills")) {
        New-Item -ItemType Directory -Path "$BaseOutDir\lua_skills" -Force | Out-Null
    }
    Copy-Item -Force -Recurse "runtime\lua_skills\*" "$BaseOutDir\lua_skills\"
    Write-Host "==> Runtime Lua skills synced to $BaseOutDir\lua_skills\"
} else {
    Write-Host "==> No runtime/lua_skills directory found"
}

# Sync third-party Lua packages to output/lua_packages
$ThirdPartyPackages = "third_party\lua_packages"
if (Test-Path $ThirdPartyPackages) {
    $PkgOut = "$BaseOutDir\lua_packages"
    if (-not (Test-Path $PkgOut)) {
        New-Item -ItemType Directory -Path $PkgOut -Force | Out-Null
    }
    Copy-Item -Force -Recurse "$ThirdPartyPackages\*" "$PkgOut\"
    Write-Host "==> Third-party Lua packages synced to $PkgOut\"
} else {
    Write-Host "==> No third_party/lua_packages found (run scripts/install_lua_deps.ps1 first)"
}

# Sync C dependency DLLs (runtime deps for C modules, e.g. zlib1.dll)
$ThirdPartyDeps = "third_party\deps"
if (Test-Path $ThirdPartyDeps) {
    Get-ChildItem -Recurse -Path $ThirdPartyDeps -Include "*.dll","*.so","*.dylib" -ErrorAction SilentlyContinue | ForEach-Object {
        Copy-Item -Force $_.FullName "$OutDir\$($_.Name)"
    }
    Write-Host "==> C runtime DLLs synced to $OutDir\"
}

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
