<#
.SYNOPSIS
Runs the local reference CLI from this repository.

.EXAMPLE
PS D:\rust\sts_simulator> .\reference-service.ps1 entity --kind card --id BattleTrance
#>
#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$python = Join-Path $repositoryRoot '.oracle-lab\reference-service-venv\Scripts\python.exe'
$forwarded = @($args)

if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
    throw "Reference service Python executable does not exist: $python"
}

Push-Location -LiteralPath $repositoryRoot
try {
    & (Join-Path $repositoryRoot 'py.ps1') -Python $python -m sts_reference.cli @forwarded
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
