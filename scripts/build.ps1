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
$LuaRuntimeOut = "$BaseOutDir\lua_runtime"
$LibsOut = "$LuaRuntimeOut\libs"
$SkillsOut = "$LuaRuntimeOut\skills"
$PkgOut = "$LuaRuntimeOut\lua_packages"
$ConfigOut = "$BaseOutDir\configs"
$ResourcesOut = "$LuaRuntimeOut\resources"
$LicensesOut = "$LuaRuntimeOut\licenses"
$DependenciesOut = "$LuaRuntimeOut\dependencies"
$DatabasesOut = "$LuaRuntimeOut\databases"
$StateOut = "$LuaRuntimeOut\state"
$TempOut = "$LuaRuntimeOut\temp"
$SystemLuaLibOut = "$LuaRuntimeOut\system_lua_lib"
$LogsOut = "$BaseOutDir\logs"
$LuaSkillsRuntimeRoot = Join-Path $ProjectDir "third_party\luaskills_runtime"
$ManagedRuntimeDistributionRoot = Join-Path $ProjectDir "third_party\luaskills_managed_runtimes"
$SourceLuaRuntimeRoot = Join-Path $ProjectDir "runtime\lua_runtime"
$ManagedRuntimeLayoutCheckScript = Join-Path $ProjectDir "scripts\debug-tools\managed_runtime_layout_check.py"

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

    # Utf8NoBom preserves the repository template encoding while the output-only flags are changed.
    # Utf8NoBom 在修改仅用于输出的开关时保持仓库模板的 UTF-8 编码。
    $Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    # Content is read explicitly as UTF-8 so Windows PowerShell cannot reinterpret Chinese comments through the active ANSI code page.
    # Content 以显式 UTF-8 读取，避免 Windows PowerShell 通过当前 ANSI 代码页错误解释中文注释。
    $Content = [System.IO.File]::ReadAllText($ModelConfigOut, $Utf8NoBom)
    $Content = $Content -replace '(?m)^  enabled:[ \t]*false[ \t]*$', '  enabled: true'
    $Content = $Content -replace '(?m)^    enabled:[ \t]*false[ \t]*$', '    enabled: true'
    [System.IO.File]::WriteAllText($ModelConfigOut, $Content, $Utf8NoBom)
    Write-Host "==> Output model_config.yaml enabled for local model smoke tests"
}

# Ensure output directories exist
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
foreach ($dir in @($LuaRuntimeOut, $LibsOut, $SkillsOut, $PkgOut, $ConfigOut, $ResourcesOut, $LicensesOut, $DependenciesOut, $DatabasesOut, $StateOut, $TempOut, $SystemLuaLibOut, $LogsOut)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}
foreach ($dir in @(
    "$DependenciesOut\\runtimes",
    "$DependenciesOut\\envs",
    "$DatabasesOut\\sqlite",
    "$DatabasesOut\\lancedb",
    "$StateOut\\skills"
)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

# Copy binary
Copy-Item -Force $BinExe "$OutDir\$BinName.exe"
Write-Host "==> Binary copied to $OutDir"

# Sync official LuaSkills runtime package exports to output/lua_runtime/.
# 同步官方 LuaSkills 运行时包导出内容到 output/lua_runtime/。
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

# Sync fetched managed Python and Node distributions into the fixed LuaSkills dependency root.
# 把已拉取的受管 Python 与 Node 发行包同步到固定 LuaSkills 依赖根。
if (Copy-DirectoryContents -Source $ManagedRuntimeDistributionRoot -Destination (Join-Path $DependenciesOut "runtimes")) {
    Write-Host "==> Managed Python/Node distributions synced to $DependenciesOut\runtimes"
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

# Sync runtime config files to output/configs/
if (Test-Path "runtime\configs") {
    if (-not (Test-Path $ConfigOut)) { New-Item -ItemType Directory -Path $ConfigOut -Force | Out-Null }
    # Rebuild the generated config directory as an exact mirror of the current repository templates.
    # 将生成配置目录重建为当前仓库模板的精确镜像。
    Reset-DirectoryContents -Path $ConfigOut
    Copy-Item -Force -Recurse "runtime\configs\*" $ConfigOut
    Enable-OutputModelConfigForLocalTesting -ConfigDirectory $ConfigOut
    Write-Host "==> Runtime configs synced to $ConfigOut"
} else {
    Write-Host "==> No runtime/configs directory found"
}

# Sync runtime shared resources to output/lua_runtime/resources/.
# 同步运行时共享资源到 output/lua_runtime/resources/。
if (Test-Path "runtime\lua_runtime\resources") {
    Copy-Item -Force -Recurse "runtime\lua_runtime\resources\*" $ResourcesOut
    Write-Host "==> Runtime shared resources synced to $ResourcesOut"
} else {
    Write-Host "==> No runtime/lua_runtime/resources directory found"
}

# Sync runtime state records to output/lua_runtime/state/.
# 同步运行时状态记录到 output/lua_runtime/state/。
if (Test-Path "runtime\lua_runtime\state") {
    Copy-Item -Force -Recurse "runtime\lua_runtime\state\*" $StateOut
    Write-Host "==> Runtime state synced to $StateOut"
} else {
    Write-Host "==> No runtime/lua_runtime/state directory found"
}

# Sync runtime Lua skills to output/lua_runtime/skills/.
# 同步运行时 Lua 技能到 output/lua_runtime/skills/。
if (Test-Path "runtime\lua_runtime\skills") {
    Reset-DirectoryContents -Path $SkillsOut
    Copy-Item -Force -Recurse "runtime\lua_runtime\skills\*" $SkillsOut
    Write-Host "==> Runtime Lua skills synced to $SkillsOut"
} else {
    Write-Host "==> No runtime/lua_runtime/skills directory found"
}

# Synchronize source-controlled dependency payloads without deleting writable envs or fetched runtimes.
# 同步源码控制的依赖载荷，同时不删除可写 envs 或已拉取的 runtimes。
foreach ($DependencyKind in @("tools", "lua", "ffi")) {
    # DependencySource is the optional source-controlled package family.
    # DependencySource 是可选的源码控制依赖包族。
    $DependencySource = Join-Path $SourceLuaRuntimeRoot ("dependencies\" + $DependencyKind)
    if (Test-Path -LiteralPath $DependencySource) {
        # DependencyDestination stays below the fixed LuaSkills dependencies root.
        # DependencyDestination 固定保留在 LuaSkills dependencies 根下。
        $DependencyDestination = Join-Path $DependenciesOut $DependencyKind
        Reset-DirectoryContents -Path $DependencyDestination
        Copy-Item -Force -Recurse (Join-Path $DependencySource "*") $DependencyDestination
    }
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
