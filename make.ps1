# make.ps1 provides the PowerShell-native task entry for local build and run workflows.
# make.ps1 用于提供本地构建与运行工作流的 PowerShell 原生入口。
# CommandMode selects the top-level command such as build, run, release, or deps.
# CommandMode 用于选择顶层命令，例如 build、run、release 或 deps。
# CommandVariant carries the optional secondary mode such as release after run or host/lua after deps.
# CommandVariant 用于承接可选的二级模式，例如 run 后面的 release，或 deps 后面的 host/lua。
param(
    [Parameter(Position = 0)]
    [string]$CommandMode = "",
    [Parameter(Position = 1)]
    [string]$CommandVariant = "",
    # RemainingArgs captures trailing command arguments, such as skill ids after update-skills.
    # RemainingArgs 用于承接后续命令参数，例如 update-skills 后面的技能标识。
    [Parameter(Position = 2, ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs = @()
)

$ErrorActionPreference = "Stop"

# ScriptDir stores the repository root so child scripts are always resolved from a stable base path.
# ScriptDir 用于保存仓库根目录，确保子脚本始终从稳定的基路径解析。
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# BuildScriptPath points at the dedicated PowerShell build script to avoid duplicating cargo packaging logic here.
# BuildScriptPath 用于指向专用的 PowerShell 构建脚本，避免在这里重复 cargo 打包逻辑。
$BuildScriptPath = Join-Path $ScriptDir "scripts\build.ps1"

# RunScriptPath points at the dedicated PowerShell run script so run behavior stays centralized.
# RunScriptPath 用于指向专用的 PowerShell 运行脚本，让运行行为保持集中管理。
$RunScriptPath = Join-Path $ScriptDir "scripts\run.ps1"

# HostDepsScriptPath points at the dedicated PowerShell host-dependency bootstrap script.
# HostDepsScriptPath 用于指向专用的 PowerShell 宿主依赖初始化脚本。
$HostDepsScriptPath = Join-Path $ScriptDir "scripts\install_host_deps.ps1"

# LuaDepsScriptPath points at the dedicated PowerShell Lua dependency bootstrap script.
# LuaDepsScriptPath 用于指向专用的 PowerShell Lua 依赖初始化脚本。
$LuaDepsScriptPath = Join-Path $ScriptDir "scripts\install_lua_deps.ps1"

# ManagedRuntimeDepsScriptPath points at the verified LuaSkills 0.5.4 Python/Node fetcher.
# ManagedRuntimeDepsScriptPath 指向经过校验的 LuaSkills 0.5.4 Python/Node 拉取器。
$ManagedRuntimeDepsScriptPath = Join-Path $ScriptDir "scripts\deps\fetch_managed_runtimes.ps1"

# ManagedRuntimeLayoutCheckScriptPath points at the post-fetch manifest/layout validator.
# ManagedRuntimeLayoutCheckScriptPath 指向拉取后的清单与布局校验器。
$ManagedRuntimeLayoutCheckScriptPath = Join-Path $ScriptDir "scripts\debug-tools\managed_runtime_layout_check.py"

# UpdateSkillsScriptPath points at the dedicated PowerShell LuaSkills update script.
# UpdateSkillsScriptPath 用于指向专用的 PowerShell LuaSkills 更新脚本。
$UpdateSkillsScriptPath = Join-Path $ScriptDir "scripts\update_skills.ps1"

# Normalize-Command converts nullable command text into a trimmed lower-case token so dispatch rules remain predictable.
# Normalize-Command 用于把可空命令文本转换成去空白的小写标记，确保分发规则稳定可预测。
# Value is the raw command-line token to normalize before dispatch.
# Value 表示分发前需要归一化的原始命令行标记。
function Normalize-Command {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }

    return $Value.Trim().ToLowerInvariant()
}

# Invoke-Build forwards the current mode to scripts/build.ps1 and returns the child exit code unchanged.
# Invoke-Build 用于把当前模式转发给 scripts/build.ps1，并原样返回子进程退出码。
# IsRelease controls whether the packaged build should target Cargo release output.
# IsRelease 用于控制打包构建是否指向 Cargo 的 release 产物。
function Invoke-Build {
    param(
        [bool]$IsRelease
    )

    if (-not (Test-Path -LiteralPath $BuildScriptPath)) {
        throw "Missing build script: $BuildScriptPath"
    }

    if ($IsRelease) {
        & $BuildScriptPath -Release
    }
    else {
        & $BuildScriptPath
    }

    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-Run forwards the current mode to scripts/run.ps1 so Ctrl+C no longer traverses a cmd batch wrapper.
# Invoke-Run 用于把当前模式转发给 scripts/run.ps1，从而避免 Ctrl+C 再经过 cmd 批处理包装层。
# IsRelease controls whether the runtime should execute the release binary instead of the debug binary.
# IsRelease 用于控制运行时执行 release 二进制还是 debug 二进制。
function Invoke-Run {
    param(
        [bool]$IsRelease
    )

    if (-not (Test-Path -LiteralPath $RunScriptPath)) {
        throw "Missing run script: $RunScriptPath"
    }

    if ($IsRelease) {
        & $RunScriptPath -Release
    }
    else {
        & $RunScriptPath
    }

    exit $LASTEXITCODE
}

# Invoke-DependencyInstall delegates dependency bootstrapping to the dedicated PowerShell scripts.
# Invoke-DependencyInstall 用于把依赖初始化委托给专用的 PowerShell 脚本。
# DependencyKind selects which dependency domain should be initialized.
# DependencyKind 用于选择要初始化的依赖域。
function Invoke-DependencyInstall {
    param(
        [ValidateSet("host", "lua", "managed", "python", "node")]
        [string]$DependencyKind
    )

    $ScriptPath = switch ($DependencyKind) {
        "host" { $HostDepsScriptPath }
        "lua"  { $LuaDepsScriptPath }
        "managed" { $ManagedRuntimeDepsScriptPath }
        "python" { $ManagedRuntimeDepsScriptPath }
        "node" { $ManagedRuntimeDepsScriptPath }
        default { throw "Unsupported dependency kind: $DependencyKind" }
    }

    if (-not (Test-Path -LiteralPath $ScriptPath)) {
        throw "Missing dependency script: $ScriptPath"
    }

    # ScriptArguments selects the exact upstream fetch target for managed runtime subcommands.
    # ScriptArguments 为受管运行时子命令选择精确的上游拉取目标。
    $ScriptArguments = @()
    if ($DependencyKind -eq "managed") {
        $ScriptArguments = @("-Target", "all")
    }
    elseif ($DependencyKind -eq "python") {
        $ScriptArguments = @("-Target", "python")
    }
    elseif ($DependencyKind -eq "node") {
        $ScriptArguments = @("-Target", "node")
    }

    # PowerShellCommand prefers modern pwsh while retaining Windows PowerShell execution support.
    # PowerShellCommand 优先使用现代 pwsh，同时保留 Windows PowerShell 执行支持。
    $PowerShellCommand = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCommand) {
        & $PowerShellCommand.Source -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ScriptArguments
    }
    else {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ScriptArguments
    }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    if ($DependencyKind -eq "managed") {
        # PythonCommand runs the repository validator after the complete managed fetch target.
        # PythonCommand 在完整受管拉取目标完成后运行仓库校验器。
        $PythonCommand = Get-Command "python" -ErrorAction SilentlyContinue
        if (-not $PythonCommand) {
            throw "python is required to validate the fetched managed runtime layout"
        }
        & $PythonCommand.Source $ManagedRuntimeLayoutCheckScriptPath `
            (Join-Path $ScriptDir "third_party\managed_runtime_cache") `
            --distribution-root (Join-Path $ScriptDir "third_party\luaskills_managed_runtimes")
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}

# Invoke-UpdateSkills delegates managed LuaSkills updates to the dedicated PowerShell script.
# Invoke-UpdateSkills 用于把受管 LuaSkills 更新委托给专用的 PowerShell 脚本。
# SkillIds optionally limits the update to specific skill identifiers.
# SkillIds 用于可选地把更新范围限制到指定技能标识。
function Invoke-UpdateSkills {
    param(
        [string[]]$SkillIds
    )

    if (-not (Test-Path -LiteralPath $UpdateSkillsScriptPath)) {
        throw "Missing update skills script: $UpdateSkillsScriptPath"
    }

    if ($SkillIds -and $SkillIds.Count -gt 0) {
        & $UpdateSkillsScriptPath -SkillId $SkillIds
    }
    else {
        & $UpdateSkillsScriptPath
    }

    exit $LASTEXITCODE
}

# Show-Usage prints the supported command forms so contributors can quickly recover from invalid input.
# Show-Usage 用于输出支持的命令形式，方便贡献者在输入无效参数后快速恢复。
function Show-Usage {
    Write-Host "Usage:"
    Write-Host "  ./make             # debug build"
    Write-Host "  ./make build       # debug build"
    Write-Host "  ./make release     # release build"
    Write-Host "  ./make run         # run debug build"
    Write-Host "  ./make run release # run release build"
    Write-Host "  ./make deps        # install host + Lua + managed Python/Node dependencies"
    Write-Host "  ./make deps host   # install host native dependencies only"
    Write-Host "  ./make deps lua    # install official LuaSkills runtime dependencies only"
    Write-Host "  ./make deps managed # fetch managed Python + Node distributions"
    Write-Host "  ./make deps python # fetch managed Python distribution only"
    Write-Host "  ./make deps node   # fetch managed Node + pnpm distributions only"
    Write-Host "  ./make update-skills [skill-id...] # update output skills and sync them into runtime"
}

# NormalizedMode stores the canonical top-level command token used by the dispatcher below.
# NormalizedMode 用于保存下方分发器使用的规范化顶层命令标记。
$NormalizedMode = Normalize-Command -Value $CommandMode

# NormalizedVariant stores the canonical secondary token used to distinguish release runs from default runs.
# NormalizedVariant 用于保存规范化的二级标记，以区分 release 运行与默认运行。
$NormalizedVariant = Normalize-Command -Value $CommandVariant

switch ($NormalizedMode) {
    "" {
        Invoke-Build -IsRelease $false
    }
    "build" {
        Invoke-Build -IsRelease ($NormalizedVariant -eq "release")
    }
    "release" {
        Invoke-Build -IsRelease $true
    }
    "run" {
        Invoke-Run -IsRelease ($NormalizedVariant -eq "release")
    }
    "deps" {
        switch ($NormalizedVariant) {
            "" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "lua"
                Invoke-DependencyInstall -DependencyKind "managed"
            }
            "all" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "lua"
                Invoke-DependencyInstall -DependencyKind "managed"
            }
            "host" {
                Invoke-DependencyInstall -DependencyKind "host"
            }
            "lua" {
                Invoke-DependencyInstall -DependencyKind "lua"
            }
            "managed" {
                Invoke-DependencyInstall -DependencyKind "managed"
            }
            "python" {
                Invoke-DependencyInstall -DependencyKind "python"
            }
            "node" {
                Invoke-DependencyInstall -DependencyKind "node"
            }
            default {
                Write-Error "Unsupported deps command: '$CommandVariant'"
                Show-Usage
                exit 1
            }
        }
    }
    "update-skills" {
        # SkillIds collects explicit update targets from all trailing command tokens.
        # SkillIds 用于从所有后续命令标记中收集显式更新目标。
        $SkillIds = @()
        if (-not [string]::IsNullOrWhiteSpace($CommandVariant)) {
            $SkillIds += $CommandVariant
        }
        $SkillIds += $RemainingArgs
        Invoke-UpdateSkills -SkillIds $SkillIds
    }
    default {
        Write-Error "Unsupported command: '$CommandMode'"
        Show-Usage
        exit 1
    }
}

