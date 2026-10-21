#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$python = (Get-Command python -ErrorAction Stop).Source
$utf8WithoutBom = [System.Text.UTF8Encoding]::new($false)
$previousConsoleOutputEncoding = [Console]::OutputEncoding
$previousPythonIoEncoding = $env:PYTHONIOENCODING
$previousPythonUtf8 = $env:PYTHONUTF8

Push-Location -LiteralPath $repository
try {
    [Console]::OutputEncoding = $utf8WithoutBom
    $env:PYTHONIOENCODING = 'utf-8'
    $env:PYTHONUTF8 = '1'
    & $python -m tools.wiki.sts_wiki @Arguments
    exit $LASTEXITCODE
}
finally {
    [Console]::OutputEncoding = $previousConsoleOutputEncoding
    $env:PYTHONIOENCODING = $previousPythonIoEncoding
    $env:PYTHONUTF8 = $previousPythonUtf8
    Pop-Location
}
