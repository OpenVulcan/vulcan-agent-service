# init.ps1 — Download ast-grep binary for current platform
# 中文：初始化 ast-grep 运行时依赖，优先使用宿主注入目录，缺失时回退到脚本目录。
# English: Initialize the ast-grep runtime dependency. Prefer host-provided directories and fall back to the script directory when needed.

$ErrorActionPreference = "Stop"

# 中文：Lua 宿主通常会注入 SKILL_DIR，这里提供回退逻辑以便脚本单独调试。
# English: The Lua host usually injects SKILL_DIR; this fallback keeps standalone debugging working.
if (-not $env:SKILL_DIR) {
    $env:SKILL_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
}

# 中文：TOOLS_DIR 在本脚本里不是硬依赖，但保留一个稳定默认值更利于复用。
# English: TOOLS_DIR is not strictly required here, but a stable default improves reuse.
if (-not $env:TOOLS_DIR) {
    $env:TOOLS_DIR = Join-Path (Split-Path -Parent $env:SKILL_DIR) "tools"
}

$BinDir = Join-Path $env:SKILL_DIR "bin"
if (-not (Test-Path $BinDir)) { New-Item -ItemType Directory -Path $BinDir -Force | Out-Null }

# Skip if binary already exists
if (Test-Path "$BinDir\ast-grep.exe") {
    Write-Host "ast-grep already installed: $BinDir\ast-grep.exe"
    exit 0
}

# Detect architecture
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") { $arch = "x86_64" }
elseif ($arch -eq "ARM64") { $arch = "aarch64" }
elseif ($arch -eq "X86") { $arch = "i686" }

# ast-grep release asset naming: app-{arch}-{os}-{env}.zip
$assetMap = @{
    "x86_64"  = "app-x86_64-pc-windows-msvc.zip"
    "aarch64" = "app-aarch64-pc-windows-msvc.zip"
    "i686"    = "app-i686-pc-windows-msvc.zip"
}

$assetName = $assetMap[$arch]
if (-not $assetName) {
    Write-Error "Unsupported architecture: $arch"
    exit 1
}

Write-Host "Fetching latest ast-grep release..."
$release = Invoke-RestMethod "https://api.github.com/repos/ast-grep/ast-grep/releases/latest" -ErrorAction Stop

$asset = $release.assets | Where-Object { $_.name -eq $assetName }
if (-not $asset) {
    Write-Error "Asset not found: $assetName. Available assets:"
    $release.assets | ForEach-Object { Write-Host "  $($_.name)" }
    exit 1
}

$tag = $release.tag_name
Write-Host "Downloading ast-grep $tag ($assetName)..."

$zipPath = Join-Path $BinDir "ast-grep.zip"
Invoke-WebRequest $asset.browser_download_url -OutFile $zipPath -ErrorAction Stop

Write-Host "Extracting..."
# Expand-Archive is built into Windows 10+ PowerShell
Expand-Archive -Force -Path $zipPath -DestinationPath $BinDir

# The zip typically extracts to a subdirectory, find ast-grep.exe
$exe = Get-ChildItem -Recurse -Path $BinDir -Filter "ast-grep.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($exe) {
    # Move to bin root if nested
    if ($exe.Directory.FullName -ne $BinDir) {
        Move-Item -Force $exe.FullName "$BinDir\ast-grep.exe"
    }
}

# Cleanup
if (Test-Path $zipPath) { Remove-Item $zipPath }
# Remove extracted directories (anything that's not the exe)
Get-ChildItem -Path $BinDir -Exclude "ast-grep.exe" | ForEach-Object {
    Remove-Item -Recurse -Force $_.FullName
}

if (Test-Path "$BinDir\ast-grep.exe") {
    $ver = & "$BinDir\ast-grep.exe" --version 2>&1
    Write-Host "ast-grep installed: $ver"
} else {
    Write-Error "ast-grep.exe not found after extraction"
    exit 1
}
