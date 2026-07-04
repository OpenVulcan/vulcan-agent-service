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
$LibsOut = "$BaseOutDir\libs"
$SkillsOut = "$BaseOutDir\skills"
$PkgOut = "$BaseOutDir\lua_packages"
$ConfigOut = "$BaseOutDir\configs"
$ResourcesOut = "$BaseOutDir\resources"
$LicensesOut = "$BaseOutDir\licenses"
$DependenciesOut = "$BaseOutDir\dependencies"
$DatabasesOut = "$BaseOutDir\databases"
$StateOut = "$BaseOutDir\state"
$TempOut = "$BaseOutDir\temp"
$LogsOut = "$BaseOutDir\logs"
$LuaSkillsRuntimeRoot = Join-Path $ProjectDir "third_party\luaskills_runtime"

function Reset-DirectoryContents {
    <#
    .SYNOPSIS
    Ensure one directory exists and remove stale contents before a structured sync.
    确保目录存在，并在结构化同步前清理旧内容。

    .PARAMETER Path
    Directory path to reset.
    需要重置的目录路径。
    #>
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
        return
    }

    Get-ChildItem -Force -LiteralPath $Path -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item -LiteralPath $_.FullName -Recurse -Force
    }
}

function Copy-DirectoryContents {
    <#
    .SYNOPSIS
    Copy direct directory contents into a destination while preserving package layout.
    将目录直属内容复制到目标目录，并保持包布局。

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

    Reset-DirectoryContents -Path $Destination
    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
    return $true
}

function Enable-OutputModelConfigForLocalTesting {
    <#
    .SYNOPSIS
    Enable model capabilities only in the built output runtime config for local smoke testing.
    仅在构建后的输出运行配置中启用模型能力，方便本地冒烟测试。

    .PARAMETER ConfigDirectory
    Output config directory that may contain model_config.yaml.
    可能包含 model_config.yaml 的输出配置目录。
    #>
    param([string]$ConfigDirectory)

    $ModelConfigOut = Join-Path $ConfigDirectory "model_config.yaml"
    if (-not (Test-Path -LiteralPath $ModelConfigOut)) {
        return
    }

    $Content = Get-Content -LiteralPath $ModelConfigOut -Raw
    $Content = $Content -replace '(?m)^  enabled:\s*false\s*$', '  enabled: true'
    $Content = $Content -replace '(?m)^    enabled:\s*false\s*$', '    enabled: true'
    Set-Content -LiteralPath $ModelConfigOut -Value $Content -Encoding UTF8
    Write-Host "==> Output model_config.yaml enabled for local model smoke tests"
}

# Ensure output directories exist
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
foreach ($dir in @($LibsOut, $SkillsOut, $PkgOut, $ConfigOut, $ResourcesOut, $LicensesOut, $DependenciesOut, $DatabasesOut, $StateOut, $TempOut, $LogsOut)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}
foreach ($dir in @(
    "$DependenciesOut\\shared\\tools",
    "$DependenciesOut\\shared\\lua",
    "$DependenciesOut\\shared\\ffi",
    "$DependenciesOut\\skill",
    "$DatabasesOut\\sqlite",
    "$DatabasesOut\\lancedb",
    "$StateOut\\skills"
)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir"

# Sync official LuaSkills runtime package exports to output/.
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Cross-platform validation is intentionally omitted because forks may replace lua_packages/runtime payloads with custom layouts.
# 这里有意不做跨平台校验，因为 fork 方可能会用自定义布局替换 lua_packages/runtime 载荷。
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
            Write-Host "==> LuaSkills runtime $RuntimeDirName synced to $RuntimeDestination"
        } else {
            Write-Host "==> LuaSkills runtime package has no $RuntimeDirName directory"
        }
    }
}
if ([string]::IsNullOrWhiteSpace($ResolvedLuaSkillsRuntimeRoot)) {
    Write-Host "==> No third_party/luaskills_runtime found (run make deps first)"
}

# Sync runtime config files to output/configs/
if (Test-Path "runtime\configs") {
    if (-not (Test-Path $ConfigOut)) { New-Item -ItemType Directory -Path $ConfigOut -Force | Out-Null }
    Copy-Item -Force -Recurse "runtime\configs\*" $ConfigOut
    Enable-OutputModelConfigForLocalTesting -ConfigDirectory $ConfigOut
    Write-Host "==> Runtime configs synced to $ConfigOut"
} else {
    Write-Host "==> No runtime/configs directory found"
}

# Sync runtime shared resources to output/resources/
if (Test-Path "runtime\resources") {
    Copy-Item -Force -Recurse "runtime\resources\*" $ResourcesOut
    Write-Host "==> Runtime shared resources synced to $ResourcesOut"
} else {
    Write-Host "==> No runtime/resources directory found"
}

# Sync runtime state records to output/state/
# 同步运行时状态记录到 output/state/
if (Test-Path "runtime\state") {
    Copy-Item -Force -Recurse "runtime\state\*" $StateOut
    Write-Host "==> Runtime state synced to $StateOut"
} else {
    Write-Host "==> No runtime/state directory found"
}

# Sync runtime Lua skills to output/skills/
if (Test-Path "runtime\skills") {
    Reset-DirectoryContents -Path $SkillsOut
    Copy-Item -Force -Recurse "runtime\skills\*" $SkillsOut
    Write-Host "==> Runtime Lua skills synced to $SkillsOut"
} else {
    Write-Host "==> No runtime/skills directory found"
}

# Prepare output/bin/tools/ as the host runtime tool directory.
$HostToolsOut = Join-Path $BaseOutDir "bin\tools"
if (-not (Test-Path $HostToolsOut)) { New-Item -ItemType Directory -Path $HostToolsOut -Force | Out-Null }
Write-Host "==> Host tool output directory prepared at $HostToolsOut"

# Copy the host-installed vldb-controller executable to output/bin/ when the dependency bootstrap has prepared it.
$ControllerOut = Join-Path $BaseOutDir "bin"
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
