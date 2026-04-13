# install_lua_deps.ps1 — Install Lua C modules via luarocks into third_party/lua_packages/
# Developer/build use only. End users do not need luarocks.
# Reuses LuaJIT source from luajit-src cargo crate — no network download needed.
# Reads package list AND C dependencies from scripts/lua_packages.txt.
# All build tools are downloaded to third_party/tools/ — system environment is NOT modified.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_lua_deps.ps1

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# ============================================================
# Configuration
# ============================================================
$ThirdParty = Join-Path $ProjectDir "third_party"
$ToolsDir   = Join-Path $ThirdParty "tools"
$LuaJITDir  = Join-Path $ThirdParty "luajit"
$LuaPackages = Join-Path $ThirdParty "lua_packages"
$LuarocksDir = Join-Path $ThirdParty "luarocks"
$DepsDir    = Join-Path $ThirdParty "deps"

# GitHub repo for pre-built deps (format: owner/repo)
$GitHubRepo = "OpenVulcan/vulcan-mcp"

# ============================================================
# Helpers: directory / download / extract
# ============================================================
function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path $Path)) { New-Item -ItemType Directory -Path $Path -Force | Out-Null }
}

function Download-Extract-TarGz {
    param([string]$Url, [string]$DestDir)
    $Archive = Join-Path $DestDir "source.tar.gz"
    if (-not (Test-Path $Archive)) {
        Invoke-WebRequest -Uri $Url -OutFile $Archive -UseBasicParsing
    }
    & $TarPath -xzf $Archive -C $DestDir
    Remove-Item $Archive -Force -ErrorAction SilentlyContinue
    return (Get-ChildItem $DestDir -Directory | Where-Object { $_.Name -ne "source" } | Sort-Object Name | Select-Object -Last 1).FullName
}

function Download-Extract-Zip {
    param([string]$Url, [string]$DestDir)
    $Archive = Join-Path $DestDir "archive.zip"
    if (-not (Test-Path $Archive)) {
        Invoke-WebRequest -Uri $Url -OutFile $Archive -UseBasicParsing
    }
    Expand-Archive $Archive $DestDir -Force
    Remove-Item $Archive -Force -ErrorAction SilentlyContinue
}

# ============================================================
# Dependency detection & local install
# ============================================================

# Script-local tool directories (appended to by Install-* functions,
# read by Activate-LocalTools to build the isolated build PATH).
$script:ToolDirs = @()
# VS install path (set by Check-VsTools if found)
$script:VsInstallPath = $null

function Detect-Tool {
    param([string]$Name, [scriptblock]$Check, [scriptblock]$Install, [string]$Desc)
    $result = & $Check
    if ($result) {
        Write-Host "  [OK] $Desc -> $result"
        return $result
    }
    Write-Host "  [MISSING] $Desc"
    Write-Host "    Installing to third_party/tools/..."
    $installPath = & $Install
    if (-not $installPath) {
        throw "Failed to install $Desc. Please install manually."
    }
    Write-Host "  [OK] $Desc -> $installPath (project-local)"
    return $installPath
}

# --- Perl ---
function Check-Perl {
    $p = Get-Command "perl.exe" -ErrorAction SilentlyContinue
    if ($p -and $p.Source -notmatch "msys|cygwin|Git") {
        $v = & perl.exe -e "print \$^V"
        return "$v ($($p.Source))"
    }
    return $null
}
function Install-Perl {
    $perlDir = Join-Path $ToolsDir "perl"
    Ensure-Dir $perlDir

    # StrawberryPerl portable zip
    $url = "https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit-portable.zip"
    try {
        Download-Extract-Zip -Url $url -DestDir $perlDir
    } catch {
        # Fallback URL
        $url2 = "https://strawberryperl.com/download/5.38.2.2/strawberry-perl-5.38.2.2-64bit-portable.zip"
        Download-Extract-Zip -Url $url2 -DestDir $perlDir
    }

    $perlBin = Join-Path $perlDir "perl\bin"
    $cBin    = Join-Path $perlDir "c\bin"
    if (-not (Test-Path $perlBin)) {
        # Auto-detect after extraction
        $found = Get-ChildItem $perlDir -Recurse -Filter "perl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) { $perlBin = $found.DirectoryName }
    }
    if (Test-Path $perlBin) {
        $script:ToolDirs += $perlBin
        if (Test-Path $cBin) { $script:ToolDirs += $cBin }
        return $perlBin
    }
    Write-Host "    ERROR: perl.exe not found after extraction. Contents:" -ForegroundColor Red
    Get-ChildItem $perlDir -Directory -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "      dir: $($_.Name)" }
    return $null
}

