# install_host_deps.ps1 — Install host-level native runtime dependencies into third_party/
# Developer/build use only. End users do not need to invoke this manually.
# This script currently provisions the vldb-lancedb dynamic library package.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_host_deps.ps1

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# 中文：统一使用 RuntimeInformation 做平台探测，保证 Windows PowerShell 与 PowerShell 7+ 一致。
# English: Use RuntimeInformation for platform detection so Windows PowerShell and PowerShell 7+ behave consistently.
$script:IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$script:IsMacOSPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
$script:IsLinuxPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)

# ============================================================
# Configuration
# ============================================================
$ThirdParty = Join-Path $ProjectDir "third_party"
$DepsDir    = Join-Path $ThirdParty "deps"
$VldbLancedbDir = Join-Path $ThirdParty "vldb_lancedb"
$VldbLancedbIncludeDir = Join-Path $VldbLancedbDir "include"
$VldbLancedbDocsDir = Join-Path $VldbLancedbDir "docs"
$VldbLancedbRepo = "OpenVulcan/vldb-lancedb"

# ============================================================
# Helpers
# ============================================================
function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path $Path)) { New-Item -ItemType Directory -Path $Path -Force | Out-Null }
}

function Get-AvailableTarPath {
    <#
    .SYNOPSIS
    获取当前环境可用的 tar 路径 / Resolve an available tar executable for the current environment.
    #>
    $systemTar = "$env:SystemRoot\System32\tar.exe"
    if (Test-Path $systemTar) { return $systemTar }
    $tarCommand = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($tarCommand) { return $tarCommand.Source }
    throw "tar.exe is required to extract host dependency archives / 解压宿主依赖需要 tar.exe"
}

function Get-CurrentArchitectureKey {
    <#
    .SYNOPSIS
    获取当前 CPU 架构标识 / Get the current CPU architecture key.
    #>
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($Arch) {
        "x64" { return "x86_64" }
        "arm64" { return "aarch64" }
        default { throw "Unsupported architecture for dependency bootstrap: $Arch" }
    }
}

function Get-VldbLancedbAssetInfo {
    <#
    .SYNOPSIS
    解析当前平台对应的 vldb-lancedb 库模式资产信息 / Resolve the vldb-lancedb library asset info for the current platform.
    #>
    $ArchKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($ArchKey -ne "x86_64") {
            throw "vldb-lancedb prebuilt library currently supports Windows x86_64 only. Current arch: $ArchKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            library_name = "vldb_lancedb.dll"
        }
    }

    if ($script:IsLinuxPlatform) {
        $Target = if ($ArchKey -eq "aarch64") { "aarch64-unknown-linux-gnu" } else { "x86_64-unknown-linux-gnu" }
        return @{
            target = $Target
            archive_ext = ".tar.gz"
            library_name = "libvldb_lancedb.so"
        }
    }

    if ($script:IsMacOSPlatform) {
        $Target = if ($ArchKey -eq "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" }
        return @{
            target = $Target
            archive_ext = ".tar.gz"
            library_name = "libvldb_lancedb.dylib"
        }
    }

    throw "Unsupported platform for vldb-lancedb bootstrap."
}

function Install-VldbLancedbLibrary {
    <#
    .SYNOPSIS
    安装宿主级 vldb-lancedb 动态库 / Install the host-level vldb-lancedb dynamic library.
    #>
    Ensure-Dir $DepsDir
    Ensure-Dir $VldbLancedbDir
    Ensure-Dir $VldbLancedbIncludeDir
    Ensure-Dir $VldbLancedbDocsDir

    $AssetInfo = Get-VldbLancedbAssetInfo
    $TarPath = Get-AvailableTarPath
    $ApiUrl = "https://api.github.com/repos/$VldbLancedbRepo/releases/latest"

    Write-Host "==> Querying latest vldb-lancedb release..."
    $Release = Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing
    $TagName = $Release.tag_name
    if (-not $TagName) {
        throw "Latest vldb-lancedb release is missing tag_name / 最新 vldb-lancedb release 缺少 tag_name"
    }

    $AssetName = "vldb-lancedb-lib-$TagName-$($AssetInfo.target)$($AssetInfo.archive_ext)"
    $MarkerFile = Join-Path $VldbLancedbDir ".installed-$TagName-$($AssetInfo.target)"
    $LibraryDest = Join-Path $DepsDir $AssetInfo.library_name

    if ((Test-Path $MarkerFile) -and (Test-Path $LibraryDest)) {
        Write-Host "==> vldb-lancedb library already installed ($AssetName)."
        return
    }

    $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if (-not $Asset) {
        $Available = ($Release.assets | ForEach-Object { $_.name }) -join ", "
        throw "vldb-lancedb asset '$AssetName' not found in latest release. Available assets: $Available"
    }

    $TempDir = Join-Path $env:TEMP "vldb_lancedb_$pid"
    if (Test-Path $TempDir) {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $TempDir

    try {
        $ArchivePath = Join-Path $TempDir $AssetName
        Write-Host "==> Downloading vldb-lancedb library package: $AssetName"
        Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ArchivePath -UseBasicParsing

        if ($AssetInfo.archive_ext -eq ".zip") {
            Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force
        } else {
            & $TarPath -xzf $ArchivePath -C $TempDir
        }

        $LibrarySource = Get-ChildItem -Path $TempDir -Recurse -File -Filter $AssetInfo.library_name -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $LibrarySource) {
            throw "Dynamic library '$($AssetInfo.library_name)' not found after extracting $AssetName"
        }

        Copy-Item $LibrarySource.FullName $LibraryDest -Force

        $HeaderSource = Get-ChildItem -Path $TempDir -Recurse -File -Filter "vldb_lancedb.h" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($HeaderSource) {
            Copy-Item $HeaderSource.FullName (Join-Path $VldbLancedbIncludeDir "vldb_lancedb.h") -Force
        }

        $DocSource = Get-ChildItem -Path $TempDir -Recurse -File -Filter "LIBRARY_USAGE.zh-CN.md" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($DocSource) {
            Copy-Item $DocSource.FullName (Join-Path $VldbLancedbDocsDir "LIBRARY_USAGE.zh-CN.md") -Force
        }

        Get-ChildItem -Path $VldbLancedbDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
        Write-Host "==> vldb-lancedb library installed successfully."
    } finally {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-VldbLancedbLibrary
