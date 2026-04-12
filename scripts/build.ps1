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

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
