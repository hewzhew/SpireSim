#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = $PSScriptRoot
$frontend = Join-Path $repository 'target\debug\combat_search_v2.exe'
$worker = Join-Path $repository 'target\release\combat_search_v2_worker.exe'
. (Join-Path $repository 'tools\cargo_artifact_contract.ps1')

function Invoke-Checked {
    param([string] $Executable, [string[]] $CommandArguments = @())
    & $Executable @CommandArguments
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Push-Location -LiteralPath $repository
try {
    if ($Arguments -and $Arguments[0] -eq 'build') {
        $target = if ($Arguments.Count -ge 2) { $Arguments[1] } else { 'all' }
        if ($target -notin @('frontend', 'worker', 'all')) {
            throw "Unknown build target '$target'. Expected frontend, worker, or all."
        }
        if ($target -in @('frontend', 'all')) {
            Invoke-Checked cargo @('build', '--locked', '-p', 'sts_combat_search_driver', '--bin', 'combat_search_v2')
        }
        if ($target -in @('worker', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '--release', '-p', 'sts_combat_search_driver',
                '--features', 'backend', '--bin', 'combat_search_v2_worker'
            )
        }
        exit 0
    }

    $helpRequest = -not $Arguments -or $Arguments.Count -eq 0 -or $Arguments[0] -in @('--help', '-h')
    if ($helpRequest) {
        if (-not (Test-Path -LiteralPath $frontend -PathType Leaf)) {
            throw 'Combat-search help frontend is not built. Run: .\cs.ps1 build frontend'
        }
        Assert-CargoArtifactFresh -Executable $frontend -Repository $repository `
            -Label 'Combat-search help frontend' -RebuildCommand '.\cs.ps1 build frontend' `
            -AdditionalInputs @('crates/sts_combat_search_driver/Cargo.toml')
        Invoke-Checked $frontend $Arguments
        exit 0
    }
    if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
        throw 'Combat-search worker is not built. Run: .\cs.ps1 build worker'
    }
    Assert-CargoArtifactFresh -Executable $worker -Repository $repository `
        -Label 'Combat-search worker' -RebuildCommand '.\cs.ps1 build worker' `
        -AdditionalInputs @('crates/sts_combat_search_driver/Cargo.toml')
    Invoke-Checked $worker $Arguments
}
finally {
    Pop-Location
}
