<#
.SYNOPSIS
Benchmarks the canonical replay-verified combat contract.

.DESCRIPTION
Builds the narrow profiling target once, warms it up, and runs repeatable
batches using exactly the same workload as the WPR profiler. Every iteration
must return identical search counters and witness identity; timing-only fields
are deliberately excluded from that contract check.

.EXAMPLE
.\tools\perf\benchmark_combat_contract.ps1

.EXAMPLE
.\tools\perf\benchmark_combat_contract.ps1 -SkipBuild -Batches 5 -IterationsPerBatch 12
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [string] $Case = ".oracle-lab\cases\seed022-a2f32-collector-full-hp.combat.json",
    [ValidateRange(1, 20)]
    [int] $Batches = 3,
    [ValidateRange(1, 100)]
    [int] $IterationsPerBatch = 8,
    [ValidateRange(0, 20)]
    [int] $WarmupIterations = 1,
    [ValidateSet("Auto", "Prepare", "Measure")]
    [string] $Phase = "Auto",
    [ValidateRange(0, 3600)]
    [int] $MeasurementBudgetSeconds = 45,
    # Backward-compatible alias for -Phase Measure.
    [switch] $SkipBuild,
    [switch] $AsJson
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "combat_contract_workload.ps1")
. (Join-Path $PSScriptRoot "combat_contract_build_receipt.ps1")

function Get-ContractIdentity($Report) {
    return [ordered]@{
        status = $Report.status
        counters = $Report.counters
        witness = $Report.witness
    }
}

function Get-Median([double[]] $Values) {
    if ($Values.Count -eq 0) {
        return $null
    }
    $Sorted = @($Values | Sort-Object)
    $Middle = [int][Math]::Floor($Sorted.Count / 2)
    if (($Sorted.Count % 2) -eq 1) {
        return $Sorted[$Middle]
    }
    return ($Sorted[$Middle - 1] + $Sorted[$Middle]) / 2.0
}

