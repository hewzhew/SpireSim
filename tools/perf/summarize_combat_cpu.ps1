<#
.SYNOPSIS
Summarizes a WPR combat trace with the portable Microsoft PerfView executable.

.DESCRIPTION
Exports PerfView's flat CPU-stack CSV when necessary, isolates every
combat_contract process recorded in the trace, and reports inclusive and
exclusive samples relative to combat work rather than unrelated machine
activity. The tool neither installs software nor starts an ETW recording.

.EXAMPLE
.\tools\perf\summarize_combat_cpu.ps1 `
    -Trace .profiles\combat-cpu-20260727-155028-c4a2a400.etl
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $Trace,
    [string] $PerfView,
    [ValidateRange(1, 100)]
    [int] $Top = 20,
    [ValidateRange(10, 600)]
    [int] $ExportTimeoutSeconds = 180,
    [ValidateRange(1, 100)]
    [double] $MinResolvedCombatPercent = 95,
    [switch] $ForceExport
)

$ErrorActionPreference = "Stop"

function Resolve-TrustedPerfView([string] $RequestedPath, [string] $ProfileRoot) {
    $Candidate = if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        $RequestedPath
    }
    else {
        Get-ChildItem -LiteralPath (Join-Path $ProfileRoot "tools") `
            -Filter "PerfView-*.exe" -File -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1 -ExpandProperty FullName
    }
    if ([string]::IsNullOrWhiteSpace($Candidate)) {
        throw "portable PerfView is missing below '$ProfileRoot\tools'"
    }
    $Resolved = [IO.Path]::GetFullPath($Candidate)
    if (-not (Test-Path -LiteralPath $Resolved -PathType Leaf)) {
        throw "PerfView executable is missing at '$Resolved'"
    }
    $Signature = Get-AuthenticodeSignature -LiteralPath $Resolved
    if ($Signature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
        $Signature.SignerCertificate.Subject -notmatch "Microsoft Corporation") {
        throw "PerfView must have a valid Microsoft Authenticode signature"
    }
    return $Resolved
}

