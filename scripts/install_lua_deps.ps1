# install_lua_deps.ps1 - Download the official LuaSkills runtime package payload into third_party.
# install_lua_deps.ps1 - 下载官方 LuaSkills runtime package 载荷到 third_party。
# Developer/build use only. This script only syncs luaskills-packages runtime assets and metadata.
# 仅供开发与构建使用；该脚本只同步 luaskills-packages 的 runtime 资产与元数据。
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so UTF-8 sources and modern script behavior stay stable.
# 若检测到 PowerShell 7，则自动在其下重启，确保 UTF-8 源文件与现代脚本行为保持稳定。
if ($PSVersionTable.PSEdition -ne "Core") {
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath
        exit $LASTEXITCODE
    }
}

$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# LuaRuntimeRepo stores the GitHub repository that publishes runtime package assets.
# LuaRuntimeRepo 保存发布 runtime package 资产的 GitHub 仓库。
$LuaRuntimeRepo = "LuaSkills/luaskills-packages"
if (-not [string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_REPO)) {
    $LuaRuntimeRepo = $env:LUA_RUNTIME_REPO.Trim()
}

# LuaRuntimeSeries stores the compatible major.minor series for runtime package assets.
# LuaRuntimeSeries 保存 runtime package 资产的兼容 major.minor 协议线。
$LuaRuntimeSeries = "0.1"
if (-not [string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_SERIES)) {
    $LuaRuntimeSeries = $env:LUA_RUNTIME_SERIES.Trim()
}

# LuaRuntimePackagesVersion stores one optional exact luaskills-packages GitHub Release tag override.
# LuaRuntimePackagesVersion 保存 luaskills-packages GitHub Release 标签的可选精确覆盖值。
$LuaRuntimePackagesVersion = ""
if (-not [string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_PACKAGES_VERSION)) {
    $LuaRuntimePackagesVersion = $env:LUA_RUNTIME_PACKAGES_VERSION.Trim()
}

# ThirdParty stores all downloaded dependency payloads outside the tracked source tree.
# ThirdParty 保存所有下载后的依赖载荷，避免写入受版本控制的源码目录。
$ThirdParty = Join-Path $ProjectDir "third_party"

# RuntimeInstallRoot stores the extracted official LuaSkills runtime package payloads.
# RuntimeInstallRoot 保存解压后的 LuaSkills 官方运行期载荷。
$RuntimeInstallRoot = Join-Path $ThirdParty "luaskills_runtime"

# DownloadCache stores verified runtime package archives.
# DownloadCache 保存已校验的运行时包压缩文件。
$DownloadCache = Join-Path $ThirdParty "downloads"

function Ensure-Dir {
    <#
    .SYNOPSIS
    Create one directory when it does not already exist.
    当目录不存在时创建目录。

    .PARAMETER Path
    Directory path that should exist.
    应确保存在的目录路径。
    #>
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Normalize-ReleaseTag {
    <#
    .SYNOPSIS
    Normalize one version token into a Git-style release tag.
    将一个版本标记规范化为 Git 风格的 release 标签。

    .PARAMETER Value
    Raw version or tag token.
    原始版本或标签文本。
    #>
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "Release tag value cannot be empty."
    }

    $Trimmed = $Value.Trim()
    if ($Trimmed.StartsWith("v")) {
        return $Trimmed
    }
    return "v$Trimmed"
}

function Convert-TagToSemVer {
    <#
    .SYNOPSIS
    Convert one Git tag such as v0.1.6 into a semantic-version object.
    将形如 v0.1.6 的 Git 标签转换为语义化版本对象。

    .PARAMETER Tag
    Git tag text to normalize and parse.
    需要规范化并解析的 Git 标签文本。
    #>
    param([string]$Tag)

    $Normalized = $Tag
    if ($Tag.StartsWith("v")) {
        $Normalized = $Tag.Substring(1)
    }
    if ($Normalized -notmatch '^\d+\.\d+\.\d+$') {
        throw "Unsupported semantic version tag: $Tag"
    }
    return [System.Version]$Normalized
}

