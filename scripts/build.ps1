# Build script for vulcan-agent-service (PowerShell)
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

$BinName = "vulcan-agent-service"

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
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed with exit code $LASTEXITCODE; existing binaries were not copied."
    exit $LASTEXITCODE
}

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
$LuaRuntimeOut = "$BaseOutDir\lua_runtime"
$LibsOut = "$LuaRuntimeOut\libs"
$PkgOut = "$LuaRuntimeOut\lua_packages"
$ConfigOut = "$BaseOutDir\configs"
$ResourcesOut = "$LuaRuntimeOut\resources"
$LicensesOut = "$LuaRuntimeOut\licenses"
$DependenciesOut = "$LuaRuntimeOut\dependencies"
$LogsOut = "$BaseOutDir\logs"
$LuaSkillsRuntimeRoot = Join-Path $ProjectDir "third_party\luaskills_runtime"
$ManagedRuntimeDistributionRoot = Join-Path $ProjectDir "third_party\luaskills_managed_runtimes"
$ManagedRuntimeLayoutCheckScript = Join-Path $ProjectDir "scripts\debug-tools\managed_runtime_layout_check.py"

function Copy-DirectoryContents {
    <#
    .SYNOPSIS
    Copy direct directory contents without deleting existing destination data.
    复制目录直属内容，并且不删除目标目录中已有的数据。

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
        return $false
    }

    if (-not (Test-Path -LiteralPath $Destination)) {
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    }
    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
    return $true
}

