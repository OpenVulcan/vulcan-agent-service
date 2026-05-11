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

# LuaRuntimeVersion preserves the legacy environment contract where callers often pass the luaskills crate version.
# LuaRuntimeVersion 保留旧环境变量契约；历史调用方通常会在这里传入 luaskills crate 版本号。
$LuaRuntimeVersion = ""
if (-not [string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_VERSION)) {
    $LuaRuntimeVersion = $env:LUA_RUNTIME_VERSION.Trim()
}

# ThirdParty stores all downloaded dependency payloads outside the tracked source tree.
# ThirdParty 保存所有下载后的依赖载荷，避免写入受版本控制的源码目录。
$ThirdParty = Join-Path $ProjectDir "third_party"

# RuntimeInstallRoot stores the extracted official LuaSkills runtime package payloads.
# RuntimeInstallRoot 保存解压后的 LuaSkills 官方运行期载荷。
$RuntimeInstallRoot = Join-Path $ThirdParty "luaskills_runtime"

# DownloadCache stores verified archives and sidecar checksums.
# DownloadCache 保存已校验的压缩包与旁路校验文件。
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
    Resolve the effective luaskills-packages release tag from explicit overrides, legacy inputs, and the compatible series.
    基于精确覆盖、旧输入语义与兼容协议线解析最终 luaskills-packages 发布标签。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Series
    Compatible packages major.minor series such as 0.1.
    兼容 packages 的 major.minor 协议线，例如 0.1。

    .PARAMETER PackagesVersion
    Optional exact luaskills-packages release tag override.
    可选的 luaskills-packages 精确发布标签覆盖值。

    .PARAMETER LegacyRuntimeVersion
    Legacy runtime version input that may still carry the luaskills crate version.
    旧运行时版本输入，历史上可能仍承载 luaskills crate 版本号。
    #>
    param(
        [string]$Repo,
        [string]$Series,
        [string]$PackagesVersion,
        [string]$LegacyRuntimeVersion
    )

    if (-not [string]::IsNullOrWhiteSpace($PackagesVersion)) {
        return Normalize-ReleaseTag -Value $PackagesVersion
    }

    if ([string]::IsNullOrWhiteSpace($LegacyRuntimeVersion)) {
        return Resolve-ReleaseTagForSeries -Repo $Repo -Series $Series
    }

    $LegacyTag = Normalize-ReleaseTag -Value $LegacyRuntimeVersion
    try {
        $LegacySemVer = Convert-TagToSemVer -Tag $LegacyTag
        $LegacySeries = "$($LegacySemVer.Major).$($LegacySemVer.Minor)"
        if ($LegacySeries -eq $Series) {
            return $LegacyTag
        }
    } catch {
        throw "Unsupported LUA_RUNTIME_VERSION value '$LegacyRuntimeVersion'. Use a semantic version such as 0.4.1, or set LUA_RUNTIME_PACKAGES_VERSION for an exact luaskills-packages tag."
    }

    Write-Host "==> LUA_RUNTIME_VERSION=$LegacyRuntimeVersion detected as legacy luaskills crate version; resolving compatible luaskills-packages tag from series $Series."
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

function Get-ReleaseAssetUrl {
    <#
    .SYNOPSIS
    Build the direct GitHub Release asset URL.
    构造 GitHub Release 资产的直接下载地址。

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

    return "https://github.com/$Repo/releases/download/$Tag/$AssetName"
}

function Get-ExpectedSha256 {
    <#
    .SYNOPSIS
    Read the first checksum token from one .sha256 sidecar file.
    从一个 .sha256 旁路文件读取首个校验值片段。

    .PARAMETER ShaPath
    Local .sha256 file path.
    本地 .sha256 文件路径。
    #>
    param([string]$ShaPath)

    return (((Get-Content -LiteralPath $ShaPath -Raw).Trim() -split "\s+")[0]).ToLowerInvariant()
}

