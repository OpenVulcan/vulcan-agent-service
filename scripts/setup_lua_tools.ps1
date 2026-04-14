# setup_lua_tools.ps1
# One-time setup: download LuaJIT, install luarocks, configure project-local package directory.
# Run from project root: powershell -ExecutionPolicy Bypass -File scripts\setup_lua_tools.ps1

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# ============================================================
# Configuration
# ============================================================
$LUAJIT_VERSION = "2.1.1736781462"
$LUAJIT_URL = "https://github.com/LuaJIT/LuaJIT/archive/refs/tags/v2.1.1736781462.tar.gz"

# Use mlua's vendored build output if available, otherwise download independent LuaJIT.
# The mlua vendored LuaJIT is statically linked, so we need a separate DLL for luarocks.
$ThirdPartyDir = Join-Path $ProjectDir "third_party"
$LuaJITDir = Join-Path $ThirdPartyDir "luajit"
$LuaBin = Join-Path $LuaJITDir "luajit.exe"
$LuaDLL = Join-Path $LuaJITDir "lua51.dll"

$LuarocksDir = Join-Path $ThirdPartyDir "luarocks"
$LuarocksExe = Join-Path $LuarocksDir "luarocks.bat"

# Where luarocks installs packages (project-local)
$LuaPackagesDir = Join-Path $ThirdPartyDir "lua_packages"

# ============================================================
# Helper functions
# ============================================================
function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
        Write-Host "==> Created $Path"
    }
}

function Download-File {
    param([string]$Url, [string]$Dest)
    Write-Host "==> Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
    Write-Host "==> Downloaded to $Dest"
}

# ============================================================
# Step 1: Get or build LuaJIT
# ============================================================
Write-Host "`n=== Step 1: Setting up LuaJIT ==="

# Try to find mlua's vendored LuaJIT build output first
$VendoredOut = Get-ChildItem -Path "$env:USERPROFILE\.cargo" -Recurse -Directory -Filter "luajit-build" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1

if ($VendoredOut -and (Test-Path $VendoredOut.FullName)) {
    # Look for compiled artifacts
    $VendoredSrc = Join-Path $VendoredOut.FullName "src"
    if (Test-Path (Join-Path $VendoredSrc "lua51.dll")) {
        Write-Host "==> Found vendored LuaJIT DLL at $VendoredSrc"
        Ensure-Dir $LuaJITDir
        Copy-Item (Join-Path $VendoredSrc "lua51.dll") $LuaJITDir -Force
        Copy-Item (Join-Path $VendoredSrc "luajit.exe") $LuaJITDir -Force
        Copy-Item (Join-Path $VendoredSrc "lua51.lib") $LuaJITDir -Force
        # Copy include files
        $IncludeDir = Join-Path $LuaJITDir "include"
        Ensure-Dir $IncludeDir
        Get-ChildItem $VendoredSrc -Filter "*.h" | ForEach-Object {
            Copy-Item $_.FullName $IncludeDir -Force
        }
        Write-Host "==> Copied vendored LuaJIT files to $LuaJITDir"
    }
}

# If no vendored DLL found, download/build independent LuaJIT
if (-not (Test-Path $LuaDLL)) {
    Write-Host "==> Vendored LuaJIT DLL not found, building from source..."
    $BuildTemp = Join-Path $ProjectDir "target\luajit_build"
    Ensure-Dir $BuildTemp

    $Archive = Join-Path $BuildTemp "luajit.tar.gz"
    if (-not (Test-Path $Archive)) {
        Download-File $LUAJIT_URL $Archive
    }

    # Extract
    Write-Host "==> Extracting LuaJIT..."
    tar -xzf $Archive -C $BuildTemp
    $ExtractedDir = Get-ChildItem $BuildTemp -Directory | Where-Object { $_.Name -like "LuaJIT-*" } | Select-Object -First 1
    if (-not $ExtractedDir) {
        throw "LuaJIT source not found after extraction"
    }
    $LuaJITSrc = $ExtractedDir.FullName

    # Build with MSVC
    Write-Host "==> Building LuaJIT with MSVC..."
    Push-Location $LuaJITSrc\src

    # Find Visual Studio environment
    $VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $VsWhere) {
        $InstallPath = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($InstallPath) {
            $VsDevCmd = Join-Path $InstallPath "Common7\Tools\VsDevCmd.bat"
            if (Test-Path $VsDevCmd) {
                cmd.exe /c "`"$VsDevCmd`" -arch=x64 && cd /d $LuaJITSrc\src && msvcbuild.bat static"
            } else {
                cmd.exe /c "call `"$VsDevCmd`" && msvcbuild.bat static"
            }
        } else {
            cmd.exe /c "msvcbuild.bat static"
        }
    } else {
        cmd.exe /c "msvcbuild.bat static"
    }

    Pop-Location

    # Check build result
    $BuiltDLL = Join-Path $LuaJITSrc "src\lua51.dll"
    $BuiltEXE = Join-Path $LuaJITSrc "src\luajit.exe"

    if (Test-Path $BuiltDLL) {
        Ensure-Dir $LuaJITDir
        Copy-Item $BuiltDLL $LuaJITDir -Force
        if (Test-Path $BuiltEXE) {
            Copy-Item $BuiltEXE $LuaJITDir -Force
        }
        $IncludeDir = Join-Path $LuaJITDir "include"
        Ensure-Dir $IncludeDir
        Get-ChildItem $LuaJITSrc\src -Filter "*.h" | ForEach-Object {
            Copy-Item $_.FullName $IncludeDir -Force
        }
        Write-Host "==> LuaJIT built and installed to $LuaJITDir"
    } else {
        throw "LuaJIT build failed: lua51.dll not found"
    }

    # Cleanup
    Remove-Item $BuildTemp -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not (Test-Path $LuaDLL)) {
    throw "LuaJIT setup failed: lua51.dll not found at $LuaDLL"
}

