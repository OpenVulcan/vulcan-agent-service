# install_host_deps.ps1 — Install host-level native runtime dependencies into third_party/
# Developer/build use only. End users do not need to invoke this manually.
# This script currently provisions the vldb-lancedb / vldb-sqlite dynamic library packages.
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
$VldbSqliteDir = Join-Path $ThirdParty "vldb_sqlite"
$VldbSqliteIncludeDir = Join-Path $VldbSqliteDir "include"
$VldbSqliteDocsDir = Join-Path $VldbSqliteDir "docs"
$VldbSqliteRepo = "OpenVulcan/vldb-sqlite"

# ============================================================
# Helpers
# ============================================================
function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path $Path)) { New-Item -ItemType Directory -Path $Path -Force | Out-Null }
}

function Find-LocalArchive {
    <#
    .SYNOPSIS
    在 third_party 顶层及其直接子目录中查找匹配的本地压缩包 / Find a matching local archive under third_party and its direct child directories.
    #>
    param([string]$AssetName)

    $CandidatePaths = @(
        (Join-Path $ThirdParty $AssetName)
    )

    $DirectSubDirs = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue
    foreach ($Dir in $DirectSubDirs) {
        $CandidatePaths += Join-Path $Dir.FullName $AssetName
    }

    foreach ($Candidate in $CandidatePaths) {
        if (Test-Path -LiteralPath $Candidate) {
            return $Candidate
        }
    }

    return $null
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

function Get-LatestRepoTag {
    <#
    .SYNOPSIS
    查询仓库当前最新 tag / Query the latest tag for a repository.
    #>
    param(
        [string]$Repo,
        [string]$DisplayName
    )

    $ApiUrl = "https://api.github.com/repos/$Repo/tags?per_page=1"
    Write-Host "==> Querying latest $DisplayName tag..."
    $Tags = Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing
    if (-not $Tags) {
        throw "Latest $DisplayName tag lookup returned no results / 最新 $DisplayName tag 查询结果为空"
    }

    $FirstTag = @($Tags)[0]
    if (-not $FirstTag.name) {
        throw "Latest $DisplayName tag is missing name / 最新 $DisplayName tag 缺少 name 字段"
    }

    return $FirstTag.name
}

function Get-ReleaseByTagOrNull {
    <#
    .SYNOPSIS
    按 tag 查询 Release；若不存在则返回 null / Query a release by tag and return null when it does not exist.
    #>
    param(
        [string]$Repo,
        [string]$TagName
    )

    $ApiUrl = "https://api.github.com/repos/$Repo/releases/tags/$TagName"
    try {
        return Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing
    } catch {
        $StatusCode = $null
        if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
            try {
                $StatusCode = [int]$_.Exception.Response.StatusCode
            } catch {
                $StatusCode = $null
            }
        }

        if ($StatusCode -eq 404) {
            return $null
        }

        throw
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
    $Release = $null
    $TagName = $null
    $AssetName = $null
    $MarkerFile = $null
    $LocalArchivePath = $null

    $LocalPattern = "vldb-lancedb-lib-v*-$($AssetInfo.target)$($AssetInfo.archive_ext)"
    $LocalArchive = Get-ChildItem -Path $ThirdParty -File -Filter $LocalPattern -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    if (-not $LocalArchive) {
        $LocalArchive = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ChildItem -Path $_.FullName -File -Filter $LocalPattern -ErrorAction SilentlyContinue } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }

    if ($LocalArchive) {
        $AssetName = $LocalArchive.Name
        if ($AssetName -match '^vldb-lancedb-lib-(v.+)-[^-]+(?:-[^-]+){2,3}(\.zip|\.tar\.gz)$') {
            $TagName = $Matches[1]
        } else {
            throw "Unable to parse vldb-lancedb tag from local archive name: $AssetName"
        }
        $MarkerFile = Join-Path $VldbLancedbDir ".installed-$TagName-$($AssetInfo.target)"
        $LocalArchivePath = $LocalArchive.FullName
    } else {
        $TagName = Get-LatestRepoTag -Repo $VldbLancedbRepo -DisplayName "vldb-lancedb"
        $AssetName = "vldb-lancedb-lib-$TagName-$($AssetInfo.target)$($AssetInfo.archive_ext)"
        $MarkerFile = Join-Path $VldbLancedbDir ".installed-$TagName-$($AssetInfo.target)"
        $LocalArchivePath = Find-LocalArchive -AssetName $AssetName
    }

    $LibraryDest = Join-Path $DepsDir $AssetInfo.library_name

    if ((Test-Path $MarkerFile) -and (Test-Path $LibraryDest)) {
        Write-Host "==> vldb-lancedb library already installed ($AssetName)."
        return
    }

    if (-not $LocalArchivePath) {
        $Release = Get-ReleaseByTagOrNull -Repo $VldbLancedbRepo -TagName $TagName
        if (-not $Release) {
            if (Test-Path $LibraryDest) {
                Write-Warning "vldb-lancedb release assets are not published for tag $TagName. Reusing the existing local binary at $LibraryDest and refreshing the install marker / 当前 tag 未发布 Release 资产，继续复用已有本地动态库并刷新安装标记。"
                Get-ChildItem -Path $VldbLancedbDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
                New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
                return
            }

            throw "vldb-lancedb tag '$TagName' currently has no GitHub Release library asset. Please download the Actions artifact '$AssetName' manually and place it under third_party (or one direct child directory) before rerunning / 当前 tag 未发布 GitHub Release 库资产，请先手动下载 Actions artifact '$AssetName' 并放到 third_party（或其一级子目录）后再重试。"
        }

        $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
        if (-not $Asset) {
            $Available = ($Release.assets | ForEach-Object { $_.name }) -join ", "
            throw "vldb-lancedb asset '$AssetName' not found in release '$TagName'. Available assets: $Available / 目标 tag 对应的 Release 中未找到该库资产。"
        }
    }

    $TempDir = Join-Path $env:TEMP ("vldb_lancedb_{0}" -f $PID)
    if (Test-Path $TempDir) {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $TempDir

    try {
        $ArchivePath = Join-Path $TempDir $AssetName
        if ($LocalArchivePath) {
            Write-Host "==> Using local vldb-lancedb library package: $LocalArchivePath"
            Copy-Item $LocalArchivePath $ArchivePath -Force
        } else {
            Write-Host "==> Downloading vldb-lancedb library package: $AssetName"
            Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ArchivePath -UseBasicParsing
        }

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

function Get-VldbSqliteAssetInfo {
    <#
    .SYNOPSIS
    解析当前平台对应的 vldb-sqlite 库模式资产信息 / Resolve the vldb-sqlite library asset info for the current platform.
    #>
    $ArchKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($ArchKey -ne "x86_64") {
            throw "vldb-sqlite prebuilt library currently supports Windows x86_64 only. Current arch: $ArchKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            library_name = "vldb_sqlite.dll"
        }
    }

    if ($script:IsLinuxPlatform) {
        $Target = if ($ArchKey -eq "aarch64") { "aarch64-unknown-linux-gnu" } else { "x86_64-unknown-linux-gnu" }
        return @{
            target = $Target
            archive_ext = ".tar.gz"
            library_name = "libvldb_sqlite.so"
        }
    }

    if ($script:IsMacOSPlatform) {
        $Target = if ($ArchKey -eq "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" }
        return @{
            target = $Target
            archive_ext = ".tar.gz"
            library_name = "libvldb_sqlite.dylib"
        }
    }

    throw "Unsupported platform for vldb-sqlite bootstrap."
}

function Install-VldbSqliteLibrary {
    <#
    .SYNOPSIS
    安装宿主级 vldb-sqlite 动态库 / Install the host-level vldb-sqlite dynamic library.
    #>
    Ensure-Dir $DepsDir
    Ensure-Dir $VldbSqliteDir
    Ensure-Dir $VldbSqliteIncludeDir
    Ensure-Dir $VldbSqliteDocsDir

    $AssetInfo = Get-VldbSqliteAssetInfo
    $TarPath = Get-AvailableTarPath
    $Release = $null
    $TagName = $null
    $AssetName = $null
    $MarkerFile = $null
    $LocalArchivePath = $null

    $LocalPattern = "vldb-sqlite-lib-v*-$($AssetInfo.target)$($AssetInfo.archive_ext)"
    $LocalArchive = Get-ChildItem -Path $ThirdParty -File -Filter $LocalPattern -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if (-not $LocalArchive) {
        $LocalArchive = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ChildItem -Path $_.FullName -File -Filter $LocalPattern -ErrorAction SilentlyContinue } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }

    if ($LocalArchive) {
        $AssetName = $LocalArchive.Name
        if ($AssetName -match '^vldb-sqlite-lib-(v.+)-[^-]+(?:-[^-]+){2,3}(\.zip|\.tar\.gz)$') {
            $TagName = $Matches[1]
        } else {
            throw "Unable to parse vldb-sqlite tag from local archive name: $AssetName"
        }
        $MarkerFile = Join-Path $VldbSqliteDir ".installed-$TagName-$($AssetInfo.target)"
        $LocalArchivePath = $LocalArchive.FullName
    } else {
        $TagName = Get-LatestRepoTag -Repo $VldbSqliteRepo -DisplayName "vldb-sqlite"
        $AssetName = "vldb-sqlite-lib-$TagName-$($AssetInfo.target)$($AssetInfo.archive_ext)"
        $MarkerFile = Join-Path $VldbSqliteDir ".installed-$TagName-$($AssetInfo.target)"
        $LocalArchivePath = Find-LocalArchive -AssetName $AssetName
    }

    $LibraryDest = Join-Path $DepsDir $AssetInfo.library_name

    if ((Test-Path $MarkerFile) -and (Test-Path $LibraryDest)) {
        Write-Host "==> vldb-sqlite library already installed ($AssetName)."
        return
    }

    if (-not $LocalArchivePath) {
        $Release = Get-ReleaseByTagOrNull -Repo $VldbSqliteRepo -TagName $TagName
        if (-not $Release) {
            if (Test-Path $LibraryDest) {
                Write-Warning "vldb-sqlite release assets are not published for tag $TagName. Reusing the existing local binary at $LibraryDest and refreshing the install marker / 当前 tag 未发布 Release 资产，继续复用已有本地动态库并刷新安装标记。"
                Get-ChildItem -Path $VldbSqliteDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
                New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
                return
            }

            throw "vldb-sqlite tag '$TagName' currently has no GitHub Release library asset. Please download the Actions artifact '$AssetName' manually and place it under third_party (or one direct child directory) before rerunning / 当前 tag 未发布 GitHub Release 库资产，请先手动下载 Actions artifact '$AssetName' 并放到 third_party（或其一级子目录）后再重试。"
        }

        $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
        if (-not $Asset) {
            $Available = ($Release.assets | ForEach-Object { $_.name }) -join ", "
            throw "vldb-sqlite asset '$AssetName' not found in release '$TagName'. Available assets: $Available / 目标 tag 对应的 Release 中未找到该库资产。"
        }
    }

    $TempDir = Join-Path $env:TEMP ("vldb_sqlite_{0}" -f $PID)
    if (Test-Path $TempDir) {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $TempDir

    try {
        $ArchivePath = Join-Path $TempDir $AssetName
        if ($LocalArchivePath) {
            Write-Host "==> Using local vldb-sqlite library package: $LocalArchivePath"
            Copy-Item $LocalArchivePath $ArchivePath -Force
        } else {
            Write-Host "==> Downloading vldb-sqlite library package: $AssetName"
            Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ArchivePath -UseBasicParsing
        }

        if ($AssetInfo.archive_ext -eq ".zip") {
            Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force
        } else {
            & $TarPath -xzf $ArchivePath -C $TempDir
        }

        $LibrarySource = Get-ChildItem -Path $TempDir -Recurse -File -Filter $AssetInfo.library_name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $LibrarySource) {
            throw "Dynamic library '$($AssetInfo.library_name)' not found after extracting $AssetName"
        }

        Copy-Item $LibrarySource.FullName $LibraryDest -Force

        $HeaderSource = Get-ChildItem -Path $TempDir -Recurse -File -Filter "vldb_sqlite.h" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($HeaderSource) {
            Copy-Item $HeaderSource.FullName (Join-Path $VldbSqliteIncludeDir "vldb_sqlite.h") -Force
        }

        $DocSource = Get-ChildItem -Path $TempDir -Recurse -File -Filter "LIBRARY_USAGE.zh-CN.md" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($DocSource) {
            Copy-Item $DocSource.FullName (Join-Path $VldbSqliteDocsDir "LIBRARY_USAGE.zh-CN.md") -Force
        }

        Get-ChildItem -Path $VldbSqliteDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $MarkerFile -Force | Out-Null
        Write-Host "==> vldb-sqlite library installed successfully."
    } finally {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-VldbLancedbLibrary
Install-VldbSqliteLibrary
