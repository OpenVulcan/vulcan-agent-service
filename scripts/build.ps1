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

# Ensure output directory exists
if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir\"

# Sync config files
if (Test-Path "configs") {
    if (-not (Test-Path "output\configs")) {
        New-Item -ItemType Directory -Path "output\configs" -Force | Out-Null
    }
    Copy-Item -Force -Recurse "configs\*" "output\configs\"
    Write-Host "==> Config files synced to output\configs\"
} else {
    Write-Host "==> No config directory found"
}

# Sync Lua skills
if (Test-Path "lua_skills") {
    if (-not (Test-Path "$OutDir\lua_skills")) {
        New-Item -ItemType Directory -Path "$OutDir\lua_skills" -Force | Out-Null
    }
    Copy-Item -Force -Recurse "lua_skills\*" "$OutDir\lua_skills\"
    Write-Host "==> Lua skills synced to $OutDir\lua_skills\"
} else {
    Write-Host "==> No lua_skills directory found"
}

# Sync third-party Lua packages (luarocks-installed C modules)
$ThirdPartyPackages = "third_party\lua_packages"
if (Test-Path $ThirdPartyPackages) {
    $PkgOut = "$OutDir\lua_packages"
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
    # Copy runtime DLLs to output root so C modules can find them at runtime
    Get-ChildItem -Recurse -Path $ThirdPartyDeps -Include "*.dll","*.so","*.dylib" -ErrorAction SilentlyContinue | ForEach-Object {
        Copy-Item -Force $_.FullName "$OutDir\$($_.Name)"
    }
    Write-Host "==> C runtime DLLs synced to $OutDir\"
}

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