Write-Host "==> LuaJIT ready at $LuaJITDir"

# ============================================================
# Step 2: Install luarocks
# ============================================================
Write-Host "`n=== Step 2: Installing luarocks ==="

Ensure-Dir $LuaPackagesDir
$LuarocksInstallDir = Join-Path $LuaPackagesDir "luarocks_tree"
Ensure-Dir $LuarocksInstallDir
$LuarocksCache = Join-Path $LuaPackagesDir "cache"
Ensure-Dir $LuarocksCache

if (-not (Test-Path $LuarocksExe)) {
    Write-Host "==> Downloading luarocks..."
    $LuarocksZip = Join-Path $ProjectDir "target\luarocks-win.zip"
    $LuarocksUrl = "https://github.com/luarocks/luarocks/releases/download/v3.12.1/luarocks-3.12.1-windows.zip"

    if (-not (Test-Path $LuarocksZip)) {
        Download-File $LuarocksUrl $LuarocksZip
    }

    Write-Host "==> Extracting luarocks..."
    $ExtractDir = Join-Path $ProjectDir "target\luarocks_extract"
    Ensure-Dir $ExtractDir
    Expand-Archive $LuarocksZip $ExtractDir -Force

    # Find the extracted luarocks directory
    $LuarocksSrcDir = Get-ChildItem $ExtractDir -Directory | Where-Object { $_.Name -like "luarocks*" } | Select-Object -First 1
    if (-not $LuarocksSrcDir) {
        # Try top-level files
        $BatFiles = Get-ChildItem $ExtractDir -Filter "luarocks.bat" -Recurse | Select-Object -First 1
        if ($BatFiles) {
            $LuarocksSrcDir = $BatFiles.Directory
        }
    }

    if ($LuarocksSrcDir) {
        Ensure-Dir $LuarocksDir
        Copy-Item -Recurse "$($LuarocksSrcDir.FullName)\*" $LuarocksDir -Force
        Write-Host "==> luarocks extracted to $LuarocksDir"
    } else {
        throw "Could not find luarocks.bat in extracted archive"
    }

    Remove-Item $LuarocksZip -Force -ErrorAction SilentlyContinue
    Remove-Item $ExtractDir -Recurse -Force -ErrorAction SilentlyContinue
}

# Create luarocks configuration
Write-Host "==> Creating luarocks config..."
$LuarocksConfig = @"
rocks_trees = {
    { name = [[user]], root = home .. [[/.luarocks]] },
    { name = [[project]], root = [[${LuaPackagesDir}]] },
}
lua_interpreter = [[luajit.exe]]
lua_dir = [[${LuaJITDir}]]
home = [[${env:USERPROFILE}]]
variables = {
    LUA = [[${LuaBin}]],
    LUA_INCDIR = [[${LuaJITDir}\include]],
    LUA_LIBDIR = [[${LuaJITDir}]],
    MSVCRT = [[msvcrt]],
}
"@

$configFile = Join-Path $LuarocksDir "config.lua"
Set-Content -Path $configFile -Value $LuarocksConfig -Encoding UTF8
Write-Host "==> Config written to $configFile"

# Bootstrap luarocks if not already done
if (-not (Test-Path (Join-Path $LuarocksInstallDir "bin\luarocks.bat"))) {
    Write-Host "==> Bootstrapping luarocks..."
    Push-Location $LuarocksDir
    $BootstrapBat = Get-ChildItem . -Filter "install.bat" | Select-Object -First 1
    if ($BootstrapBat) {
        $BootstrapArgs = @("/LUA", $LuaJITDir, "/P", $LuaPackagesDir, "/TREE", $LuarocksInstallDir, "/NOADMIN")
        & cmd.exe /c ($BootstrapBat.Name + " " + ($BootstrapArgs -join " "))
    } else {
        # Try self-bootstrap with the batch file
        if (Test-Path $LuarocksExe) {
            $env:Path = "$LuaJITDir;$env:Path"
            & $LuarocksExe config
        }
    }
    Pop-Location
}

# Add luarocks to PATH hint
Write-Host "`n=== Setup Complete ==="
Write-Host "LuaJIT:        $LuaJITDir"
Write-Host "luarocks:      $LuarocksDir"
Write-Host "Packages dir:  $LuaPackagesDir"
Write-Host ""
Write-Host "To install packages:"
Write-Host "  `$env:Path = `"$LuaJITDir`";`"`$env:Path`" ; & `"$LuarocksExe`" install <package>"
Write-Host ""
Write-Host "Or use the batch wrapper:"
Write-Host "  .\scripts\luarocks.bat install <package>"
