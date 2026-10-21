<#
.SYNOPSIS
Runs the repository Python through a strict UTF-8 stdio and import boundary.

.DESCRIPTION
Internal developer helper for ad-hoc Python diagnostics and tests on Windows.
It selects the interpreter in this order: an explicit leading
`-Python <python.exe>`, an explicit leading `-Environment <name>` backed by
`.oracle-lab/hosts/<name>-python.txt`, the repository's configured
`.oracle-lab/hosts/learning-python.txt`, then PATH only when that configuration
file does not exist. An empty or stale configured path fails closed instead of
silently selecting a different environment.

The helper configures UTF-8, propagates the
native exit code, and restores the caller's console and environment afterward.
Every token after the optional selector is forwarded unchanged to Python,
including short options such as pytest's `-p`. Pipeline input, when present, is
forwarded to Python stdin so typed workers can be exercised through this same
interpreter boundary.

.EXAMPLE
.\scripts\py.ps1 -m unittest discover -s tools/wiki/tests -p 'test_*.py'

.EXAMPLE
.\scripts\py.ps1 -m pytest tools/wiki/tests -q

.EXAMPLE
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m pytest reference_service/tests -q

#>
#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$pipelineInputSupplied = $MyInvocation.ExpectingInput

$pythonArguments = @($args)
$selectedPython = $null
$pythonSelectorSupplied = $false
$environmentSelectorSupplied = $false
$selectedEnvironment = $null
if ($pythonArguments.Count -gt 0 -and $pythonArguments[0] -ieq '-Python') {
    $pythonSelectorSupplied = $true
    if ($pythonArguments.Count -lt 2) {
        throw 'py.ps1 requires an executable after -Python'
    }
    $selectedPython = [string] $pythonArguments[1]
    $pythonArguments = if ($pythonArguments.Count -gt 2) {
        @($pythonArguments[2..($pythonArguments.Count - 1)])
    }
    else {
        @()
    }
}
elseif ($pythonArguments.Count -gt 0 -and $pythonArguments[0] -ieq '-Environment') {
    $environmentSelectorSupplied = $true
    if ($pythonArguments.Count -lt 2) {
        throw 'py.ps1 requires a name after -Environment'
    }
    $selectedEnvironment = [string] $pythonArguments[1]
    if ($selectedEnvironment -notmatch '^[a-z][a-z0-9-]{0,31}$') {
        throw 'py.ps1 environment names must be lowercase tokens'
    }
    $pythonArguments = if ($pythonArguments.Count -gt 2) {
        @($pythonArguments[2..($pythonArguments.Count - 1)])
    }
    else {
        @()
    }
}

# PowerShell enumerates the output of the selector expression above. When
# exactly one Python argument remains, that would otherwise collapse to a
# scalar and splatting would forward it one character at a time.
$pythonArguments = @($pythonArguments)

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$configuredPythonFile = if ($environmentSelectorSupplied) {
    Join-Path $repositoryRoot ".oracle-lab\hosts\$selectedEnvironment-python.txt"
}
else {
    Join-Path $repositoryRoot '.oracle-lab\hosts\learning-python.txt'
}

if ($pythonSelectorSupplied) {
    if ([string]::IsNullOrWhiteSpace($selectedPython)) {
        throw 'py.ps1 requires a non-empty executable after -Python'
    }
    if (Test-Path -LiteralPath $selectedPython -PathType Leaf) {
        $pythonPath = (Resolve-Path -LiteralPath $selectedPython).Path
    }
    else {
        $command = Get-Command $selectedPython -CommandType Application -ErrorAction Stop |
            Select-Object -First 1
        $pythonPath = $command.Path
    }
}
elseif ($environmentSelectorSupplied -and -not (Test-Path -LiteralPath $configuredPythonFile -PathType Leaf)) {
    throw "Configured Python environment does not exist: $configuredPythonFile"
}
elseif (Test-Path -LiteralPath $configuredPythonFile -PathType Leaf) {
    $configuredPython = [IO.File]::ReadAllText($configuredPythonFile).Trim()
    if ([string]::IsNullOrWhiteSpace($configuredPython)) {
        throw "Configured learning Python path is empty: $configuredPythonFile"
    }
    $configuredPythonPath = if ([IO.Path]::IsPathRooted($configuredPython)) {
        $configuredPython
    }
    else {
        Join-Path $repositoryRoot $configuredPython
    }
    if (-not (Test-Path -LiteralPath $configuredPythonPath -PathType Leaf)) {
        throw "Configured learning Python executable does not exist: $configuredPythonPath"
    }
    $pythonPath = (Resolve-Path -LiteralPath $configuredPythonPath).Path
}
else {
    $command = Get-Command python -CommandType Application -ErrorAction Stop |
        Select-Object -First 1
    $pythonPath = $command.Path
}

if (-not (Test-Path -LiteralPath $pythonPath -PathType Leaf)) {
    throw "Python executable does not exist: $pythonPath"
}
$utf8WithoutBom = [System.Text.UTF8Encoding]::new($false, $true)
$previousInputEncoding = [Console]::InputEncoding
$previousOutputEncoding = [Console]::OutputEncoding
$previousPythonIoEncoding = $env:PYTHONIOENCODING
$previousPythonUtf8 = $env:PYTHONUTF8
$exitCode = 0

try {
    [Console]::InputEncoding = $utf8WithoutBom
    [Console]::OutputEncoding = $utf8WithoutBom
    $env:PYTHONIOENCODING = 'utf-8:strict'
    $env:PYTHONUTF8 = '1'
    # A lexical $input reference makes pwsh -File pre-read native stdin using
    # its startup decoder. Resolve it dynamically only for a real PowerShell
    # pipeline; native callers keep direct, unbuffered stdin/stdout access.
    if ($pipelineInputSupplied) {
        $pipelineValues = Get-Variable -Name input
        $pipelineValues.Value | & $pythonPath @pythonArguments
    }
    else {
        & $pythonPath @pythonArguments
    }
    $exitCode = $LASTEXITCODE
}
finally {
    [Console]::InputEncoding = $previousInputEncoding
    [Console]::OutputEncoding = $previousOutputEncoding
    $env:PYTHONIOENCODING = $previousPythonIoEncoding
    $env:PYTHONUTF8 = $previousPythonUtf8
}

exit $exitCode
