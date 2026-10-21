<#
.SYNOPSIS
Generates turn-plan tactical episodes from combat benchmark manifests.

.EXAMPLE
.\tools\ml\run_tactical_trace_batch.ps1
Finds tools\artifacts\tmp\ml_capture_seed*\benchmark.json, writes tactical
labs and enriched CombatTacticalEpisodeV1 JSONL files under
tools\artifacts\tmp\current_tactical_batch.

.EXAMPLE
.\tools\ml\run_tactical_trace_batch.ps1 -RunBaseline
Generates episodes, then runs the compact ranking baseline over the generated
episode JSONL files.
#>
[CmdletBinding(PositionalBinding=$false)]
param(
    [string] $BenchmarkRoot = "tools\artifacts\tmp",
    [string[]] $BenchmarkPath = @(),
    [string] $OutputRoot = "tools\artifacts\tmp\current_tactical_batch",
    [string] $BenchmarkDirectoryPattern = "ml_capture_seed*",
    [int] $GuidanceLabMaxCases = 100,
    [int] $SearchMaxNodes = 1000,
    [int] $ProbeMaxNodes = 256,
    [Nullable[int]] $TurnPlanProbeMaxInnerNodes = $null,
    [Nullable[int]] $TurnPlanProbeMaxEndStates = $null,
    [Nullable[int]] $TurnPlanProbePerBucketLimit = $null,
    [switch] $Build,
    [switch] $RunBaseline,
    [ValidateSet("source", "group", "source-cv")]
    [string] $SplitMode = "source-cv",
    [ValidateSet("root-delta", "action-shape", "target-detail", "enemy-slot-context", "tactical-summary", "action-facts")]
    [string[]] $FeatureGroups = @("tactical-summary", "action-facts"),
    [int] $Epochs = 10,
    [ValidateSet("selected", "equivalent-hp-outcome")]
    [string] $TargetMode = "selected",
    [ValidateSet("binary", "pairwise-utility", "decomposed-utility")]
    [string] $TrainingMode = "decomposed-utility",
    [switch] $CompareFeatureGroups,
    [switch] $CompareTargetModes,
    [switch] $CompactOutput,
    [switch] $CleanOutput
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$WorkerExe = Join-Path $RepoRoot "target\release\combat_search_v2_worker.exe"
$Extractor = Join-Path $RepoRoot "tools\ml\combat_tactical_trace_extract.py"
$Baseline = Join-Path $RepoRoot "tools\ml\combat_first_action_ranking_baseline.py"
$OutputDir = Join-Path $RepoRoot $OutputRoot

if ($Build -or -not (Test-Path -LiteralPath $WorkerExe)) {
    Write-Host "building the dedicated combat-search worker"
    cargo build --locked --release -p sts_combat_search_driver --features backend --bin combat_search_v2_worker
}

if ($CleanOutput -and (Test-Path -LiteralPath $OutputDir)) {
    Remove-Item -LiteralPath $OutputDir -Recurse -Force
}
New-Item -ItemType Directory -Force $OutputDir | Out-Null

if ($BenchmarkPath.Count -eq 0) {
    $BenchmarkPath = Get-ChildItem -LiteralPath (Join-Path $RepoRoot $BenchmarkRoot) -Directory -Filter $BenchmarkDirectoryPattern |
        ForEach-Object { Join-Path $_.FullName "benchmark.json" } |
        Where-Object { Test-Path -LiteralPath $_ } |
        Sort-Object
}

if ($BenchmarkPath.Count -eq 0) {
    throw "No benchmark.json files found under $BenchmarkRoot matching $BenchmarkDirectoryPattern."
}

Write-Host "tactical trace batch: benchmarks=$($BenchmarkPath.Count) output=$OutputRoot max_cases=$GuidanceLabMaxCases max_nodes=$SearchMaxNodes probe_nodes=$ProbeMaxNodes turn_plan_probe=max_inner:$TurnPlanProbeMaxInnerNodes max_end:$TurnPlanProbeMaxEndStates per_bucket:$TurnPlanProbePerBucketLimit"

$TurnPlanProbeArgs = @()
if ($null -ne $TurnPlanProbeMaxInnerNodes) {
    $TurnPlanProbeArgs += "--turn-plan-probe-max-inner-nodes"
    $TurnPlanProbeArgs += "$TurnPlanProbeMaxInnerNodes"
}
if ($null -ne $TurnPlanProbeMaxEndStates) {
    $TurnPlanProbeArgs += "--turn-plan-probe-max-end-states"
    $TurnPlanProbeArgs += "$TurnPlanProbeMaxEndStates"
}
if ($null -ne $TurnPlanProbePerBucketLimit) {
    $TurnPlanProbeArgs += "--turn-plan-probe-per-bucket-limit"
    $TurnPlanProbeArgs += "$TurnPlanProbePerBucketLimit"
}

$EpisodeFiles = @()
foreach ($Benchmark in $BenchmarkPath) {
    $BenchmarkFullPath = if ([System.IO.Path]::IsPathRooted($Benchmark)) {
        $Benchmark
    } else {
        Join-Path $RepoRoot $Benchmark
    }
    $Name = Split-Path (Split-Path $BenchmarkFullPath -Parent) -Leaf
    $LabPath = Join-Path $OutputDir "$Name.turn_plan_lab.json"
    $EpisodePath = Join-Path $OutputDir "$Name.enriched_tactical_episode.jsonl"

    Write-Host "[$Name] turn-plan lab"
    $DriverArgs = @(
        "--benchmark-spec", "$BenchmarkFullPath",
        "--turn-plan-guidance-lab",
        "--guidance-lab-max-cases", "$GuidanceLabMaxCases",
        "--max-nodes", "$SearchMaxNodes",
        "--probe-max-nodes", "$ProbeMaxNodes"
    ) + $TurnPlanProbeArgs + @("--output", "$LabPath")
    if ($CompactOutput) {
        $DriverOutput = & $WorkerExe @DriverArgs 2>&1
        if ($LASTEXITCODE -ne 0) {
            $DriverOutput | ForEach-Object { Write-Host $_ }
            throw "turn-plan lab failed for $Name"
        }
    } else {
        & $WorkerExe @DriverArgs
    }

    Write-Host "[$Name] tactical episode"
    $ExtractorArgs = @("$Extractor", "$LabPath", "--out-jsonl", "$EpisodePath", "--summary-only")
    if ($CompactOutput) {
        $ExtractorOutput = python @ExtractorArgs 2>&1
        if ($LASTEXITCODE -ne 0) {
            $ExtractorOutput | ForEach-Object { Write-Host $_ }
            throw "tactical episode extraction failed for $Name"
        }
        $EpisodeLine = $ExtractorOutput | Where-Object { $_ -match "^\s+episodes=" } | Select-Object -First 1
        $CoverageLine = $ExtractorOutput | Where-Object { $_ -match "^\s+root_legal_action_mask=" } | Select-Object -First 1
        if ($EpisodeLine) {
            Write-Host "[$Name] tactical episode done $($EpisodeLine.Trim())"
        }
        if ($CoverageLine) {
            Write-Host "[$Name] $($CoverageLine.Trim())"
        }
    } else {
        python @ExtractorArgs
    }
    $EpisodeFiles += $EpisodePath
}

if ($RunBaseline) {
    $BaselineSummaryPath = Join-Path $OutputDir "ranking_baseline_summary.json"
    $PriorHintsPath = Join-Path $OutputDir "root_action_prior_hints.jsonl"
    $ArgsList = @(
        "$Baseline",
        $EpisodeFiles,
        "--split-mode", "$SplitMode",
        "--epochs", "$Epochs",
        "--target-mode", "$TargetMode",
        "--training-mode", "$TrainingMode",
        "--summary-json-out", "$BaselineSummaryPath",
        "--prior-hints-jsonl-out", "$PriorHintsPath",
        "--report-mode", "compact"
    )
    if ($FeatureGroups.Count -gt 0) {
        $ArgsList += "--feature-groups"
        $ArgsList += $FeatureGroups
    }
    if ($CompareFeatureGroups) {
        $ArgsList += "--compare-feature-groups"
    }
    if ($CompareTargetModes) {
        $ArgsList += "--compare-target-modes"
    }
    Write-Host "ranking baseline: files=$($EpisodeFiles.Count) split=$SplitMode target=$TargetMode training=$TrainingMode features=$($FeatureGroups -join ',') epochs=$Epochs summary=$BaselineSummaryPath prior_hints=$PriorHintsPath"
    if ($CompactOutput) {
        $BaselineOutput = python @ArgsList 2>&1
        if ($LASTEXITCODE -ne 0) {
            $BaselineOutput | ForEach-Object { Write-Host $_ }
            throw "ranking baseline failed"
        }
        $Summary = Get-Content -LiteralPath $BaselineSummaryPath -Raw | ConvertFrom-Json
        $CoverageRatio = $Summary.root_action_mask_coverage.candidate_first_action_coverage_ratio
        $Metric = $Summary.metrics.logistic_source_cv
        if ($null -eq $Metric) {
            $Metric = $Summary.metrics.logistic_test
        }
        if ($null -ne $Metric) {
            Write-Host ("ranking summary: groups={0} readiness={1} candidate_first_action_ratio={2:N3} outcome_match={3:N3} hp_regret={4:N2} hp_gain_vs_ordered={5:N2}" -f `
                $Summary.usable_group_count, `
                $Summary.readiness, `
                $CoverageRatio, `
                $Metric.target_outcome_match_rate, `
                $Metric.avg_hp_regret_to_target, `
                $Metric.avg_hp_gain_vs_ordered)
        }
        if ($null -ne $Summary.target_mode_compare) {
            $Selected = $Summary.target_mode_compare.selected
            $Equivalent = $Summary.target_mode_compare.'equivalent-hp-outcome'
            if ($null -ne $Selected -and $null -ne $Equivalent) {
                Write-Host ("target compare: selected_hp_regret={0:N2} equivalent_hp_regret={1:N2} selected_outcome={2:N3} equivalent_outcome={3:N3}" -f `
                    $Selected.avg_hp_regret_to_target, `
                    $Equivalent.avg_hp_regret_to_target, `
                    $Selected.target_outcome_match_rate, `
                    $Equivalent.target_outcome_match_rate)
            }
        }
        if ($null -ne $Summary.best_target_mode_by_hp_regret) {
            $BestTarget = $Summary.best_target_mode_by_hp_regret
            Write-Host ("best target by hp_regret: mode={0} hp_regret={1:N2} outcome_match={2:N3}" -f `
                $BestTarget.target_mode, `
                $BestTarget.avg_hp_regret_to_target, `
                $BestTarget.target_outcome_match_rate)
        }
        if ($null -ne $Summary.prior_effect_decision) {
            $Decision = $Summary.prior_effect_decision
            $ConstraintText = ($Decision.constraints -join ",")
            if ([string]::IsNullOrWhiteSpace($ConstraintText)) {
                $ConstraintText = "-"
            }
            Write-Host ("prior effect: recommendation={0} constraints={1}" -f `
                $Decision.recommendation, `
                $ConstraintText)
        }
        if ($null -ne $Summary.prior_hints) {
            Write-Host ("prior hints: records={0} action_hints={1} path={2}" -f `
                $Summary.prior_hints.record_count, `
                $Summary.prior_hints.action_hint_count, `
                $Summary.prior_hints.path)
        }
    } else {
        python @ArgsList
    }
}
