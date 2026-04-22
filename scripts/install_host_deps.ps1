# install_host_deps.ps1 - Install the host-side vldb-controller binary into third_party/
# Developer/build use only. End users do not need to invoke this manually.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_host_deps.ps1

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

$script:IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$script:IsMacOSPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
$script:IsLinuxPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)

$ThirdParty = Join-Path $ProjectDir "third_party"
$VldbControllerDir = Join-Path $ThirdParty "vldb_controller"
$VldbControllerBinDir = Join-Path $VldbControllerDir "bin"
$VldbControllerRepo = "OpenVulcan/vldb-controller"

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Find-LocalArchive {
    param([string]$AssetName)

    $CandidatePaths = @((Join-Path $ThirdParty $AssetName))
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
    $systemTar = "$env:SystemRoot\System32\tar.exe"
    if (Test-Path -LiteralPath $systemTar) { return $systemTar }
    $tarCommand = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($tarCommand) { return $tarCommand.Source }
    throw "tar.exe is required to extract host dependency archives"
}

function Get-CurrentArchitectureKey {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($arch) {
        "x64" { return "x86_64" }
        "arm64" { return "aarch64" }
        default { throw "Unsupported architecture for dependency bootstrap: $arch" }
    }
}

function Get-LatestRepoTag {
    param(
        [string]$Repo,
        [string]$DisplayName
    )

    $apiUrl = "https://api.github.com/repos/$Repo/tags?per_page=1"
    Write-Host "==> Querying latest $DisplayName tag..."
    $tags = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
    if (-not $tags) {
        throw "Latest $DisplayName tag lookup returned no results"
    }

    $firstTag = @($tags)[0]
    if (-not $firstTag.name) {
        throw "Latest $DisplayName tag is missing name"
    }

    return $firstTag.name
}

function Get-ReleaseByTagOrNull {
    param(
        [string]$Repo,
        [string]$TagName
    )

    $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/$TagName"
    try {
        return Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
    } catch {
        $statusCode = $null
        if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
            try {
                $statusCode = [int]$_.Exception.Response.StatusCode
            } catch {
                $statusCode = $null
            }
        }

        if ($statusCode -eq 404) {
            return $null
        }

        throw
    }
}

function Get-VldbControllerAssetInfo {
    $archKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($archKey -ne "x86_64") {
            throw "vldb-controller prebuilt host binary currently supports Windows x86_64 only. Current arch: $archKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            binary_name = "vldb-controller.exe"
        }
    }

    if ($script:IsLinuxPlatform) {
        $target = if ($archKey -eq "aarch64") { "aarch64-unknown-linux-gnu" } else { "x86_64-unknown-linux-gnu" }
        return @{
            target = $target
            archive_ext = ".tar.gz"
            binary_name = "vldb-controller"
        }
    }

    if ($script:IsMacOSPlatform) {
        $target = if ($archKey -eq "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" }
        return @{
            target = $target
            archive_ext = ".tar.gz"
            binary_name = "vldb-controller"
        }
    }

    throw "Unsupported platform for vldb-controller bootstrap"
}

function Install-VldbControllerBinary {
    Ensure-Dir $VldbControllerDir
    Ensure-Dir $VldbControllerBinDir

    $assetInfo = Get-VldbControllerAssetInfo
    $tarPath = Get-AvailableTarPath
    $release = $null
    $tagName = $null
    $assetName = $null
    $markerFile = $null
    $localArchivePath = $null

    $localPattern = "vldb-controller-v*-$($assetInfo.target)$($assetInfo.archive_ext)"
    $localArchive = Get-ChildItem -Path $ThirdParty -File -Filter $localPattern -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if (-not $localArchive) {
        $localArchive = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ChildItem -Path $_.FullName -File -Filter $localPattern -ErrorAction SilentlyContinue } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }

    if ($localArchive) {
        $assetName = $localArchive.Name
        if ($assetName -match '^vldb-controller-(v.+)-[^-]+(?:-[^-]+){2,3}(\.zip|\.tar\.gz)$') {
            $tagName = $Matches[1]
        } else {
            throw "Unable to parse vldb-controller tag from local archive name: $assetName"
        }
        $markerFile = Join-Path $VldbControllerDir ".installed-$tagName-$($assetInfo.target)"
        $localArchivePath = $localArchive.FullName
    } else {
        $tagName = Get-LatestRepoTag -Repo $VldbControllerRepo -DisplayName "vldb-controller"
        $assetName = "vldb-controller-$tagName-$($assetInfo.target)$($assetInfo.archive_ext)"
        $markerFile = Join-Path $VldbControllerDir ".installed-$tagName-$($assetInfo.target)"
        $localArchivePath = Find-LocalArchive -AssetName $assetName
    }

    $binaryDest = Join-Path $VldbControllerBinDir $assetInfo.binary_name

    if ((Test-Path -LiteralPath $markerFile) -and (Test-Path -LiteralPath $binaryDest)) {
        Write-Host "==> vldb-controller host binary already installed ($assetName)."
        return
    }

    if (-not $localArchivePath) {
        $release = Get-ReleaseByTagOrNull -Repo $VldbControllerRepo -TagName $tagName
        if (-not $release) {
            if (Test-Path -LiteralPath $binaryDest) {
                Write-Warning ("vldb-controller release assets are not published for tag {0}. Reusing the existing local binary at {1} and refreshing the install marker." -f $tagName, $binaryDest)
                Get-ChildItem -Path $VldbControllerDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
                    Remove-Item -Force -ErrorAction SilentlyContinue
                New-Item -ItemType File -Path $markerFile -Force | Out-Null
                return
            }

            throw ("vldb-controller tag '{0}' currently has no GitHub Release host binary asset. Please download the artifact '{1}' manually and place it under third_party (or one direct child directory) before rerunning." -f $tagName, $assetName)
        }

        $asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
        if (-not $asset) {
            $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
            throw ("vldb-controller asset '{0}' not found in release '{1}'. Available assets: {2}" -f $assetName, $tagName, $available)
        }
    }

    $tempDir = Join-Path $env:TEMP ('vldb_controller_{0}' -f $PID)
    if (Test-Path -LiteralPath $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $tempDir

    try {
        $archivePath = Join-Path $tempDir $assetName
        if ($localArchivePath) {
            Write-Host "==> Using local vldb-controller package: $localArchivePath"
            Copy-Item -LiteralPath $localArchivePath -Destination $archivePath -Force
        } else {
            Write-Host "==> Downloading vldb-controller package: $assetName"
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath -UseBasicParsing
        }

        if ($assetInfo.archive_ext -eq ".zip") {
            Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force
        } else {
            & $tarPath -xzf $archivePath -C $tempDir
        }

        $binarySource = Get-ChildItem -Path $tempDir -Recurse -File -Filter $assetInfo.binary_name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $binarySource) {
            throw ("Executable '{0}' not found after extracting {1}" -f $assetInfo.binary_name, $assetName)
        }

        Copy-Item -LiteralPath $binarySource.FullName -Destination $binaryDest -Force

        Get-ChildItem -Path $VldbControllerDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $markerFile -Force | Out-Null
        Write-Host "==> vldb-controller host binary installed successfully."
    } finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-VldbControllerBinary