# Ensure output directories exist without deleting user-owned output data.
# 确保输出目录存在，但不删除用户拥有的 output 数据。
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
foreach ($dir in @($LuaRuntimeOut, $ConfigOut, $ResourcesOut, $LogsOut)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir"

# Copy official LuaSkills runtime package exports into output/lua_runtime/.
# 将官方 LuaSkills 运行时包导出内容复制到 output/lua_runtime/。
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Existing output files are preserved so a build cannot erase user-managed runtime data.
# 保留已有 output 文件，确保构建不会擦除用户管理的运行时数据。
$ResolvedLuaSkillsRuntimeRoot = $null
try {
    $ResolvedLuaSkillsRuntimeRoot = (Resolve-Path -LiteralPath $LuaSkillsRuntimeRoot -ErrorAction Stop).Path
} catch {
    $ResolvedLuaSkillsRuntimeRoot = $null
}
if (-not [string]::IsNullOrWhiteSpace($ResolvedLuaSkillsRuntimeRoot)) {
    foreach ($RuntimeDirName in @("lua_packages", "libs", "resources", "licenses")) {
        $RuntimeSource = Join-Path $ResolvedLuaSkillsRuntimeRoot $RuntimeDirName
        if ($RuntimeDirName -eq "lua_packages") {
            $RuntimeDestination = $PkgOut
        } elseif ($RuntimeDirName -eq "libs") {
            $RuntimeDestination = $LibsOut
        } elseif ($RuntimeDirName -eq "resources") {
            $RuntimeDestination = $ResourcesOut
        } elseif ($RuntimeDirName -eq "licenses") {
            $RuntimeDestination = $LicensesOut
        } else {
            throw "Unsupported LuaSkills runtime directory: $RuntimeDirName"
        }

        if (Copy-DirectoryContents -Source $RuntimeSource -Destination $RuntimeDestination) {
            Write-Host "==> LuaSkills runtime $RuntimeDirName copied to $RuntimeDestination"
        } else {
            Write-Host "==> LuaSkills runtime package has no $RuntimeDirName directory"
        }
    }
}
if ([string]::IsNullOrWhiteSpace($ResolvedLuaSkillsRuntimeRoot)) {
    Write-Host "==> No third_party/luaskills_runtime found (run make deps first)"
}

# Copy fetched managed Python and Node distributions into the fixed LuaSkills dependency root.
# 将已拉取的受管 Python 与 Node 发行包复制到固定 LuaSkills 依赖根。
if (Copy-DirectoryContents -Source $ManagedRuntimeDistributionRoot -Destination (Join-Path $DependenciesOut "runtimes")) {
    Write-Host "==> Managed Python/Node distributions copied to $DependenciesOut\runtimes"
    # PythonCommand validates the final copied layout rather than trusting the source cache alone.
    # PythonCommand 校验最终复制后的布局，而不是只信任源缓存。
    $PythonCommand = Get-Command "python" -ErrorAction SilentlyContinue
    if ($PythonCommand) {
        & $PythonCommand.Source $ManagedRuntimeLayoutCheckScript $LuaRuntimeOut
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
    elseif ($CargoProfile -eq "release") {
        throw "python is required to validate managed runtimes during release packaging"
    }
    else {
        Write-Warning "python is unavailable; debug packaging skipped the managed runtime layout validator"
    }
} else {
    if ($CargoProfile -eq "release") {
        throw "Managed Python/Node distributions are required for release packaging; run make deps managed first"
    }
    Write-Host "==> No managed Python/Node distributions found (run make deps managed first)"
}

# Copy repository config templates to output/configs without overwriting installed configuration.
# 将仓库配置模板复制到 output/configs，且不覆盖已安装配置。
if (Test-Path "configs") {
    if (-not (Test-Path $ConfigOut)) { New-Item -ItemType Directory -Path $ConfigOut -Force | Out-Null }
    Get-ChildItem -File -LiteralPath "configs" | ForEach-Object {
        # ConfigDestination identifies one output config file while preserving an installed file.
        # ConfigDestination 表示一个输出配置文件，并保留已经安装的文件。
        $ConfigDestination = Join-Path $ConfigOut $_.Name
        if (-not (Test-Path -LiteralPath $ConfigDestination)) {
            Copy-Item -Force -LiteralPath $_.FullName -Destination $ConfigDestination
        }
    }
    Write-Host "==> Config templates copied to $ConfigOut (existing files preserved)"
} else {
    Write-Host "==> No configs directory found"
}

# Copy repository shared resources to output/lua_runtime/resources/.
# 将仓库共享资源复制到 output/lua_runtime/resources/。
if (Test-Path "resources") {
    Copy-Item -Force -Recurse "resources\*" $ResourcesOut
    Write-Host "==> Shared resources copied to $ResourcesOut"
} else {
    Write-Host "==> No resources directory found"
}

# Prepare output/lua_runtime/bin as the fixed host-provided tool and controller directory.
# 将 output/lua_runtime/bin 准备为宿主提供工具与控制器的固定目录。
$HostToolsOut = Join-Path $LuaRuntimeOut "bin"
if (-not (Test-Path $HostToolsOut)) { New-Item -ItemType Directory -Path $HostToolsOut -Force | Out-Null }
Write-Host "==> Host tool output directory prepared at $HostToolsOut"

# Copy the host-installed vldb-controller executable to output/lua_runtime/bin/ when prepared.
# 准备完成后，将宿主安装的 vldb-controller 可执行文件复制到 output/lua_runtime/bin/。
$ControllerOut = Join-Path $LuaRuntimeOut "bin"
if (-not (Test-Path $ControllerOut)) { New-Item -ItemType Directory -Path $ControllerOut -Force | Out-Null }
$ControllerBinaryName = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    "vldb-controller.exe"
} else {
    "vldb-controller"
}
$ControllerBinarySource = Join-Path $ProjectDir "third_party\vldb_controller\bin\$ControllerBinaryName"
if (Test-Path $ControllerBinarySource) {
    if (-not (Get-Item -LiteralPath $ControllerBinarySource).PSIsContainer) {
        try {
            Copy-Item -Force $ControllerBinarySource "$ControllerOut\$ControllerBinaryName" -ErrorAction Stop
            Write-Host "==> vldb-controller synced to $ControllerOut"
        } catch {
            Write-Warning "vldb-controller is currently running or locked; keeping the existing output binary and continuing."
        }
    } else {
        Write-Error "vldb-controller source path is not a file: $ControllerBinarySource"
        exit 1
    }
} else {
    Write-Host "==> No third_party/vldb_controller/bin/$ControllerBinaryName found"
}

Write-Host "==> Done. Binary: $OutDir\$BinName.exe"