function Invoke-CombatContract(
    [string] $Executable,
    [object[]] $Arguments
) {
    $StartInfo = [Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = $Executable
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.RedirectStandardOutput = $true
    $StartInfo.RedirectStandardError = $true
    foreach ($Argument in $Arguments) {
        $StartInfo.ArgumentList.Add([string] $Argument)
    }

    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $StartInfo
    $Stopwatch = [Diagnostics.Stopwatch]::StartNew()
    if (-not $Process.Start()) {
        throw "failed to start combat contract"
    }
    $StandardOutput = $Process.StandardOutput.ReadToEndAsync()
    $StandardError = $Process.StandardError.ReadToEndAsync()
    $Process.WaitForExit()
    $Stopwatch.Stop()
    $Raw = $StandardOutput.GetAwaiter().GetResult().Trim()
    $ErrorText = $StandardError.GetAwaiter().GetResult().Trim()
    if ($Process.ExitCode -ne 0) {
        throw "combat contract failed with exit code $($Process.ExitCode)`n$ErrorText`n$Raw"
    }
    try {
        $Report = $Raw | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "combat contract did not return valid JSON`n$Raw"
    }
    return [pscustomobject]@{
        report = $Report
        elapsed_milliseconds = $Stopwatch.Elapsed.TotalMilliseconds
    }
}

$RepoRoot = [IO.Path]::GetFullPath(
    (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
)
$CasePath = if ([IO.Path]::IsPathRooted($Case)) {
    [IO.Path]::GetFullPath($Case)
} else {
    [IO.Path]::GetFullPath((Join-Path $RepoRoot $Case))
}
if (-not (Test-Path -LiteralPath $CasePath -PathType Leaf)) {
    throw "combat case is missing at '$CasePath'"
}

Push-Location $RepoRoot
try {
    if ($SkipBuild) {
        if ($Phase -ne "Auto") {
            throw "-SkipBuild cannot be combined with -Phase; use -Phase Measure"
        }
        $Phase = "Measure"
    }
    $Executable = Join-Path $RepoRoot "target\profiling\combat_contract.exe"
    $BuildStatus = Get-StsCombatContractBuildReceiptStatus $RepoRoot $Executable
    if ($Phase -eq "Measure" -and -not $BuildStatus.valid) {
        throw "combat contract benchmark is not prepared: $($BuildStatus.reason); run once with -Phase Prepare"
    }
    if ($Phase -eq "Prepare" -or ($Phase -eq "Auto" -and -not $BuildStatus.valid)) {
        & cargo build --locked --profile profiling -p sts_combat_contract --bin combat_contract
        if ($LASTEXITCODE -ne 0) {
            throw "combat contract profiling build failed"
        }
        $BuildReceipt = Write-StsCombatContractBuildReceipt $RepoRoot $Executable
        $Preparation = [ordered]@{
            schema_name = "CombatContractBenchmarkPreparationV1"
            schema_version = 1
            prepared = $true
            measurement_ran = $false
            requested_phase = $Phase
            previous_receipt_valid = [bool] $BuildStatus.valid
            previous_receipt_problem = $BuildStatus.reason
            build_source_fingerprint = $BuildReceipt.source_fingerprint
            executable_sha256 = $BuildReceipt.executable_sha256
            next_command = ".\tools\perf\benchmark_combat_contract.ps1 -Phase Measure"
        }
        if ($AsJson) {
            $Preparation | ConvertTo-Json -Depth 10
        }
        else {
            [pscustomobject] $Preparation | Format-List
        }
        return
    }
    $BuildReceipt = $BuildStatus.receipt
    $WorkloadArguments = Get-StsCombatContractWorkloadArguments $CasePath

    $ExpectedIdentity = $null
    $ExpectedIdentityJson = $null
    $LastReport = $null
    for ($Index = 0; $Index -lt $WarmupIterations; $Index++) {
        $Run = Invoke-CombatContract $Executable $WorkloadArguments
        $LastReport = $Run.report
        $ExpectedIdentity = Get-ContractIdentity $LastReport
        $ExpectedIdentityJson = $ExpectedIdentity | ConvertTo-Json -Depth 100 -Compress
    }

    $BatchMilliseconds = [Collections.Generic.List[double]]::new()
    $BatchIterationCounts = [Collections.Generic.List[int]]::new()
    $IterationMilliseconds = [Collections.Generic.List[double]]::new()
    $SearchMilliseconds = [Collections.Generic.List[double]]::new()
    $TransitionMetricNames = @(
        "simulation",
        "identity",
        "key_build",
        "key_index",
        "seen_set",
        "publish"
    )
    $TransitionMetricSamples = @{}
    foreach ($MetricName in $TransitionMetricNames) {
        $TransitionMetricSamples[$MetricName] = [Collections.Generic.List[double]]::new()
    }
    $MeasurementStopwatch = [Diagnostics.Stopwatch]::StartNew()
    $CompletedIterations = 0
    $StoppedForBudget = $false
    :BatchLoop for ($Batch = 1; $Batch -le $Batches; $Batch++) {
        $BatchElapsedMilliseconds = 0.0
        $BatchCompletedIterations = 0
        for ($Iteration = 1; $Iteration -le $IterationsPerBatch; $Iteration++) {
            if ($CompletedIterations -gt 0 -and
                $MeasurementBudgetSeconds -gt 0 -and
                $MeasurementStopwatch.Elapsed.TotalSeconds -ge $MeasurementBudgetSeconds) {
                $StoppedForBudget = $true
                break BatchLoop
            }
            $Run = Invoke-CombatContract $Executable $WorkloadArguments
            $BatchElapsedMilliseconds += $Run.elapsed_milliseconds
            $IterationMilliseconds.Add($Run.elapsed_milliseconds)
            $CompletedIterations++
            $BatchCompletedIterations++
            $LastReport = $Run.report
            $SearchMilliseconds.Add($LastReport.search_elapsed_ns / 1000000.0)
            foreach ($MetricName in $TransitionMetricNames) {
                $MetricValue = $LastReport.ns_per_applied_transition.$MetricName
                if ($null -ne $MetricValue) {
                    $TransitionMetricSamples[$MetricName].Add([double] $MetricValue)
                }
            }
            $Identity = Get-ContractIdentity $LastReport
            $IdentityJson = $Identity | ConvertTo-Json -Depth 100 -Compress
            if ($null -eq $ExpectedIdentityJson) {
                $ExpectedIdentity = $Identity
                $ExpectedIdentityJson = $IdentityJson
            } elseif ($IdentityJson -cne $ExpectedIdentityJson) {
                throw "search identity changed in batch $Batch iteration $Iteration"
            }
        }
        $BatchMilliseconds.Add($BatchElapsedMilliseconds)
        $BatchIterationCounts.Add($BatchCompletedIterations)
    }
    if ($BatchCompletedIterations -gt 0 -and
        ($BatchIterationCounts.Count -eq 0 -or $BatchIterationCounts[-1] -ne $BatchCompletedIterations)) {
        $BatchMilliseconds.Add($BatchElapsedMilliseconds)
        $BatchIterationCounts.Add($BatchCompletedIterations)
    }
    $MeasurementStopwatch.Stop()
    if ($CompletedIterations -eq 0) {
        throw "measurement budget ended before one benchmark iteration"
    }

    $MedianBatchMilliseconds = Get-Median $BatchMilliseconds
    $MedianSearchMilliseconds = Get-Median $SearchMilliseconds
    $MedianTransitionMetrics = [ordered]@{}
    foreach ($MetricName in $TransitionMetricNames) {
        $MedianTransitionMetrics[$MetricName] = Get-Median $TransitionMetricSamples[$MetricName]
    }
    $GitCommit = (& git rev-parse HEAD).Trim()
    $GitDirty = -not [string]::IsNullOrWhiteSpace((& git status --porcelain) -join "`n")
    $Result = [ordered]@{
        schema_name = "CombatContractBenchmarkV1"
        schema_version = 1
        git_commit = $GitCommit
        git_dirty = $GitDirty
        build_git_commit = $BuildReceipt.git_commit
        build_git_dirty = $BuildReceipt.git_dirty
        build_source_fingerprint = $BuildReceipt.source_fingerprint
        executable_sha256 = $BuildReceipt.executable_sha256
        pdb_sha256 = $BuildReceipt.pdb_sha256
        case = $CasePath
        batches = $Batches
        iterations_per_batch = $IterationsPerBatch
        warmup_iterations = $WarmupIterations
        requested_iterations = $Batches * $IterationsPerBatch
        completed_iterations = $CompletedIterations
        measurement_budget_seconds = $MeasurementBudgetSeconds
        measurement_elapsed_seconds = [math]::Round($MeasurementStopwatch.Elapsed.TotalSeconds, 3)
        stopped_for_measurement_budget = $StoppedForBudget
        batch_milliseconds = @($BatchMilliseconds)
        batch_iteration_counts = @($BatchIterationCounts)
        median_batch_milliseconds = $MedianBatchMilliseconds
        median_iteration_milliseconds = Get-Median $IterationMilliseconds
        median_search_milliseconds = $MedianSearchMilliseconds
        median_ns_per_applied_transition = $MedianTransitionMetrics
        identity = $ExpectedIdentity
    }

    if ($AsJson) {
        $Result | ConvertTo-Json -Depth 100
    } else {
        [pscustomobject]@{
            commit = $GitCommit.Substring(0, [Math]::Min(8, $GitCommit.Length))
            dirty = $GitDirty
            build_source = $BuildReceipt.source_fingerprint.Substring(0, 12)
            executable = $BuildReceipt.executable_sha256.Substring(0, 12)
            batches = "$Batches x $IterationsPerBatch"
            batch_ms = (@($BatchMilliseconds | ForEach-Object { [Math]::Round($_, 2) }) -join ", ")
            median_process_ms = [Math]::Round($Result.median_iteration_milliseconds, 2)
            median_search_ms = [Math]::Round($Result.median_search_milliseconds, 2)
            transition_ns = @($TransitionMetricNames | ForEach-Object {
                "$_=$([Math]::Round($MedianTransitionMetrics[$_], 1))"
            }) -join ", "
            generation_work = $ExpectedIdentity.counters.generation_work
            transitions = $ExpectedIdentity.counters.applied_action_transitions
            exact_nodes = $ExpectedIdentity.counters.exact_nodes
            witness = "$($ExpectedIdentity.witness.final_hp) HP / $($ExpectedIdentity.witness.actions) actions"
        } | Format-List
    }
}
finally {
    Pop-Location
}
