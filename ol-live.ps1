#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = $PSScriptRoot
$client = Join-Path $repository 'target\release\oracle_lab_client.exe'
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
        if ($target -notin @('client', 'host', 'all')) {
            throw "Unknown build target '$target'. Expected client, host, or all."
        }
        if ($target -in @('client', 'all')) {
            Invoke-Checked cargo @('build', '--locked', '--release', '-p', 'oracle_lab_client', '--bin', 'oracle_lab_client')
        }
        if ($target -in @('host', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '--release', '-p', 'sts_oracle_lab',
                '--features', 'canonical-oracle-artifacts', '--bin', 'oracle_lab_service'
            )
        }
        exit 0
    }
    if (-not (Test-Path -LiteralPath $client -PathType Leaf)) {
        throw 'Canonical resident client is not built. Run: .\ol-live.ps1 build client'
    }
    Assert-CargoArtifactFresh -Executable $client -Repository $repository `
        -Label 'Canonical resident client' -RebuildCommand '.\ol-live.ps1 build client' `
        -AdditionalInputs @('crates/oracle_lab_client/Cargo.toml') `
        -SelfGuardInputs @(
            'crates/oracle_lab_client/src/main.rs',
            'crates/oracle_artifact_contract/src/lib.rs',
            'crates/oracle_artifact_contract/build-inputs/oracle-client.txt'
        )
    Invoke-Checked $client (@('--canonical-oracle') + $Arguments)
}
finally {
    Pop-Location
}
