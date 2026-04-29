# install_lua_deps.ps1 - Download the official LuaSkills runtime dependency package.
# install_lua_deps.ps1 - 下载 LuaSkills 官方运行期依赖包。
# Developer/build use only. The script does not compile Lua, LuaRocks, or C dependencies.
# 仅供开发与构建使用；该脚本不会编译 Lua、LuaRocks 或 C 依赖。
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# RuntimeRepo stores the official LuaSkills repository that publishes runtime packages.
# RuntimeRepo 保存发布运行期包的 LuaSkills 官方仓库。
$RuntimeRepo = if ([string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_REPO)) { "LuaSkills/luaskills" } else { $env:LUA_RUNTIME_REPO.Trim() }

# ThirdParty stores all downloaded dependency payloads outside the tracked source tree.
# ThirdParty 保存所有下载后的依赖载荷，避免写入受版本控制的源码目录。
$ThirdParty = Join-Path $ProjectDir "third_party"

# RuntimeInstallRoot stores the extracted official LuaSkills runtime package.
# RuntimeInstallRoot 保存解压后的 LuaSkills 官方运行期包。
$RuntimeInstallRoot = Join-Path $ThirdParty "luaskills_runtime"

# DownloadCache stores verified archives and sidecar checksums.
# DownloadCache 保存已校验的压缩包与旁路校验文件。
$DownloadCache = Join-Path $ThirdParty "downloads"

# HostDepsScriptPath points at the host-native dependency downloader.
# HostDepsScriptPath 指向宿主原生依赖下载脚本。
$HostDepsScriptPath = Join-Path $PSScriptRoot "install_host_deps.ps1"

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

function Get-LuaSkillsVersionTag {
    <#
    .SYNOPSIS
    Resolve the LuaSkills release tag from the environment or Cargo dependency.
    从环境变量或 Cargo 依赖解析 LuaSkills 发布标签。

    .OUTPUTS
    Release tag such as v0.2.2.
    形如 v0.2.2 的发布标签。
    #>
    if (-not [string]::IsNullOrWhiteSpace($env:LUA_RUNTIME_VERSION)) {
        $Configured = $env:LUA_RUNTIME_VERSION.Trim()
        if ($Configured.StartsWith("v")) { return $Configured }
        return "v$Configured"
    }

    $CargoToml = Join-Path $ProjectDir "Cargo.toml"
    if (Test-Path -LiteralPath $CargoToml) {
        foreach ($Line in Get-Content -LiteralPath $CargoToml) {
            if ($Line -match '^\s*luaskills\s*=\s*"([^"]+)"') {
                return "v$($Matches[1])"
            }
        }
    }

    return "v0.2.2"
}

function Get-CurrentPlatformKey {
    <#
    .SYNOPSIS
    Resolve the official LuaSkills runtime package platform key.
    解析 LuaSkills 官方运行期包的平台标识。

    .OUTPUTS
    Platform key such as windows-x64, linux-x64, linux-arm64, macos-x64, or macos-arm64.
    平台标识，例如 windows-x64、linux-x64、linux-arm64、macos-x64 或 macos-arm64。
    #>
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    $ArchKey = switch ($Arch) {
        "x64" { "x64" }
        "arm64" { "arm64" }
        default { throw "Unsupported architecture for LuaSkills runtime package: $Arch" }
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        if ($ArchKey -ne "x64") {
            throw "LuaSkills official runtime package currently supports Windows x64 only. Current arch: $ArchKey"
        }
        return "windows-x64"
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        return if ($ArchKey -eq "arm64") { "macos-arm64" } else { "macos-x64" }
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        return if ($ArchKey -eq "arm64") { "linux-arm64" } else { "linux-x64" }
    }

    throw "Unsupported operating system for LuaSkills runtime package."
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
    if (Test-Path -LiteralPath $SystemTar) { return $SystemTar }

    $TarCommand = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($TarCommand) { return $TarCommand.Source }

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
    Read the first checksum token from a .sha256 sidecar file.
    从 .sha256 旁路文件读取第一个校验值片段。

    .PARAMETER ShaPath
    Local .sha256 file path.
    本地 .sha256 文件路径。

    .OUTPUTS
    Lower-case SHA-256 digest.
    小写 SHA-256 摘要。
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

    .OUTPUTS
    Boolean value indicating whether the archive is valid.
    表示压缩包是否有效的布尔值。
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

function Save-OfficialLuaRuntimeArchive {
    <#
    .SYNOPSIS
    Download and verify the official LuaSkills runtime package archive.
    下载并校验 LuaSkills 官方运行期包。

    .PARAMETER Repo
    GitHub repository in owner/name form.
    owner/name 形式的 GitHub 仓库。

    .PARAMETER Tag
    Release tag name.
    Release 标签名。

    .PARAMETER AssetName
    Exact runtime package asset name.
    精确的运行期包资产名称。

    .OUTPUTS
    Verified archive path.
    已校验的压缩包路径。
    #>
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$AssetName
    )

    Ensure-Dir $DownloadCache

    $ArchivePath = Join-Path $DownloadCache $AssetName
    $ShaPath = "$ArchivePath.sha256"
    $ArchiveUrl = Get-ReleaseAssetUrl -Repo $Repo -Tag $Tag -AssetName $AssetName
    $ShaUrl = Get-ReleaseAssetUrl -Repo $Repo -Tag $Tag -AssetName "$AssetName.sha256"

    Write-Host "==> Downloading checksum: $ShaUrl"
    Invoke-WebRequest -Uri $ShaUrl -OutFile $ShaPath -UseBasicParsing
    $Expected = Get-ExpectedSha256 -ShaPath $ShaPath

    if (Test-ArchiveSha256 -ArchivePath $ArchivePath -ExpectedSha256 $Expected) {
        Write-Host "==> Reusing verified LuaSkills runtime archive: $ArchivePath"
        return $ArchivePath
    }

    Write-Host "==> Downloading LuaSkills runtime package: $ArchiveUrl"
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
    Clear the extracted official runtime package directory inside third_party.
    清理 third_party 内已解压的官方运行期包目录。
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
    Copy direct directory contents while preserving the official package layout.
    复制目录直属内容并保持官方包布局。

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
        throw "Required LuaSkills runtime directory is missing from package: $Source"
    }

    Ensure-Dir $Destination
    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
}