# --- cmake ---
function Check-Cmake {
    $c = Get-Command "cmake.exe" -ErrorAction SilentlyContinue
    if ($c) {
        return "$(& cmake.exe --version | Select-Object -First 1) ($($c.Source))"
    }
    return $null
}
function Install-Cmake {
    $cmakeDir = Join-Path $ToolsDir "cmake"
    Ensure-Dir $cmakeDir

    $version = "3.31.6"
    $url = "https://github.com/Kitware/CMake/releases/download/v$version/cmake-$version-windows-x86_64.zip"
    try {
        Download-Extract-Zip -Url $url -DestDir $cmakeDir
    } catch {
        Write-Host "    GitHub download failed, trying alternative..."
        $url2 = "https://cmake.org/files/v$([System.Version]$version).Major.$([System.Version]$version).Minor/cmake-$version-windows-x86_64.zip"
        Download-Extract-Zip -Url $url2 -DestDir $cmakeDir
    }

    $cmakeBin = Join-Path $cmakeDir "cmake-$version-windows-x86_64\bin"
    if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
        $script:ToolDirs += $cmakeBin
        return $cmakeBin
    }
    # Check if extracted to a different name pattern
    $found = Get-ChildItem $cmakeDir -Directory -Filter "cmake-*" | Sort-Object Name | Select-Object -Last 1
    if ($found) {
        $cmakeBin = Join-Path $found.FullName "bin"
        if (Test-Path (Join-Path $cmakeBin "cmake.exe")) {
            $script:ToolDirs += $cmakeBin
            return $cmakeBin
        }
    }
    return $null
}

# --- tar ---
function Check-Tar {
    # Windows 10+ has tar.exe in System32
    $systemTar = "$env:SystemRoot\System32\tar.exe"
    if (Test-Path $systemTar) { return $systemTar }
    $t = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($t) { return $t.Source }
    return $null
}
function Install-Tar {
    # Use Git for Windows tar if available
    $gitTar = "C:\Program Files\Git\usr\bin\tar.exe"
    if (Test-Path $gitTar) {
        $script:ToolDirs += (Split-Path $gitTar)
        return $gitTar
    }
    return $null
}

# --- vcpkg ---
# Project-local vcpkg paths
$VcpkgDir = Join-Path $ToolsDir "vcpkg"
$VcpkgExePath = Join-Path $VcpkgDir "vcpkg.exe"
# Where vcpkg-init.ps1 installs to
$UserVcpkgDir = Join-Path $env:USERPROFILE ".vcpkg"

function Check-Vcpkg {
    $localBinName = if ($IsWindows) { "vcpkg.exe" } else { "vcpkg" }

    # 1. From user's ~/.vcpkg (installed via vcpkg-init.ps1) — use directly, no copy needed
    $userExe = Join-Path $UserVcpkgDir $localBinName
    if (Test-Path $userExe) {
        $script:VcpkgExe = $userExe
        $ver = & $userExe version 2>$null | Select-Object -First 1
        return "$ver (~/.vcpkg)"
    }

    # 2. Also check project-local as fallback (if manually placed there)
    $targetPath = Join-Path $VcpkgDir $localBinName
    if (Test-Path $targetPath) {
        $script:VcpkgExe = $targetPath
        $ver = & $targetPath version 2>$null | Select-Object -First 1
        return "$ver (project-local)"
    }

    return $null
}
function Install-Vcpkg {
    # Use official vcpkg-init.ps1 for one-click install
    Write-Host "    Running official vcpkg bootstrap (aka.ms/vcpkg-init.ps1)..."
    try {
        iex (iwr -UseBasic "https://aka.ms/vcpkg-init.ps1")
    } catch {
        Write-Host "    Bootstrap failed: $_" -ForegroundColor Red
        return $null
    }

    # Re-detect from ~/.vcpkg
    return (Check-Vcpkg)
}

# Install deps via vcpkg into a local directory
function Install-Deps-With-Vcpkg {
    param([string[]]$DepNames)
    $VcpkgInstallDir = Join-Path $DepsDir "vcpkg_installed"
    Ensure-Dir $VcpkgInstallDir

    # Check if we already installed all deps
    $installed = $true
    foreach ($dep in $DepNames) {
        $triplet = "${dep}:x64-windows-static"
        $manifestFile = Join-Path $VcpkgInstallDir "info" "$triplet.list"
        if (-not (Test-Path $manifestFile)) {
            $installed = $false
            break
        }
    }
    if ($installed) {
        Write-Host "  ==> All deps already installed via vcpkg at $VcpkgInstallDir"
        return $VcpkgInstallDir
    }

    Write-Host "  ==> Installing via vcpkg: $($DepNames -join ', ') (x64-windows-static)"
    Write-Host "  ==> Output directory: $VcpkgInstallDir"
    Write-Host "  ==> Note: first run will download and compile from source (5-15 min for OpenSSL)"

    # vcpkg 2025+ requires manifest with baseline. Create temp vcpkg.json
    $tempDir = Join-Path $env:TEMP "vulcan_vcpkg_$pid"
    Ensure-Dir $tempDir
    $vcpkgJson = Join-Path $tempDir "vcpkg.json"

    # Get baseline commit SHA from vcpkg bundle
    $bundleInfo = Join-Path $UserVcpkgDir "vcpkg-bundle.json"
    $baseline = "cb2981c4e03d421fa03b9bb5044cd1986180e7e4" # fallback
    if (Test-Path $bundleInfo) {
        $bi = Get-Content $bundleInfo | ConvertFrom-Json
        if ($bi.embeddedsha) { $baseline = $bi.embeddedsha }
    }

    $jsonContent = @{
        name = "vulcan-deps"
        version = "1.0.0"
        dependencies = $DepNames
        "builtin-baseline" = $baseline
    } | ConvertTo-Json -Depth 5
    Set-Content -Path $vcpkgJson -Value $jsonContent -Encoding UTF8

    # In manifest mode, vcpkg install takes NO package arguments.
    # Packages come from vcpkg.json dependencies.
    $args = @("install", "--triplet=x64-windows-static", "--x-install-root=$VcpkgInstallDir", "--keep-going")
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $script:VcpkgExe
    $psi.Arguments = $args -join " "
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $false
    $psi.WorkingDirectory = $tempDir

    $proc = [System.Diagnostics.Process]::Start($psi)
    $proc.WaitForExit()

    # Clean up temp dir
    Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue

    # Check if packages were actually installed (vcpkg may return exit code 1
    # for warnings while still succeeding)
    $installedCount = 0
    foreach ($dep in $DepNames) {
        $triplet = "${dep}:x64-windows-static"
        $manifestFile = Join-Path $VcpkgInstallDir "info" "$triplet.list"
        if (Test-Path $manifestFile) { $installedCount++ }
    }

    if ($installedCount -eq $DepNames.Count) {
        Write-Host "  ==> vcpkg install succeeded ($installedCount/$($DepNames.Count) packages)."
        return $VcpkgInstallDir
    } elseif ($installedCount -gt 0) {
        Write-Host "  ==> vcpkg partial success ($installedCount/$($DepNames.Count) packages installed)."
        return $VcpkgInstallDir
    } else {
        Write-Host "  ==> vcpkg install failed — no packages installed (exit code $($proc.ExitCode))" -ForegroundColor Yellow
        return $null
    }
}

