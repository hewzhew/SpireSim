#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

& (Join-Path $PSScriptRoot 'run-play.ps1') @Arguments
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
