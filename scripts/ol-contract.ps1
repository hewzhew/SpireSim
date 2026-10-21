#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$worker = Join-Path $repository 'target\release\combat_contract.exe'
. (Join-Path $repository 'tools\cargo_artifact_contract.ps1')

Push-Location -LiteralPath $repository
try {
    if ($Arguments -and $Arguments[0] -eq 'build') {
        & cargo build --locked --release -p sts_combat_contract --bin combat_contract
        exit $LASTEXITCODE
    }
    if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
        throw 'Narrow combat-contract worker is not built. Run: .\scripts\ol-contract.ps1 build'
    }
    Assert-CargoArtifactFresh -Executable $worker -Repository $repository `
        -Label 'Combat-contract worker' -RebuildCommand '.\scripts\ol-contract.ps1 build' `
        -AdditionalInputs @('crates/sts_combat_contract/Cargo.toml')
    & $worker @Arguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