function Test-ArchiveSha256 {
    <#
    .SYNOPSIS
    Check whether one archive matches its expected SHA-256 digest.
    检查压缩包是否匹配期望的 SHA-256 摘要。

    .PARAMETER ArchivePath
    Archive path to verify.
    需要校验的压缩包路径。

    .PARAMETER ExpectedSha256
    Expected lower-case SHA-256 digest.
    期望的小写 SHA-256 摘要。
    #>
    param(
        [string]$ArchivePath,
        [string]$ExpectedSha256
    )

    if (-not (Test-Path -LiteralPath $ArchivePath)) {
        return $false
    }

    $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
    return $Actual -eq $ExpectedSha256
}

function Save-ReleaseAssetWithSha256 {
    <#
    .SYNOPSIS
    Download one GitHub Release asset and verify its .sha256 sidecar.
    下载单个 GitHub Release 资产并校验其 .sha256 旁路文件。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Tag
    Release tag name.
    Release 标签名。

    .PARAMETER AssetName
    Exact asset file name.
    精确资产文件名。

    .PARAMETER ShaAssetName
    Optional checksum asset file name when the sidecar does not follow the default .sha256 suffix pattern.
    当校验文件不遵循默认 .sha256 后缀规则时，可选指定其精确文件名。
    #>
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$AssetName,
        [string]$ShaAssetName = ""
    )

    Ensure-Dir $DownloadCache

    if ([string]::IsNullOrWhiteSpace($ShaAssetName)) {
        $ShaAssetName = "$AssetName.sha256"
    }

    $ArchivePath = Join-Path $DownloadCache $AssetName
    $ShaPath = "$ArchivePath.sha256"
    $ArchiveUrl = Get-ReleaseAssetUrl -Repo $Repo -Tag $Tag -AssetName $AssetName
    $ShaUrl = Get-ReleaseAssetUrl -Repo $Repo -Tag $Tag -AssetName $ShaAssetName

    Write-Host "==> Downloading checksum: $ShaUrl"
    Invoke-WebRequest -Uri $ShaUrl -OutFile $ShaPath -UseBasicParsing
    $Expected = Get-ExpectedSha256 -ShaPath $ShaPath

    if (Test-ArchiveSha256 -ArchivePath $ArchivePath -ExpectedSha256 $Expected) {
        Write-Host "==> Reusing verified archive: $ArchivePath"
        return $ArchivePath
    }

    Write-Host "==> Downloading asset: $ArchiveUrl"
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ArchivePath -UseBasicParsing

    if (-not (Test-ArchiveSha256 -ArchivePath $ArchivePath -ExpectedSha256 $Expected)) {
        $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
        throw "SHA-256 mismatch for $AssetName. Expected $Expected, got $Actual"
    }

    return $ArchivePath
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

function Resolve-BundleExtractRoot {
    <#
    .SYNOPSIS
    Resolve the extracted bundle directory that contains lua_packages metadata files.
    解析包含 lua_packages 元数据文件的 bundle 解压目录。

    .PARAMETER ExtractRoot
    Temporary extraction root for the release bundle zip.
    release bundle zip 的临时解压根目录。
    #>
    param([string]$ExtractRoot)

    $CompatFile = Get-ChildItem -LiteralPath $ExtractRoot -Recurse -File -Filter "lua_packages.txt" |
        Select-Object -First 1
    if (-not $CompatFile) {
        throw "LuaSkills packages bundle does not contain lua_packages.txt."
    }
    return Split-Path -Parent $CompatFile.FullName
}

