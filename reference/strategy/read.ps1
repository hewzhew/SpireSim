#requires -Version 7.0
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Subject,
    [switch] $List,
    [switch] $Json
)

$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$arguments = @('-m', 'tools.strategy_reader.cli', '--repository', $repository)
if ($List) { $arguments += '--list' }
if ($Subject) { $arguments += $Subject }
if ($Json) { $arguments += '--json' }
& (Join-Path $repository 'py.ps1') @arguments
exit $LASTEXITCODE