# --- VS BuildTools (nmake) ---
function Check-VsTools {
    $nmake = Get-Command "nmake.exe" -ErrorAction SilentlyContinue
    if ($nmake) { return "nmake at $($nmake.Source)" }

    $vswhere = Get-VsWherePath
    if ($vswhere) {
        $path = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
        if ($path) {
            $vcVars = Join-Path $path "VC\Auxiliary\Build\vcvarsall.bat"
            if (Test-Path $vcVars) {
                $script:VsInstallPath = $path
                return "VS at $path"
            }
        }
    }
    return $null
}
function Install-VsTools {
    Write-Host "    Visual Studio BuildTools is required for nmake/msbuild."
    Write-Host "    This is a system-level dependency that cannot be made project-local."

    if (Get-Command "winget" -ErrorAction SilentlyContinue) {
        Write-Host "    Attempting winget install of VS BuildTools..."
        Write-Host "    (This will open an installer window — please complete it manually if prompted)"
        $proc = Start-Process "winget" -ArgumentList "install","--id=Microsoft.VisualStudio.2022.BuildTools","--silent","--override","--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" -NoNewWindow -Wait -PassThru
        if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq -1978334960) {
            # Re-detect after install
            $vswhere = Get-VsWherePath
            if ($vswhere) {
                $path = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
                if ($path) { $script:VsInstallPath = $path }
            }
            return "VS BuildTools install initiated or already present"
        }
    }

    Write-Host ""
    Write-Host "    Please install VS BuildTools manually:" -ForegroundColor Yellow
    Write-Host "    1. Download from https://visualstudio.microsoft.com/downloads/" -ForegroundColor Yellow
    Write-Host "    2. Select 'C++ build tools' workload" -ForegroundColor Yellow
    Write-Host "    3. Re-run this script" -ForegroundColor Yellow
    Write-Host ""
    return $null
}

function Get-VsWherePath {
    $candidates = @(
        "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
        "C:\Program Files\Microsoft Visual Studio\Installer\vswhere.exe"
    )
    foreach ($p in $candidates) {
        if (Test-Path $p) { return $p }
    }
    return $null
}

# --- Build the local tool PATH (script-scoped only) ---

function Activate-LocalTools {
    # $script:ToolDirs is already populated by Install-* functions.
    # Deduplicate and build the PATH string.
    $uniqueDirs = $script:ToolDirs | Sort-Object -Unique
    $script:BuildEnvPath = ($uniqueDirs -join ";") + ";" + $env:Path
    Write-Host "  Tool dirs in build PATH:"
    foreach ($d in $uniqueDirs) {
        Write-Host "    - $d"
    }

    # If VS is installed, inject nmake/cl.exe into build PATH via vcvarsall
    if ($script:VsInstallPath) {
        $vcVarsAll = Join-Path $script:VsInstallPath "VC\Auxiliary\Build\vcvarsall.bat"
        if (Test-Path $vcVarsAll) {
            Write-Host "  Activating VS dev environment from $script:VsInstallPath..."
            $tempTxt = Join-Path $env:TEMP "vs_env_$pid.txt"
            $tempBat = Join-Path $env:TEMP "vs_env_$pid.bat"
            $batContent = "@echo off`ncall `"$vcVarsAll`" amd64 >nul 2>&1`nset > `"$tempTxt`""
            Set-Content -Path $tempBat -Value $batContent
            cmd.exe /c $tempBat
            Remove-Item $tempBat -Force -ErrorAction SilentlyContinue

            if (Test-Path $tempTxt) {
                foreach ($line in Get-Content $tempTxt) {
                    if ($line -match '^([^=]+)=(.*)') {
                        Set-Item -Path "Env:\$($Matches[1])" -Value $Matches[2] -ErrorAction SilentlyContinue
                    }
                }
                Remove-Item $tempTxt -Force -ErrorAction SilentlyContinue
            }

            # Rebuild BuildEnvPath with updated system PATH
            $script:BuildEnvPath = ($uniqueDirs -join ";") + ";" + $env:Path

            if (Get-Command "nmake.exe" -ErrorAction SilentlyContinue) {
                Write-Host "  nmake: OK"
            } else {
                Write-Host "  WARNING: nmake not found after VS env setup" -ForegroundColor Yellow
            }
        }
    }
}

