# Run vulcan-agent-service binary (PowerShell)
# Usage:
#   .\run.ps1           # run debug build
#   .\run.ps1 release   # run release build
#   .\run.ps1 -Release  # run release build

param(
    [switch]$Release
)

$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

if ($Release -or ($args.Count -gt 0 -and $args[0] -eq "release")) {
    $BinPath = "output\bin\vulcan-agent-service.exe"
} else {
    $BinPath = "output\debug\vulcan-agent-service.exe"
}

if (-not (Test-Path $BinPath)) {
    Write-Host "==> Binary not found: $BinPath"
    Write-Host "==> Run 'make' or 'make release' first."
    exit 1
}

Write-Host "==> Running vulcan-agent-service ($BinPath)..."
Write-Host ""

& $BinPath
$code = $LASTEXITCODE

Write-Host ""
Write-Host "==> Process exited with code $code."
exit $code