function Normalize-BundleLicenseIndexPaths {
    <#
    .SYNOPSIS
    Rewrite bundle license-index paths so they match the runtime layout.
    重写 bundle 授权索引路径，使其匹配运行时布局。

    .PARAMETER IndexPath
    Installed luaskills-packages license index path.
    已安装的 luaskills-packages 授权索引路径。
    #>
    param([string]$IndexPath)

    if (-not (Test-Path -LiteralPath $IndexPath)) {
        throw "LuaSkills packages license index is missing: $IndexPath"
    }

    $Content = Get-Content -LiteralPath $IndexPath -Raw
    $Content = $Content.Replace('"dist/licenses/', '"licenses/luaskills-packages/')
    Set-Content -LiteralPath $IndexPath -Value $Content -Encoding UTF8
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
    $ThirdPartyLicensesPath = Join-Path $RuntimeInstallRoot "resources\luaskills-packages\THIRD_PARTY_LICENSES.json"
    $ThirdPartyNoticesPath = Join-Path $RuntimeInstallRoot "resources\luaskills-packages\THIRD_PARTY_NOTICES.md"
    $LicenseIndexPath = Join-Path $RuntimeInstallRoot "licenses\luaskills-packages\index.json"
    return (Test-Path -LiteralPath $MarkerFile) -and
        (Test-Path -LiteralPath $RuntimeManifestPath) -and
        (Test-Path -LiteralPath $PackagesManifestPath) -and
        (Test-Path -LiteralPath $ThirdPartyLicensesPath) -and
        (Test-Path -LiteralPath $ThirdPartyNoticesPath) -and
        (Test-Path -LiteralPath $LicenseIndexPath)
}