function Run-With-LocalPath {
    param([string]$Cmd, [string[]]$Args)
    $allArgs = @($Cmd) + $Args
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Cmd
    $psi.Arguments = $Args -join " "
    $psi.EnvironmentVariables["PATH"] = $script:BuildEnvPath
    $psi.WorkingDirectory = Get-Location
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    $proc.WaitForExit()
    if ($proc.ExitCode -ne 0) {
        $err = $proc.StandardError.ReadToEnd()
        Write-Host $err -ForegroundColor Red
        throw "$Cmd failed with exit code $($proc.ExitCode)"
    }
    return $proc.StandardOutput.ReadToEnd()
}

# ============================================================
# Pre-built C deps from GitHub Releases
# ============================================================

$ReleaseTag = "deps-v1"  # matches the workflow release tag

function Download-Prebuilt-Deps {
    $Platform = if ($IsWindows) { "windows-x64" }
                elseif ($IsMacOS) { "macos-x64" }
                else { "linux-x64" }

    $assetName = "lua-deps-${Platform}.tar.gz"
    $markerFile = Join-Path $DepsDir ".prebuilt-${assetName}.installed"

    # Check if already installed
    if (Test-Path $markerFile) {
        Write-Host "  ==> Pre-built deps already installed ($assetName)."
        return $DepsDir
    }

    # Ensure we have curl
    if (-not (Get-Command "curl.exe" -ErrorAction SilentlyContinue)) {
        Write-Host "  ==> curl not found, cannot download pre-built deps."
        return $null
    }

    Write-Host "  ==> Downloading pre-built deps ($assetName) from GitHub Releases..."

    $apiUrl = "https://api.github.com/repos/$GitHubRepo/releases/tags/$ReleaseTag"
    try {
        $release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
    } catch {
        Write-Host "  ==> GitHub Release not found ($ReleaseTag). Will compile locally." -ForegroundColor Yellow
        return $null
    }

    $asset = $release.assets | Where-Object { $_.name -eq $assetName }
    if (-not $asset) {
        $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
        Write-Host "  ==> Pre-built asset '$assetName' not found in release. Available: $available" -ForegroundColor Yellow
        return $null
    }

    $archivePath = Join-Path $DepsDir "prebuilt.tar.gz"
    Write-Host "  ==> Downloading $($asset.browser_download_url)..."
    try {
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath -UseBasicParsing
    } catch {
        Write-Host "  ==> Download failed: $_" -ForegroundColor Red
        return $null
    }

    Write-Host "  ==> Extracting to $DepsDir..."
    tar -xzf $archivePath -C $DepsDir
    Remove-Item $archivePath -Force -ErrorAction SilentlyContinue

    # Create marker
    New-Item -ItemType File -Path $markerFile -Force | Out-Null

    Write-Host "  ==> Pre-built deps installed successfully."
    return $DepsDir
}

# ============================================================
# Step 0: Detect and install build tools
# ============================================================
Write-Host "`n=== Step 0: Detect Build Tools ==="

$TarPath = Detect-Tool "tar" { Check-Tar } { Install-Tar } "tar"
$CmakePath = Detect-Tool "cmake" { Check-Cmake } { Install-Cmake } "cmake"

# vcpkg is preferred for C dependency management
$VcpkgAvailable = Detect-Tool "vcpkg" { Check-Vcpkg } { Install-Vcpkg } "vcpkg"

if ($VcpkgAvailable) {
    Write-Host "`n  [INFO] vcpkg available — C deps will use vcpkg (preferred)."
} else {
    Write-Host "`n  [WARN] vcpkg not available. Will fall back to source compilation for C deps."
    Write-Host "         Consider installing vcpkg: iex (iwr -useb https://aka.ms/vcpkg-init.ps1)"

    # Still need Perl + VS BuildTools for source compilation fallback
    $PerlPath = Detect-Tool "perl" { Check-Perl } { Install-Perl } "perl"
    $vsResult = Detect-Tool "vs" { Check-VsTools } { Install-VsTools } "VS BuildTools (nmake)"
    if (-not $vsResult) {
        throw "VS BuildTools is required for source compilation. Install it and re-run this script."
    }
}

Write-Host "`n  Activating project-local tool paths..."
Activate-LocalTools

# ============================================================
# Parse lua_packages.txt
# ============================================================
$PackagesFile = Join-Path $ProjectDir "scripts\lua_packages.txt"
$Packages = @()
$Deps = @{}
$CurrentPkg = $null

