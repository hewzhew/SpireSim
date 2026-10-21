[CmdletBinding()]
param(
    [ValidateSet('Report', 'CleanProfile', 'CleanAll')]
    [string] $Action = 'Report',

    [ValidateSet('dev', 'release', 'dev-opt', 'profiling', 'release-final')]
    [string] $Profile,

    [ValidateRange(1, 4096)]
    [double] $WarnAboveGB = 100,

    [switch] $ConfirmAllBuildArtifacts
)

$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Get-DirectoryBytes([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return [int64]0
    }
    $measurement = Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction Stop |
        Measure-Object -Property Length -Sum
    return [int64]$measurement.Sum
}

Push-Location -LiteralPath $repository
try {
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) {
        throw 'cargo metadata failed'
    }
    $workspaceRoot = [IO.Path]::GetFullPath([string]$metadata.workspace_root)
    if ($workspaceRoot -ne $repository) {
        throw "Cargo workspace root differs from the validated repository: $workspaceRoot"
    }
    $target = [IO.Path]::GetFullPath([string]$metadata.target_directory)
    $expectedTarget = [IO.Path]::GetFullPath((Join-Path $repository 'target'))
    if ($target -ne $expectedTarget) {
        throw "Refusing an unsafe Cargo target directory: $target"
    }

    $driveName = [IO.Path]::GetPathRoot($target).TrimEnd('\').TrimEnd(':')
    $beforeFree = (Get-PSDrive -Name $driveName).Free
    $beforeBytes = Get-DirectoryBytes $target

    switch ($Action) {
        'Report' {}
        'CleanProfile' {
            if ([string]::IsNullOrWhiteSpace($Profile)) {
                throw 'CleanProfile requires -Profile.'
            }
            cargo clean --profile $Profile
            if ($LASTEXITCODE -ne 0) {
                throw "cargo clean --profile $Profile failed"
            }
        }
        'CleanAll' {
            if (-not $ConfirmAllBuildArtifacts) {
                throw 'CleanAll requires -ConfirmAllBuildArtifacts.'
            }
            cargo clean
            if ($LASTEXITCODE -ne 0) {
                throw 'cargo clean failed'
            }
        }
    }

    $afterFree = (Get-PSDrive -Name $driveName).Free
    $afterBytes = Get-DirectoryBytes $target
    [pscustomobject]@{
        schema = 'sts-cargo-cache-report-v1'
        action = $Action
        profile = if ($Action -eq 'CleanProfile') { $Profile } else { $null }
        workspace_root = $workspaceRoot
        target_directory = $target
        target_gb_before = [math]::Round($beforeBytes / 1GB, 2)
        target_gb_after = [math]::Round($afterBytes / 1GB, 2)
        disk_free_gb_before = [math]::Round($beforeFree / 1GB, 2)
        disk_free_gb_after = [math]::Round($afterFree / 1GB, 2)
        warning = if ($afterBytes -gt $WarnAboveGB * 1GB) {
            "Cargo target exceeds the configured ${WarnAboveGB} GB threshold"
        } else {
            $null
        }
    } | ConvertTo-Json -Depth 4
} finally {
    Pop-Location
}