function Install-LuaRuntimePayloads {
    <#
    .SYNOPSIS
    Extract and install the runtime package plus the luaskills-packages bundle metadata into third_party.
    解压并安装 runtime package 与 luaskills-packages bundle 元数据到 third_party。

    .PARAMETER RuntimeArchivePath
    Verified runtime package archive path.
    已校验的 runtime package 压缩包路径。

    .PARAMETER BundleArchivePath
    Verified luaskills-packages bundle archive path.
    已校验的 luaskills-packages bundle 压缩包路径。

    .PARAMETER RuntimeTag
    Runtime package release tag.
    runtime package 发布标签。

    .PARAMETER Platform
    Current platform key.
    当前平台标识。
    #>
    param(
        [string]$RuntimeArchivePath,
        [string]$BundleArchivePath,
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
    $BundleTempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan_luaskills_bundle_{0}" -f $PID)

    foreach ($TempDir in @($RuntimeTempDir, $BundleTempDir)) {
        if (Test-Path -LiteralPath $TempDir) {
            Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        Ensure-Dir $TempDir
    }

    try {
        & $TarPath -xzf $RuntimeArchivePath -C $RuntimeTempDir
        Expand-Archive -Path $BundleArchivePath -DestinationPath $BundleTempDir -Force

        $BundleRoot = Resolve-BundleExtractRoot -ExtractRoot $BundleTempDir

        Clear-RuntimeInstallRoot

        foreach ($DirName in @("lua_packages", "libs", "resources", "licenses")) {
            Copy-DirectoryContents -Source (Join-Path $RuntimeTempDir $DirName) -Destination (Join-Path $RuntimeInstallRoot $DirName)
        }

        $PackagesResourcesRoot = Join-Path $RuntimeInstallRoot "resources\luaskills-packages"
        $PackagesLicensesRoot = Join-Path $RuntimeInstallRoot "licenses\luaskills-packages"
        Ensure-Dir $PackagesResourcesRoot
        Ensure-Dir $PackagesLicensesRoot

        foreach ($FileName in @(
            "THIRD_PARTY_LICENSES.json",
            "THIRD_PARTY_NOTICES.md",
            "install-manifest.json",
            "lua_packages.txt",
            "platform-support.json",
            "platform-support.md"
        )) {
            $SourcePath = Join-Path $BundleRoot $FileName
            if (Test-Path -LiteralPath $SourcePath) {
                Copy-Item -Force -LiteralPath $SourcePath -Destination (Join-Path $PackagesResourcesRoot $FileName)
            }
        }

        $HelpSource = Join-Path $BundleRoot "help"
        if (Test-Path -LiteralPath $HelpSource) {
            Copy-DirectoryContents -Source $HelpSource -Destination (Join-Path $PackagesResourcesRoot "help")
        }

        $BundleLicensesSource = Join-Path $BundleRoot "licenses"
        if (Test-Path -LiteralPath $BundleLicensesSource) {
            Copy-DirectoryContents -Source $BundleLicensesSource -Destination $PackagesLicensesRoot
        }

        Normalize-BundleLicenseIndexPaths -IndexPath (Join-Path $PackagesLicensesRoot "index.json")

        $RuntimeManifestPath = Join-Path $RuntimeInstallRoot "resources\lua-runtime-manifest.json"
        $PackagesManifestPath = Join-Path $RuntimeInstallRoot "resources\luaskills-packages-manifest.json"
        $ThirdPartyLicensesPath = Join-Path $PackagesResourcesRoot "THIRD_PARTY_LICENSES.json"
        $ThirdPartyNoticesPath = Join-Path $PackagesResourcesRoot "THIRD_PARTY_NOTICES.md"
        $LicenseIndexPath = Join-Path $PackagesLicensesRoot "index.json"
        if (-not (Test-Path -LiteralPath $RuntimeManifestPath)) {
            throw "Lua runtime manifest was not found after installing runtime packages."
        }
        if (-not (Test-Path -LiteralPath $PackagesManifestPath)) {
            throw "LuaSkills packages manifest was not found after installing runtime packages."
        }
        if (-not (Test-Path -LiteralPath $ThirdPartyLicensesPath)) {
            throw "LuaSkills packages third-party licenses file was not found after installing runtime packages."
        }
        if (-not (Test-Path -LiteralPath $ThirdPartyNoticesPath)) {
            throw "LuaSkills packages third-party notices file was not found after installing runtime packages."
        }
        if (-not (Test-Path -LiteralPath $LicenseIndexPath)) {
            throw "LuaSkills packages license index was not found after installing runtime packages."
        }

        Get-ChildItem -Path $RuntimeInstallRoot -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
        Write-Host "==> LuaSkills runtime payloads installed to $RuntimeInstallRoot"
    } finally {
        foreach ($TempDir in @($RuntimeTempDir, $BundleTempDir)) {
            Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

$ResolvedLuaRuntimeTag = Resolve-LuaRuntimePackagesTag `
    -Repo $LuaRuntimeRepo `
    -Series $LuaRuntimeSeries `
    -PackagesVersion $LuaRuntimePackagesVersion `
    -LegacyRuntimeVersion $LuaRuntimeVersion
$Platform = Get-CurrentPlatformKey
$RuntimeAssetName = "lua-runtime-packages-$Platform.tar.gz"
$BundleAssetName = "luaskills-packages-bundle-$ResolvedLuaRuntimeTag.zip"

Write-Host ""
Write-Host "=== LuaSkills Runtime Packages ==="
Write-Host "==> Runtime repo:    $LuaRuntimeRepo"
Write-Host "==> Runtime version: $ResolvedLuaRuntimeTag"
Write-Host "==> Platform:        $Platform"
Write-Host "==> This flow downloads only luaskills-packages runtime assets and bundle metadata."

Write-Host ""
Write-Host "=== Step 1: Runtime Package ==="
$RuntimeArchivePath = Save-ReleaseAssetWithSha256 -Repo $LuaRuntimeRepo -Tag $ResolvedLuaRuntimeTag -AssetName $RuntimeAssetName

Write-Host ""
Write-Host "=== Step 2: Runtime Metadata Bundle ==="
$BundleArchivePath = Save-ReleaseAssetWithSha256 `
    -Repo $LuaRuntimeRepo `
    -Tag $ResolvedLuaRuntimeTag `
    -AssetName $BundleAssetName `
    -ShaAssetName "luaskills-packages-bundle-$ResolvedLuaRuntimeTag.sha256"

Write-Host ""
Write-Host "=== Step 3: Install Runtime Payloads ==="
Install-LuaRuntimePayloads -RuntimeArchivePath $RuntimeArchivePath -BundleArchivePath $BundleArchivePath -RuntimeTag $ResolvedLuaRuntimeTag -Platform $Platform

Write-Host ""
Write-Host "==> Lua runtime dependencies ready."