function Resolve-ReleaseTagForSeries {
    <#
    .SYNOPSIS
    Resolve the newest published GitHub release tag inside one major.minor series.
    解析一个 major.minor 协议线内最新的已发布 GitHub release 标签。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Series
    Major.minor series such as 0.1.
    形如 0.1 的 major.minor 协议线。
    #>
    param(
        [string]$Repo,
        [string]$Series
    )

    if ($Series -notmatch '^\d+\.\d+$') {
        throw "Unsupported packages series: $Series"
    }

    $ApiUrl = "https://api.github.com/repos/$Repo/releases?per_page=100"
    $Releases = Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing
    $Matches = @()

    foreach ($Release in $Releases) {
        if ($Release.draft -or $Release.prerelease) {
            continue
        }

        $TagName = [string]$Release.tag_name
        try {
            $Version = Convert-TagToSemVer -Tag $TagName
        } catch {
            continue
        }

        $ReleaseSeries = "$($Version.Major).$($Version.Minor)"
        if ($ReleaseSeries -ne $Series) {
            continue
        }

        $Matches += [PSCustomObject]@{
            Tag = $TagName
            Version = $Version
        }
    }

    if (-not $Matches -or $Matches.Count -eq 0) {
        throw "No published release found for $Repo series $Series"
    }

    return ($Matches | Sort-Object Version -Descending | Select-Object -First 1).Tag
}

function Resolve-LuaRuntimePackagesTag {
    <#
    .SYNOPSIS
    Resolve the effective luaskills-packages release tag from an exact override or the configured series.
    基于精确覆盖或已配置协议线解析最终 luaskills-packages 发布标签。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Series
    Compatible packages major.minor series such as 0.1.
    兼容 packages 的 major.minor 协议线，例如 0.1。

    .PARAMETER PackagesVersion
    Optional exact luaskills-packages release tag override.
    可选的 luaskills-packages 精确发布标签覆盖值。

    #>
    param(
        [string]$Repo,
        [string]$Series,
        [string]$PackagesVersion
    )

    if (-not [string]::IsNullOrWhiteSpace($PackagesVersion)) {
        return Normalize-ReleaseTag -Value $PackagesVersion
    }

    return Resolve-ReleaseTagForSeries -Repo $Repo -Series $Series
}

function Get-CurrentPlatformKey {
    <#
    .SYNOPSIS
    Resolve the official LuaSkills runtime asset platform key.
    解析 LuaSkills 官方运行时资产的平台标识。

    .OUTPUTS
    Platform key such as windows-x64, linux-x64, linux-arm64, macos-x64, or macos-arm64.
    平台标识，例如 windows-x64、linux-x64、linux-arm64、macos-x64 或 macos-arm64。
    #>
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    $ArchKey = switch ($Arch) {
        "x64" { "x64" }
        "arm64" { "arm64" }
        default { throw "Unsupported architecture for LuaSkills runtime assets: $Arch" }
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        if ($ArchKey -ne "x64") {
            throw "LuaSkills official runtime assets currently support Windows x64 only. Current arch: $ArchKey"
        }
        return "windows-x64"
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        if ($ArchKey -eq "arm64") {
            return "macos-arm64"
        }
        return "macos-x64"
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        if ($ArchKey -eq "arm64") {
            return "linux-arm64"
        }
        return "linux-x64"
    }

    throw "Unsupported operating system for LuaSkills runtime assets."
}

function Get-AvailableTarPath {
    <#
    .SYNOPSIS
    Resolve an available tar executable for .tar.gz extraction.
    解析可用于解压 .tar.gz 的 tar 可执行文件。

    .OUTPUTS
    Full path or command path for tar.
    tar 的完整路径或命令路径。
    #>
    $SystemTar = "$env:SystemRoot\System32\tar.exe"
    if (Test-Path -LiteralPath $SystemTar) {
        return $SystemTar
    }

    $TarCommand = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($TarCommand) {
        return $TarCommand.Source
    }

    throw "tar.exe is required to extract the LuaSkills runtime package."
}

function Get-ReleaseAssetInfo {
    <#
    .SYNOPSIS
    Find one exact GitHub Release asset download URL and API digest.
    查找一个精确 GitHub Release 资产下载地址与 API 摘要。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Tag
    Release tag name.
    Release 标签名。

    .PARAMETER AssetName
    Exact release asset name.
    精确的 Release 资产名称。
    #>
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$AssetName
    )

    $ApiUrl = "https://api.github.com/repos/$Repo/releases/tags/$Tag"
    $Release = Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing
    $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if (-not $Asset) {
        $Available = ($Release.assets | ForEach-Object { $_.name }) -join ", "
        throw "Asset '$AssetName' not found in $Repo@$Tag. Available: $Available"
    }
    return [PSCustomObject]@{
        Url = $Asset.browser_download_url
        Digest = [string]$Asset.digest
    }
}