function Install-OfficialLuaRuntimePackage {
    <#
    .SYNOPSIS
    Extract the official LuaSkills runtime archive into third_party/luaskills_runtime.
    将 LuaSkills 官方运行期压缩包解压到 third_party/luaskills_runtime。

    .PARAMETER ArchivePath
    Verified runtime archive path.
    已校验的运行期压缩包路径。

    .PARAMETER Tag
    Release tag name.
    Release 标签名。

    .PARAMETER Platform
    Runtime package platform key.
    运行期包平台标识。
    #>
    param(
        [string]$ArchivePath,
        [string]$Tag,
        [string]$Platform
    )

    $MarkerFile = Join-Path $RuntimeInstallRoot ".installed-$Tag-$Platform"
    $ManifestFile = Join-Path $RuntimeInstallRoot "resources\lua-runtime-manifest.json"
    if ((Test-Path -LiteralPath $MarkerFile) -and (Test-Path -LiteralPath $ManifestFile)) {
        Write-Host "==> LuaSkills runtime package already installed ($Tag, $Platform)."
        return
    }

    $TarPath = Get-AvailableTarPath
    $TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan_luaskills_runtime_{0}" -f $PID)
    if (Test-Path -LiteralPath $TempDir) {
        Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $TempDir

    try {
        & $TarPath -xzf $ArchivePath -C $TempDir
        Clear-RuntimeInstallRoot

        foreach ($DirName in @("lua_packages", "libs", "resources", "licenses")) {
            Copy-DirectoryContents -Source (Join-Path $TempDir $DirName) -Destination (Join-Path $RuntimeInstallRoot $DirName)
        }

        New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
        Write-Host "==> LuaSkills official runtime installed to $RuntimeInstallRoot"
    } finally {
        Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-HostDependencyInstall {
    <#
    .SYNOPSIS
    Download host-native dependencies that are not part of the LuaSkills runtime package.
    下载不属于 LuaSkills runtime 包的宿主原生依赖。
    #>
    if (-not (Test-Path -LiteralPath $HostDepsScriptPath)) {
        throw "Missing host dependency script: $HostDepsScriptPath"
    }

    $PowerShellCommand = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCommand) {
        & $PowerShellCommand.Source -NoProfile -ExecutionPolicy Bypass -File $HostDepsScriptPath
    } else {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $HostDepsScriptPath
    }

    if ($LASTEXITCODE -ne 0) {
        throw "Host dependency downloader failed with exit code $LASTEXITCODE"
    }
}

$RuntimeVersion = Get-LuaSkillsVersionTag
$Platform = Get-CurrentPlatformKey
$AssetName = "lua-runtime-$Platform.tar.gz"

Write-Host ""
Write-Host "=== LuaSkills Official Dependencies ==="
Write-Host "==> Repository: $RuntimeRepo"
Write-Host "==> Version:    $RuntimeVersion"
Write-Host "==> Platform:   $Platform"
Write-Host "==> This flow downloads official packages only; no LuaRocks, vcpkg, or source compilation is performed."

Write-Host ""
Write-Host "=== Step 1: Host Dependencies ==="
Invoke-HostDependencyInstall

Write-Host ""
Write-Host "=== Step 2: LuaSkills Runtime Package ==="
$ArchivePath = Save-OfficialLuaRuntimeArchive -Repo $RuntimeRepo -Tag $RuntimeVersion -AssetName $AssetName
Install-OfficialLuaRuntimePackage -ArchivePath $ArchivePath -Tag $RuntimeVersion -Platform $Platform

Write-Host ""
Write-Host "==> Dependencies ready."