if (Test-Path $PackagesFile) {
    foreach ($line in Get-Content $PackagesFile) {
        $line = $line.Trim()
        if (-not $line -or $line.StartsWith('#')) { continue }

        if ($line -match '^pkg\s+(\S+)(?:\s+(\S+))?') {
            $Packages += $Matches[1]
            $CurrentPkg = $Matches[1]
            if (-not $Deps.ContainsKey($CurrentPkg)) {
                $Deps[$CurrentPkg] = @()
            }
        } elseif ($line -match '^dep\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)') {
            $depName = $Matches[1]
            $os = $Matches[2]
            $method = $Matches[3]
            $url = $Matches[4]
            if ($os -eq 'windows' -or $os -eq 'any') {
                $Deps[$CurrentPkg] += @{
                    name = $depName
                    os = $os
                    method = $method
                    url = $url
                }
            }
        }
    }
    Write-Host "`n==> Packages from $PackagesFile"
    foreach ($pkg in $Packages) {
        $depList = $Deps[$pkg]
        $depStr = if ($depList -and $depList.Count -gt 0) { " [deps: $($depList.name -join ', ')]" } else { " [pure lua]" }
        Write-Host "    - $pkg$depStr"
    }
} else {
    throw "$PackagesFile not found"
}

# ============================================================
# Build helpers (use local tool PATH via $script:BuildEnvPath)
# ============================================================