function Save-ReleaseAssetWithDigest {
    <#
    .SYNOPSIS
    Download one GitHub Release asset and verify its GitHub API digest.
    下载单个 GitHub Release 资产并校验其 GitHub API 摘要。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Tag
    Release tag name.
    Release 标签名。

    .PARAMETER AssetName
    Exact asset file name.
    精确资产文件名。

    #>
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$AssetName
    )

    Ensure-Dir $DownloadCache

    $ArchivePath = Join-Path $DownloadCache $AssetName
    $AssetInfo = Get-ReleaseAssetInfo -Repo $Repo -Tag $Tag -AssetName $AssetName
    if ($AssetInfo.Digest -notlike "sha256:*") {
        throw "GitHub API digest for $AssetName is missing or unsupported: $($AssetInfo.Digest)"
    }
    $Expected = $AssetInfo.Digest.Substring("sha256:".Length).ToLowerInvariant()

    if ((Test-Path -LiteralPath $ArchivePath) -and ((Get-FileSha256Hex -Path $ArchivePath) -eq $Expected)) {
        Write-Host "==> Reusing verified archive: $ArchivePath"
        return $ArchivePath
    }

    Write-Host "==> Downloading asset: $($AssetInfo.Url)"
    try {
        Invoke-WebRequest -Uri $AssetInfo.Url -OutFile $ArchivePath -UseBasicParsing
    } catch {
        throw "Failed to download runtime asset '$AssetName' from $Repo@$Tag. Confirm that the LuaSkills runtime assets have been published for this tag. Original error: $($_.Exception.Message)"
    }

    $Actual = Get-FileSha256Hex -Path $ArchivePath
    if ($Actual -ne $Expected) {
        throw "SHA-256 mismatch for $AssetName. Expected $Expected, got $Actual"
    }

    return $ArchivePath
}

function Get-FileSha256Hex {
    <#
    .SYNOPSIS
    Resolve one file SHA-256 digest with a portable fallback for older Windows PowerShell hosts.
    使用兼容旧版 Windows PowerShell 宿主的回退路径解析单个文件的 SHA-256 摘要。

    .PARAMETER Path
    File path whose SHA-256 digest should be returned as lowercase hexadecimal text.
    需要以小写十六进制文本返回 SHA-256 摘要的文件路径。

    .OUTPUTS
    Lowercase hexadecimal SHA-256 digest of the target file.
    目标文件的小写十六进制 SHA-256 摘要。
    #>
    param([string]$Path)

    $GetFileHashCommand = Get-Command -Name "Get-FileHash" -ErrorAction SilentlyContinue
    if ($GetFileHashCommand) {
        return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }

    $Sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $Stream = [System.IO.File]::OpenRead($Path)
        try {
            $DigestBytes = $Sha256.ComputeHash($Stream)
        } finally {
            $Stream.Dispose()
        }
    } finally {
        $Sha256.Dispose()
    }

    $Builder = New-Object -TypeName System.Text.StringBuilder
    foreach ($Byte in $DigestBytes) {
        [void]$Builder.AppendFormat("{0:x2}", $Byte)
    }
    return $Builder.ToString()
}

