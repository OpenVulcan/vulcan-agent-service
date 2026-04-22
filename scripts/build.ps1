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
$SkillsOut = "$BaseOutDir\skills"
$PkgOut = "$BaseOutDir\lua_packages"
$ConfigOut = "$BaseOutDir\configs"
$ResourcesOut = "$BaseOutDir\resources"
$DependenciesOut = "$BaseOutDir\dependencies"
$DatabasesOut = "$BaseOutDir\databases"
$StateOut = "$BaseOutDir\state"
$TempOut = "$BaseOutDir\temp"
$LogsOut = "$BaseOutDir\logs"

# Ensure output directories exist
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
foreach ($dir in @($LibsOut, $SkillsOut, $PkgOut, $ConfigOut, $ResourcesOut, $DependenciesOut, $DatabasesOut, $StateOut, $TempOut, $LogsOut)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}
foreach ($dir in @(
    "$DependenciesOut\\shared\\tools",
    "$DependenciesOut\\shared\\lua",
    "$DependenciesOut\\shared\\ffi",
    "$DependenciesOut\\skill",
    "$DatabasesOut\\sqlite",
    "$DatabasesOut\\lancedb",
    "$StateOut\\skills"
)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

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

# Sync runtime shared resources to output/resources/
if (Test-Path "runtime\resources") {
    Copy-Item -Force -Recurse "runtime\resources\*" "$ResourcesOut\"
    Write-Host "==> Runtime shared resources synced to $ResourcesOut\"
} else {
    Write-Host "==> No runtime/resources directory found"
}

# Sync build-time runtime resource manifests to output/resources/
# 中文：将脚本侧维护的 Lua 扩展能力清单复制到输出目录，供运行时动态读取能力列表。
$LuaPackagesManifestSource = Join-Path -Path $ProjectDir -ChildPath "scripts\lua_packages.txt"
if (Test-Path -LiteralPath (Join-Path -Path $ProjectDir -ChildPath "scripts\lua_packages.txt")) {
    if (-not (Test-Path $ResourcesOut)) { New-Item -ItemType Directory -Path $ResourcesOut -Force | Out-Null }
    Copy-Item -Force -LiteralPath (Join-Path -Path $ProjectDir -ChildPath "scripts\lua_packages.txt") -Destination (Join-Path $ResourcesOut "lua_packages.txt")
    Write-Host "==> Runtime resources synced to $ResourcesOut\"
} else {
    Write-Host "==> No scripts/lua_packages.txt manifest found"
}

# Sync runtime Lua skills to output/skills/
if (Test-Path "runtime\skills") {
    Copy-Item -Force -Recurse "runtime\skills\*" "$SkillsOut\"
    Write-Host "==> Runtime Lua skills synced to $SkillsOut\"
} else {
    Write-Host "==> No runtime/skills directory found"
}

# Prepare output/bin/tools/ as the host runtime tool directory.
# 中文：runtime/ 不再承载宿主工具产物；这里仅确保 output/bin/tools 存在，供本地复制或安装流程写入。
$HostToolsOut = Join-Path $BaseOutDir "bin\tools"
if (-not (Test-Path $HostToolsOut)) { New-Item -ItemType Directory -Path $HostToolsOut -Force | Out-Null }
Write-Host "==> Host tool output directory prepared at $HostToolsOut\"

# Sync third-party Lua packages to output/lua_packages/
# Only copy runtime-relevant directories: lib/lua/, share/lua/
$ThirdPartyPackages = "third_party\lua_packages"
if (Test-Path $ThirdPartyPackages) {
    $pkgSrcDirs = @("lib\lua", "share\lua")
    foreach ($dir in $pkgSrcDirs) {
        $src = Join-Path $ThirdPartyPackages $dir
        $dst = Join-Path $PkgOut $dir
        if (Test-Path $src) {
            New-Item -ItemType Directory -Path $dst -Force | Out-Null
            $versioned = Join-Path $src "5.1"
            $copyRoot = if (Test-Path $versioned) { $versioned } else { $src }
            Get-ChildItem $copyRoot -Force | Where-Object { $_.Name -ne "5.1" } | ForEach-Object {
                $target = Join-Path $dst $_.Name
                Copy-Item -LiteralPath $_.FullName -Destination $target -Force -Recurse
            }
        }
    }
    Write-Host "==> Third-party Lua packages synced to $PkgOut\ (flattening 5.1 package layout into lua/)"
} else {
    Write-Host "==> No third_party/lua_packages found (run scripts/install_lua_deps.ps1 first)"
}

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