function Run-Cmd {
    param([string[]]$Cmd)
    # Use cmd.exe /c for nmake/perl/cmake with local PATH
    $cmdLine = $Cmd -join " "
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "cmd.exe"
    $psi.Arguments = "/c `"$cmdLine`""
    $psi.EnvironmentVariables["PATH"] = $script:BuildEnvPath
    $psi.WorkingDirectory = Get-Location
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = [System.Diagnostics.Process]::Start($psi)

    # Stream output
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    $proc.WaitForExit()

    $stdout = $outTask.Result
    $stderr = $errTask.Result

    if ($stdout) { Write-Host $stdout.Trim() }
    if ($stderr -and $proc.ExitCode -ne 0) { Write-Host $stderr.Trim() -ForegroundColor Red }

    if ($proc.ExitCode -ne 0) {
        throw "Command failed (exit code $($proc.ExitCode)): $cmdLine"
    }
    return $stdout
}

function Build-OpenSSL {
    param([string]$Url, [string]$BuildDir)
    $InstallDir = Join-Path $DepsDir "openssl"
    if (Test-Path (Join-Path $InstallDir "lib\libssl.lib")) {
        Write-Host "  ==> OpenSSL already built at $InstallDir"
        return $InstallDir
    }

    Write-Host "  ==> Downloading OpenSSL..."
    Ensure-Dir $BuildDir
    $SrcDir = Download-Extract-TarGz -Url $Url -DestDir $BuildDir

    Write-Host "  ==> Building OpenSSL (perl Configure + nmake)..."
    Push-Location $SrcDir
    try {
        # Configure for MSVC x64
        Run-Cmd -Cmd @("perl", "Configure", "VC-WIN64A", "--prefix=$InstallDir", "--openssldir=$InstallDir\ssl", "no-tests", "no-shared")
        Run-Cmd -Cmd @("nmake")
        Run-Cmd -Cmd @("nmake", "install_sw")
    } finally {
        Pop-Location
    }

    Write-Host "  ==> OpenSSL installed at $InstallDir"
    return $InstallDir
}

function Build-Zlib {
    param([string]$Url, [string]$BuildDir)
    $InstallDir = Join-Path $DepsDir "zlib"
    if (Test-Path (Join-Path $InstallDir "lib\zdll.lib")) {
        Write-Host "  ==> Zlib already built at $InstallDir"
        return $InstallDir
    }

    Write-Host "  ==> Downloading Zlib..."
    Ensure-Dir $BuildDir
    $SrcDir = Download-Extract-TarGz -Url $Url -DestDir $BuildDir

    Write-Host "  ==> Building Zlib (cmake + nmake)..."
    Push-Location $SrcDir
    try {
        $BuildSub = Join-Path $SrcDir "build"
        Ensure-Dir $BuildSub
        Push-Location $BuildSub
        try {
            Run-Cmd -Cmd @("cmake", "..", "-GNMake Makefiles", "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=$InstallDir", "-DBUILD_SHARED_LIBS=ON")
            Run-Cmd -Cmd @("nmake")
            Run-Cmd -Cmd @("nmake", "install")
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }

    Write-Host "  ==> Zlib installed at $InstallDir"
    return $InstallDir
}

function Build-Pcre2 {
    param([string]$Url, [string]$BuildDir)
    $InstallDir = Join-Path $DepsDir "pcre2"
    if (Test-Path (Join-Path $InstallDir "lib\pcre2-8.lib")) {
        Write-Host "  ==> PCRE2 already built at $InstallDir"
        return $InstallDir
    }

    Write-Host "  ==> Downloading PCRE2..."
    Ensure-Dir $BuildDir
    $SrcDir = Download-Extract-TarGz -Url $Url -DestDir $BuildDir

    Write-Host "  ==> Building PCRE2 (cmake + nmake)..."
    Push-Location $SrcDir
    try {
        $BuildSub = Join-Path $SrcDir "build"
        Ensure-Dir $BuildSub
        Push-Location $BuildSub
        try {
            Run-Cmd -Cmd @("cmake", "..", "-GNMake Makefiles", "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=$InstallDir", "-DBUILD_SHARED_LIBS=OFF", "-DPCRE2_BUILD_PCRE2GREP=OFF", "-DPCRE2_SUPPORT_JIT=ON")
            Run-Cmd -Cmd @("nmake")
            Run-Cmd -Cmd @("nmake", "install")
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }

    Write-Host "  ==> PCRE2 installed at $InstallDir"
    return $InstallDir
}

function Build-LibYAML {
    param([string]$Url, [string]$BuildDir)
    $InstallDir = Join-Path $DepsDir "libyaml"
    if (Test-Path (Join-Path $InstallDir "lib\yaml.lib")) {
        Write-Host "  ==> LibYAML already built at $InstallDir"
        return $InstallDir
    }

    Write-Host "  ==> Downloading LibYAML..."
    Ensure-Dir $BuildDir
    $SrcDir = Download-Extract-TarGz -Url $Url -DestDir $BuildDir

    Write-Host "  ==> Building LibYAML (cmake + nmake)..."
    Push-Location $SrcDir
    try {
        $BuildSub = Join-Path $SrcDir "build"
        Ensure-Dir $BuildSub
        Push-Location $BuildSub
        try {
            Run-Cmd -Cmd @("cmake", "..", "-GNMake Makefiles", "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=$InstallDir", "-DBUILD_SHARED_LIBS=OFF")
            Run-Cmd -Cmd @("nmake")
            Run-Cmd -Cmd @("nmake", "install")
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }

    Write-Host "  ==> LibYAML installed at $InstallDir"
    return $InstallDir
}

# ============================================================
# Step 1: Build LuaJIT SDK from cargo target
# ============================================================
Write-Host "`n=== Step 1: LuaJIT SDK ==="

$LuaJITExe = Join-Path $LuaJITDir "luajit.exe"
$LuaJITDLL = Join-Path $LuaJITDir "lua51.dll"
$LuaIncludeDir = Join-Path $LuaJITDir "include"

if ((Test-Path $LuaJITDLL) -and (Test-Path $LuaIncludeDir)) {
    Write-Host "==> LuaJIT SDK already exists at $LuaJITDir (reusing)"
} else {
    $MluaOutDirs = Get-ChildItem -Path "$ProjectDir\target" -Recurse -Directory -Filter "luajit-build" -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "mlua-sys" } |
        Sort-Object LastWriteTime -Descending

    $BuildSrcDir = $null
    $LibFile = $null
    foreach ($d in $MluaOutDirs) {
        $SrcDir = Join-Path $d.FullName "src"
        $LibFile = Join-Path $d.FullName "lib\lua51.lib"
        if (-not (Test-Path $LibFile)) {
            $LibFile = Join-Path $SrcDir "lua51.lib"
        }
        if ((Test-Path (Join-Path $SrcDir "lua.h")) -and (Test-Path $LibFile)) {
            $BuildSrcDir = $SrcDir
            break
        }
    }

    if (-not $BuildSrcDir) {
        throw "LuaJIT build artifacts not found in cargo target. Run 'cargo build' first."
    }

    Write-Host "==> Found LuaJIT source at $BuildSrcDir"

    $DllFound = $false
    if (Test-Path (Join-Path $BuildSrcDir "lua51.dll")) {
        $DllFound = $true
        Write-Host "==> Found already-built DLL in cargo target"
    } else {
        Write-Host "==> Building LuaJIT DLL (msvcbuild.bat)..."
        Push-Location $BuildSrcDir
        try {
            $psi = New-Object System.Diagnostics.ProcessStartInfo
            $psi.FileName = "cmd.exe"
            $psi.Arguments = "/c `"set PATH=.;$($script:BuildEnvPath);%PATH% && msvcbuild.bat`""
            $psi.EnvironmentVariables["PATH"] = $script:BuildEnvPath
            $psi.WorkingDirectory = $BuildSrcDir
            $psi.RedirectStandardOutput = $true
            $psi.RedirectStandardError = $true
            $psi.UseShellExecute = $false
            $psi.CreateNoWindow = $false  # Show window for msvcbuild.bat
            $proc = [System.Diagnostics.Process]::Start($psi)
            $proc.WaitForExit()
            if (Test-Path (Join-Path $BuildSrcDir "lua51.dll")) {
                $DllFound = $true
            }
        } finally {
            Pop-Location
        }
    }

    if (-not $DllFound) {
        throw "Failed to build LuaJIT DLL at $BuildSrcDir"
    }

    Write-Host "==> Installing LuaJIT SDK to $LuaJITDir"
    Ensure-Dir $LuaJITDir
    Ensure-Dir $LuaIncludeDir

    Copy-Item (Join-Path $BuildSrcDir "lua51.dll") $LuaJITDir -Force
    if ($LibFile -and (Test-Path $LibFile)) {
        Copy-Item $LibFile $LuaJITDir -Force
    }
    $ExeSrc = Join-Path $BuildSrcDir "luajit.exe"
    if (Test-Path $ExeSrc) {
        Copy-Item $ExeSrc $LuaJITDir -Force
    }
    if (Test-Path (Join-Path $BuildSrcDir "lua.h")) {
        Get-ChildItem $BuildSrcDir -Filter "*.h" | ForEach-Object {
            Copy-Item $_.FullName $LuaIncludeDir -Force
        }
    }
}

if (-not (Test-Path $LuaJITDLL)) {
    throw "LuaJIT SDK setup failed: lua51.dll not found at $LuaJITDir"
}

Write-Host "==> LuaJIT SDK: $LuaJITDir"

# ============================================================
# Step 2: Install luarocks
# ============================================================
Write-Host "`n=== Step 2: luarocks ==="

$LuarocksExe = Join-Path $LuarocksDir "luarocks.exe"

if (-not (Test-Path $LuarocksExe)) {
    Write-Host "==> Downloading luarocks..."
    $BuildTemp = Join-Path $ProjectDir "target\luarocks_build"
    Ensure-Dir $BuildTemp

    $LuarocksVersion = "3.12.1"
    $LuarocksUrl = "https://github.com/luarocks/luarocks/releases/download/v$LuarocksVersion/luarocks-$LuarocksVersion-windows-64.zip"
    $Archive = Join-Path $BuildTemp "luarocks.zip"

    if (-not (Test-Path $Archive)) {
        Invoke-WebRequest -Uri $LuarocksUrl -OutFile $Archive -UseBasicParsing
    }

    Write-Host "==> Extracting luarocks..."
    Expand-Archive $Archive $BuildTemp -Force
    $LuarocksSrc = (Get-ChildItem $BuildTemp -Directory | Where-Object { $_.Name -like "luarocks*" } | Select-Object -First 1).FullName

    if ($LuarocksSrc) {
        Ensure-Dir $LuarocksDir
        Copy-Item -Recurse "$LuarocksSrc\*" $LuarocksDir -Force
    } else {
        throw "luarocks source not found in extracted archive"
    }

    Remove-Item $Archive -Force -ErrorAction SilentlyContinue
    Remove-Item $BuildTemp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "==> Creating luarocks config..."
Ensure-Dir $LuaPackages

$ConfigContent = @"
rocks_trees = {
    { name = [[project]], root = [[${LuaPackages}]] },
}
lua_interpreter = [[luajit.exe]]
lua_dir = [[${LuaJITDir}]]
variables = {
    LUA_INCDIR = [[${LuaIncludeDir}]],
    LUA_LIBDIR = [[${LuaJITDir}]],
    MSVCRT = [[msvcrt]],
}
"@

Set-Content -Path (Join-Path $LuarocksDir "config.lua") -Value $ConfigContent -Encoding UTF8

# ============================================================
# Step 3: C dependencies — pre-built → vcpkg → source compile
# ============================================================
Write-Host "`n=== Step 3: C Dependencies ==="

Ensure-Dir $DepsDir
$DepPaths = @{}

# Collect unique dep names
$AllDepNames = @()
foreach ($pkg in $Packages) {
    $pkgDeps = $Deps[$pkg]
    if (-not $pkgDeps -or $pkgDeps.Count -eq 0) { continue }
    foreach ($dep in $pkgDeps) {
        if ($dep.name -notin $AllDepNames) {
            $AllDepNames += $dep.name
        }
    }
}

if ($AllDepNames.Count -eq 0) {
    Write-Host "  ==> No C dependencies needed."
} else {
    Write-Host "  ==> Required C deps: $($AllDepNames -join ', ')"

    # --- Priority 1: Pre-built from GitHub Releases ---
    if ($GitHubRepo -ne "{{GITHUB_USER}}/{{GITHUB_REPO}}") {
        $PrebuiltResult = Download-Prebuilt-Deps
        if ($PrebuiltResult) {
            # Pre-built deps are laid out as: deps/openssl/, deps/zlib/, deps/pcre2/, deps/libyaml/
            foreach ($depName in $AllDepNames) {
                $depDir = Join-Path $DepsDir $depName
                if (Test-Path $depDir) {
                    $DepPaths[$depName] = $depDir
                }
            }
            Write-Host "  ==> Using pre-built deps. No local compilation needed."
        }
    }

    # --- Priority 2: vcpkg compile (if pre-built not available) ---
    $stillMissing = $AllDepNames | Where-Object { -not $DepPaths.ContainsKey($_) }
    if ($stillMissing.Count -gt 0 -and $VcpkgAvailable) {
        $VcpkgInstallDir = Install-Deps-With-Vcpkg -DepNames $stillMissing
        if ($VcpkgInstallDir) {
            $tripletDir = Join-Path $VcpkgInstallDir "x64-windows-static"
            if (Test-Path $tripletDir) {
                foreach ($depName in $stillMissing) {
                    $DepPaths[$depName] = $tripletDir
                }
            }
        } else {
            Write-Host "  ==> vcpkg install failed, falling back to source compilation..." -ForegroundColor Yellow
            $VcpkgAvailable = $false
        }
    }

    # --- Priority 3: Source compile (if vcpkg not available or failed) ---
    $stillMissing = $AllDepNames | Where-Object { -not $DepPaths.ContainsKey($_) }
    if ($stillMissing.Count -gt 0) {
        $PerlPath = Detect-Tool "perl" { Check-Perl } { Install-Perl } "perl"
        $vsResult = Detect-Tool "vs" { Check-VsTools } { Install-VsTools } "VS BuildTools (nmake)"
        if (-not $vsResult) {
            throw "VS BuildTools is required for source compilation. Install it and re-run this script."
        }
        Activate-LocalTools

        foreach ($depName in $stillMissing) {
            $BuildDir = Join-Path $DepsDir "build\$depName"
            # Find method and URL from lua_packages.txt
            $method = $null; $url = $null
            foreach ($pkg in $Packages) {
                foreach ($dep in $Deps[$pkg]) {
                    if ($dep.name -eq $depName) {
                        $method = $dep.method; $url = $dep.url; break
                    }
                }
                if ($method) { break }
            }

            Write-Host "==> Dependency: $depName ($method)"
            switch ($method) {
                "vcpkg" {
                    $InstallDir = switch ($depName) {
                        "openssl"  { Build-OpenSSL  -Url $url -BuildDir $BuildDir }
                        "pcre2"    { Build-Pcre2    -Url $url -BuildDir $BuildDir }
                        default    { throw "Unknown vcpkg dep: $depName" }
                    }
                    $DepPaths[$depName] = $InstallDir
                }
                "bundled" {
                    $InstallDir = switch ($depName) {
                        "zlib"    { Build-Zlib    -Url $url -BuildDir $BuildDir }
                        "libyaml" { Build-LibYAML -Url $url -BuildDir $BuildDir }
                        default   { throw "Unknown bundled dep: $depName" }
                    }
                    $DepPaths[$depName] = $InstallDir
                }
                "none" {
                    Write-Host "  ==> Pure Lua, no build needed"
                    $DepPaths[$depName] = ""
                }
                default {
                    Write-Host "  ==> Unknown method '$method' for $depName, skipping auto-build"
                }
            }
        }
    }
}

# ============================================================
# Step 4: Install Lua packages
# ============================================================
Write-Host "`n=== Step 4: Installing Lua packages ==="

# Build script-local PATH with project-local tools + deps
$pkgPath = "$LuaJITDir"
foreach ($dir in ($script:ToolDirs | Sort-Object -Unique)) {
    if ($dir -and (Test-Path $dir)) { $pkgPath = "$dir;$pkgPath" }
}
if ($DepPaths.ContainsKey("openssl") -and $DepPaths["openssl"]) {
    $pkgPath = "$(Join-Path $DepPaths['openssl'] 'bin');$pkgPath"
}
if ($DepPaths.ContainsKey("zlib") -and $DepPaths["zlib"]) {
    $pkgPath = "$(Join-Path $DepPaths['zlib'] 'bin');$pkgPath"
}
if ($DepPaths.ContainsKey("pcre2") -and $DepPaths["pcre2"]) {
    $pkgPath = "$(Join-Path $DepPaths['pcre2'] 'bin');$pkgPath"
}
# Original system PATH at end
$pkgPath = "$pkgPath;$env:Path"

Push-Location $LuarocksDir

$InstallResults = @{}

foreach ($pkg in $Packages) {
    Write-Host "==> Installing $pkg..."
    $ExtraArgs = @()

    $pkgDeps = $Deps[$pkg]
    if ($pkgDeps) {
        foreach ($dep in $pkgDeps) {
            $depName = $dep.name
            if ($DepPaths.ContainsKey($depName) -and $DepPaths[$depName]) {
                $depDir = $DepPaths[$depName]
                $ExtraArgs += "--with-$($depName)-libdir=$(Join-Path $depDir 'lib')"
                $ExtraArgs += "--with-$($depName)-incdir=$(Join-Path $depDir 'include')"
            }
        }
    }

    # Run luarocks with project-local PATH
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $LuarocksExe
    $psi.Arguments = "install $pkg --tree=`"$LuaPackages`" --lua-dir=`"$LuaJITDir`" $($ExtraArgs -join ' ')"
    $psi.EnvironmentVariables["PATH"] = $pkgPath
    $psi.WorkingDirectory = $LuarocksDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $false  # Show luarocks output

    $proc = [System.Diagnostics.Process]::Start($psi)
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    $proc.WaitForExit()

    $stdout = $outTask.Result
    if ($stdout) { Write-Host $stdout.Trim() }
    $stderr = $errTask.Result
    if ($stderr -and $proc.ExitCode -ne 0) { Write-Host $stderr.Trim() -ForegroundColor Red }

    $InstallResults[$pkg] = ($proc.ExitCode -eq 0)
    if ($proc.ExitCode -ne 0) {
        Write-Host "==> WARNING: Failed to install $pkg (exit code $($proc.ExitCode))" -ForegroundColor Yellow
    }
}

Pop-Location

Write-Host "`n==> Install results:"
foreach ($pkg in $InstallResults.Keys | Sort-Object) {
    $status = if ($InstallResults[$pkg]) { "OK" } else { "FAILED" }
    $color = if ($InstallResults[$pkg]) { "Green" } else { "Yellow" }
    Write-Host "  $pkg : $status" -ForegroundColor $color
}

Write-Host "`n==> Installed files:"
if (Test-Path (Join-Path $LuaPackages "lib\lua\5.1")) {
    Get-ChildItem (Join-Path $LuaPackages "lib\lua\5.1") -Recurse -File | ForEach-Object { Write-Host "  $($_.FullName)" }
}
if (Test-Path (Join-Path $LuaPackages "share\lua\5.1")) {
    Get-ChildItem (Join-Path $LuaPackages "share\lua\5.1") -Recurse -File | ForEach-Object { Write-Host "  $($_.FullName)" }
}

Write-Host "`n==> Done."
Write-Host "    LuaJIT SDK: $LuaJITDir"
Write-Host "    Deps:       $DepsDir"
Write-Host "    Packages:   $LuaPackages"
Write-Host "    Tools:      $ToolsDir (project-local)"