function Invoke-PerfViewCsvExport(
    [string] $Executable,
    [string] $TracePath,
    [string] $LogPath,
    [string] $SymbolCacheRoot,
    [int] $TimeoutSeconds
) {
    $StartInfo = [Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = $Executable
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.RedirectStandardOutput = $true
    $StartInfo.RedirectStandardError = $true
    # Use only immutable, build-keyed project symbols. This bypasses PerfView's
    # intermittent GUI-backed adjacent-PDB matcher and prevents a later build
    # from silently symbolizing an older trace with the wrong PDB.
    $StartInfo.Environment["_NT_SYMBOL_PATH"] = "SRV*$SymbolCacheRoot"
    foreach ($Argument in @(
            "/AcceptEULA",
            "/LogFile=$LogPath",
            "UserCommand",
            "SaveCPUStacksAsCsv",
            $TracePath
        )) {
        $StartInfo.ArgumentList.Add($Argument)
    }

    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $StartInfo
    if (-not $Process.Start()) {
        throw "failed to start PerfView"
    }
    $StandardOutput = $Process.StandardOutput.ReadToEndAsync()
    $StandardError = $Process.StandardError.ReadToEndAsync()
    $TimedOut = -not $Process.WaitForExit($TimeoutSeconds * 1000)
    if ($TimedOut) {
        try {
            $Process.Kill($true)
        }
        catch {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
        $Process.WaitForExit()
    }
    $Output = @(
        $StandardOutput.GetAwaiter().GetResult(),
        $StandardError.GetAwaiter().GetResult()
    ).Where({ -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine
    if ($TimedOut) {
        throw "PerfView CSV export exceeded $TimeoutSeconds seconds"
    }
    if ($Process.ExitCode -ne 0) {
        throw "PerfView CSV export failed with exit code $($Process.ExitCode): $Output"
    }
}

function Convert-ToNumber([string] $Value) {
    return [double]::Parse($Value, [Globalization.CultureInfo]::InvariantCulture)
}

function Convert-ToHotspot([object] $Row, [double] $CombatSamples, [string] $Metric) {
    $Samples = Convert-ToNumber $Row.$Metric
    $Symbol = $Row.Name -replace '^.*combat_contract!', ''
    return [pscustomobject][ordered]@{
        symbol = $Symbol
        samples = [int] $Samples
        combat_percent = [math]::Round(100.0 * $Samples / $CombatSamples, 2)
    }
}

$RepoRoot = [IO.Path]::GetFullPath(
    (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
)
$ProfileRoot = [IO.Path]::GetFullPath((Join-Path $RepoRoot ".profiles"))
$SymbolCacheRoot = [IO.Path]::GetFullPath((Join-Path $ProfileRoot "symbol-cache"))
$TracePath = if ([IO.Path]::IsPathRooted($Trace)) {
    [IO.Path]::GetFullPath($Trace)
}
else {
    [IO.Path]::GetFullPath((Join-Path $RepoRoot $Trace))
}
if (-not $TracePath.StartsWith(
        $ProfileRoot + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase
    )) {
    throw "combat traces must remain below '$ProfileRoot'"
}
if (-not (Test-Path -LiteralPath $TracePath -PathType Leaf)) {
    throw "combat trace is missing at '$TracePath'"
}

$PerfViewPath = Resolve-TrustedPerfView $PerfView $ProfileRoot
$BasePath = Join-Path `
    ([IO.Path]::GetDirectoryName($TracePath)) `
    ([IO.Path]::GetFileNameWithoutExtension($TracePath))
$CsvPath = "$BasePath.perfView.csv"
$LogPath = "$BasePath.perfView-export.log"
$SummaryPath = "$BasePath.perf-summary.json"
$TraceInfo = Get-Item -LiteralPath $TracePath
$NeedsExport = $ForceExport -or
    -not (Test-Path -LiteralPath $CsvPath -PathType Leaf) -or
    (Get-Item -LiteralPath $CsvPath).LastWriteTimeUtc -lt $TraceInfo.LastWriteTimeUtc
if ($NeedsExport) {
    if ($ForceExport -and (Test-Path -LiteralPath $CsvPath -PathType Leaf)) {
        Remove-Item -LiteralPath $CsvPath -Force
    }
    Invoke-PerfViewCsvExport `
        $PerfViewPath $TracePath $LogPath $SymbolCacheRoot $ExportTimeoutSeconds
}
if (-not (Test-Path -LiteralPath $CsvPath -PathType Leaf)) {
    throw "PerfView completed without producing '$CsvPath'"
}

$Rows = @(Import-Csv -LiteralPath $CsvPath)
$CombatProcesses = @($Rows | Where-Object Name -Like "Process64 combat_contract (*)*")
if ($CombatProcesses.Count -eq 0) {
    throw "trace contains no combat_contract process roots"
}
$CombatSamples = ($CombatProcesses |
        ForEach-Object { Convert-ToNumber $_.Inc } |
        Measure-Object -Sum).Sum
if ($CombatSamples -le 0) {
    throw "combat_contract process roots contain no CPU samples"
}
$CombatFunctions = @($Rows | Where-Object Name -Match '(^|\\)combat_contract!')
$UnresolvedCombatFunctions = @($CombatFunctions |
        Where-Object Name -Match 'combat_contract!\?')
$ResolvedCombatFunctions = @($CombatFunctions |
        Where-Object Name -NotMatch 'combat_contract!\?')
$ResolvedExclusiveSamples = ($ResolvedCombatFunctions |
        ForEach-Object { Convert-ToNumber $_.Exc } |
        Measure-Object -Sum).Sum
$UnresolvedExclusiveSamples = ($UnresolvedCombatFunctions |
        ForEach-Object { Convert-ToNumber $_.Exc } |
        Measure-Object -Sum).Sum
if ($null -eq $ResolvedExclusiveSamples) {
    $ResolvedExclusiveSamples = 0
}
if ($null -eq $UnresolvedExclusiveSamples) {
    $UnresolvedExclusiveSamples = 0
}
$SymbolizedExclusiveSamples = $ResolvedExclusiveSamples + $UnresolvedExclusiveSamples
$ResolvedCombatPercent = if ($SymbolizedExclusiveSamples -le 0) {
    0
} else {
    100.0 * $ResolvedExclusiveSamples / $SymbolizedExclusiveSamples
}
if ($ResolvedCombatPercent -lt $MinResolvedCombatPercent) {
    if (Test-Path -LiteralPath $SummaryPath -PathType Leaf) {
        Remove-Item -LiteralPath $SummaryPath -Force
    }
    throw (
        "combat symbols resolved only {0:N2}% of executable-exclusive samples; " +
        "required at least {1:N2}%. The trace is valid, but its hotspot report is not."
    ) -f $ResolvedCombatPercent, $MinResolvedCombatPercent
}
$InclusiveFunctions = @($CombatFunctions | Where-Object Name -NotMatch (
        'combat_contract!(?:__scrt_common_main_seh|main$|std::rt::|' +
        'std::sys::backtrace::|core::ops::function::FnOnce::call_once|' +
        'combat_contract::(?:main|run)$)'
    ))
$TopExclusive = @($CombatFunctions |
        Where-Object { (Convert-ToNumber $_.Exc) -gt 0 } |
        Sort-Object { Convert-ToNumber $_.Exc } -Descending |
        Select-Object -First $Top |
        ForEach-Object { Convert-ToHotspot $_ $CombatSamples "Exc" })
$TopInclusive = @($InclusiveFunctions |
        Where-Object { (Convert-ToNumber $_.Inc) -gt 0 } |
        Sort-Object { Convert-ToNumber $_.Inc } -Descending |
        Select-Object -First $Top |
        ForEach-Object { Convert-ToHotspot $_ $CombatSamples "Inc" })

$Summary = [ordered]@{
    schema_name = "StsCombatCpuSummaryV1"
    schema_version = 1
    analyzed_at_utc = [DateTimeOffset]::UtcNow.ToString("O")
    trace_path = $TracePath
    trace_bytes = $TraceInfo.Length
    csv_path = $CsvPath
    perfview_path = $PerfViewPath
    perfview_sha256 = (Get-FileHash -LiteralPath $PerfViewPath -Algorithm SHA256).Hash
    combat_processes = $CombatProcesses.Count
    combat_samples = [int] $CombatSamples
    resolved_combat_exclusive_samples = [int] $ResolvedExclusiveSamples
    unresolved_combat_exclusive_samples = [int] $UnresolvedExclusiveSamples
    resolved_combat_percent = [math]::Round($ResolvedCombatPercent, 2)
    top_exclusive = $TopExclusive
    top_inclusive = $TopInclusive
}
$Summary | ConvertTo-Json -Depth 8 |
    Set-Content -LiteralPath $SummaryPath -Encoding utf8

Write-Host "combat CPU samples: $([int] $CombatSamples) across $($CombatProcesses.Count) processes"
Write-Host "resolved combat symbols: $([math]::Round($ResolvedCombatPercent, 2))%"
Write-Host "top exclusive"
$TopExclusive | Format-Table samples, combat_percent, symbol -AutoSize
Write-Host "top inclusive"
$TopInclusive | Format-Table samples, combat_percent, symbol -AutoSize
Write-Host "summary: $SummaryPath"
