param(
    # Optional skill ids to update; omitted means all install records under output/state/installs.
    # 可选的待更新技能标识；省略时使用 output/state/installs 下的全部安装记录。
    [string[]]$SkillId = @(),
    # Runtime root used as the update staging area.
    # 作为更新暂存区使用的运行根。
    [string]$OutputRuntimeRoot = "output",
    # Runtime root that receives updated skills and install records.
    # 接收已更新技能与安装记录的运行根。
    [string]$TargetRuntimeRoot = "runtime",
    # Optional explicit LuaSkills FFI dynamic library path.
    # 可选的显式 LuaSkills FFI 动态库路径。
    [string]$LuaskillsLib = "",
    # Skip cargo build when no local LuaSkills FFI library is available.
    # 当没有可用本地 LuaSkills FFI 动态库时跳过 cargo build。
    [switch]$SkipBuild,
    # Only sync skills and state install records.
    # 仅同步技能目录与 state 安装记录。
    [switch]$NoSyncDependencies,
    # Print the resolved operation without updating or syncing files.
    # 打印解析后的操作但不执行更新或同步。
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Resolve-PythonCommand {
    <#
    .SYNOPSIS
    Resolve a Python 3 command available on the current machine.
    解析当前机器上可用的 Python 3 命令。

    .OUTPUTS
    Command arguments used to launch Python.
    用于启动 Python 的命令参数。
    #>

    if (Get-Command python -ErrorAction SilentlyContinue) {
        return @("python")
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        return @("py", "-3")
    }
    throw "Unable to find python or py on PATH."
}

# ProjectRoot points at the MCP repository root regardless of the caller location.
# ProjectRoot 指向 MCP 仓库根目录，避免调用方当前位置影响路径解析。
$ScriptRoot = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($ScriptRoot) -and -not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
    $ScriptRoot = Split-Path -Parent $PSCommandPath
}
if ([string]::IsNullOrWhiteSpace($ScriptRoot)) {
    throw "Unable to resolve update_skills.ps1 script root."
}
$ProjectRoot = Split-Path -Parent $ScriptRoot
Set-Location -LiteralPath $ProjectRoot

$PythonCommand = @(Resolve-PythonCommand)
$PythonArgs = @(
    (Join-Path $ProjectRoot "scripts\update_skills.py"),
    "--output-runtime-root", $OutputRuntimeRoot,
    "--target-runtime-root", $TargetRuntimeRoot
)

foreach ($Item in $SkillId) {
    if ($Item) {
        $PythonArgs += @("--skill-id", $Item)
    }
}

if ($LuaskillsLib) {
    $PythonArgs += @("--luaskills-lib", $LuaskillsLib)
}
if ($SkipBuild) {
    $PythonArgs += "--skip-build"
}
if ($NoSyncDependencies) {
    $PythonArgs += "--no-sync-dependencies"
}
if ($DryRun) {
    $PythonArgs += "--dry-run"
}

$PythonLauncher = $PythonCommand[0]
$PythonLauncherArgs = @()
if ($PythonCommand.Length -gt 1) {
    $PythonLauncherArgs = $PythonCommand[1..($PythonCommand.Length - 1)]
}
$ProcessArgs = @($PythonLauncherArgs + $PythonArgs | Where-Object {
    $null -ne $_ -and -not [string]::IsNullOrWhiteSpace([string]$_)
})

$PythonProcess = Start-Process `
    -FilePath $PythonLauncher `
    -ArgumentList $ProcessArgs `
    -NoNewWindow `
    -Wait `
    -PassThru
exit $PythonProcess.ExitCode