function Clear-RuntimeInstallRoot {
    <#
    .SYNOPSIS
    Clear the extracted runtime install directory inside third_party.
    清理 third_party 内的运行时安装目录。
    #>
    Ensure-Dir $ThirdParty
    $ThirdPartyRoot = (Resolve-Path -LiteralPath $ThirdParty).Path

    if (Test-Path -LiteralPath $RuntimeInstallRoot) {
        $ResolvedRuntimeRoot = (Resolve-Path -LiteralPath $RuntimeInstallRoot).Path
        if (-not $ResolvedRuntimeRoot.StartsWith($ThirdPartyRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to clear a runtime directory outside third_party: $ResolvedRuntimeRoot"
        }
        Remove-Item -LiteralPath $RuntimeInstallRoot -Recurse -Force
    }

    Ensure-Dir $RuntimeInstallRoot
}

function Copy-DirectoryContents {
    <#
    .SYNOPSIS
    Copy direct directory contents while preserving package layout.
    复制目录直属内容并保持包布局。

    .PARAMETER Source
    Source directory.
    来源目录。

    .PARAMETER Destination
    Destination directory.
    目标目录。
    #>
    param(
        [string]$Source,
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        throw "Required runtime directory is missing from package: $Source"
    }

    Ensure-Dir $Destination
    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
}

function Test-RuntimeInstallReady {
    <#
    .SYNOPSIS
    Check whether the extracted runtime root already satisfies the packaged runtime layout.
    检查已解压的运行根目录是否已经满足 packaged runtime 布局要求。

    .PARAMETER MarkerFile
    Marker file that represents the exact installed version combination.
    表示当前安装版本组合的标记文件。
    #>
    param([string]$MarkerFile)

    $RuntimeManifestPath = Join-Path $RuntimeInstallRoot "resources\lua-runtime-manifest.json"
    $PackagesManifestPath = Join-Path $RuntimeInstallRoot "resources\luaskills-packages-manifest.json"
    return (Test-Path -LiteralPath $MarkerFile) -and
        (Test-Path -LiteralPath $RuntimeManifestPath) -and
        (Test-Path -LiteralPath $PackagesManifestPath)
}

function Install-LuaRuntimePayloads {
    <#
    .SYNOPSIS
    Extract and install the official lua-runtime-packages archive into third_party.
    解压并安装官方 lua-runtime-packages 压缩包到 third_party。

    .PARAMETER RuntimeArchivePath
    Verified runtime package archive path.
    已校验的 runtime package 压缩包路径。

    .PARAMETER RuntimeTag
    Runtime package release tag.
    runtime package 发布标签。

    .PARAMETER Platform
    Current platform key.
    当前平台标识。
    #>
    param(
        [string]$RuntimeArchivePath,
        [string]$RuntimeTag,
        [string]$Platform
    )

    $MarkerFile = Join-Path $RuntimeInstallRoot ".installed-$RuntimeTag-$Platform"
    if (Test-RuntimeInstallReady -MarkerFile $MarkerFile) {
        Write-Host "==> LuaSkills runtime payloads already installed ($RuntimeTag, $Platform)."
        return
    }

    $TarPath = Get-AvailableTarPath
    $RuntimeTempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan_runtime_packages_{0}" -f $PID)

    if (Test-Path -LiteralPath $RuntimeTempDir) {
        Remove-Item -LiteralPath $RuntimeTempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $RuntimeTempDir

    try {
        & $TarPath -xzf $RuntimeArchivePath -C $RuntimeTempDir

        Clear-RuntimeInstallRoot

        foreach ($DirName in @("lua_packages", "libs", "resources", "licenses")) {
            $SourcePath = Join-Path $RuntimeTempDir $DirName
            if (Test-Path -LiteralPath $SourcePath) {
                Copy-DirectoryContents -Source $SourcePath -Destination (Join-Path $RuntimeInstallRoot $DirName)
            }
        }

        foreach ($DirName in @("bin", "skills", "dependencies", "state", "databases", "config", "temp", "system_lua_lib")) {
            Ensure-Dir (Join-Path $RuntimeInstallRoot $DirName)
        }
        Ensure-Dir (Join-Path $RuntimeInstallRoot "temp\downloads")

        $RuntimeManifestPath = Join-Path $RuntimeInstallRoot "resources\lua-runtime-manifest.json"
        $PackagesManifestPath = Join-Path $RuntimeInstallRoot "resources\luaskills-packages-manifest.json"
        if (-not (Test-Path -LiteralPath $RuntimeManifestPath)) {
            throw "Lua runtime manifest was not found after installing runtime packages."
        }
        if (-not (Test-Path -LiteralPath $PackagesManifestPath)) {
            throw "LuaSkills packages manifest was not found after installing runtime packages."
        }

        Get-ChildItem -Path $RuntimeInstallRoot -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
        Write-Host "==> LuaSkills runtime payloads installed to $RuntimeInstallRoot"
    } finally {
        Remove-Item -LiteralPath $RuntimeTempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$ResolvedLuaRuntimeTag = Resolve-LuaRuntimePackagesTag `
    -Repo $LuaRuntimeRepo `
    -Series $LuaRuntimeSeries `
    -PackagesVersion $LuaRuntimePackagesVersion
$Platform = Get-CurrentPlatformKey
$RuntimeAssetName = "lua-runtime-packages-$Platform.tar.gz"

Write-Host ""
Write-Host "=== LuaSkills Runtime Packages ==="
Write-Host "==> Runtime repo:    $LuaRuntimeRepo"
Write-Host "==> Runtime version: $ResolvedLuaRuntimeTag"
Write-Host "==> Platform:        $Platform"
Write-Host "==> This flow downloads only lua-runtime-packages assets from luaskills-packages."

Write-Host ""
Write-Host "=== Step 1: Runtime Package ==="
$RuntimeArchivePath = Save-ReleaseAssetWithDigest -Repo $LuaRuntimeRepo -Tag $ResolvedLuaRuntimeTag -AssetName $RuntimeAssetName

Write-Host ""
Write-Host "=== Step 2: Install Runtime Payloads ==="
Install-LuaRuntimePayloads -RuntimeArchivePath $RuntimeArchivePath -RuntimeTag $ResolvedLuaRuntimeTag -Platform $Platform

Write-Host ""
Write-Host "==> Lua runtime dependencies ready."
