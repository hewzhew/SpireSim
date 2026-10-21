<#
.SYNOPSIS
Runs compact combat_search_v2 turn-plan policy comparisons over benchmark manifests.

.EXAMPLE
.\tools\ml\run_turn_plan_policy_compare.ps1
Compares diagnostic_only against tactical_enemy_turn_boundary_frontier_seed for
all tools\artifacts\tmp\ml_capture_seed*\benchmark.json files.

.EXAMPLE
.\tools\ml\run_turn_plan_policy_compare.ps1 -Right root_frontier_seed -MaxNodes 5000
Runs the root frontier seed comparison and prints one row per benchmark plus an
aggregate row.
#>
param(
    [string] $BenchmarkRoot = "tools\artifacts\tmp",
    [string[]] $BenchmarkPath = @(),
    [string] $BenchmarkDirectoryPattern = "ml_capture_seed*",
    [string] $Left = "diagnostic_only",
    [string] $Right = "tactical_enemy_turn_boundary_frontier_seed",
    [int] $MaxNodes = 5000,
    [switch] $Build
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$WorkerExe = Join-Path $RepoRoot "target\release\combat_search_v2_worker.exe"

if ($Build -or -not (Test-Path -LiteralPath $WorkerExe)) {
    Write-Host "building the dedicated combat-search worker"
    cargo build --locked --release -p sts_combat_search_driver --features backend --bin combat_search_v2_worker
}

if ($BenchmarkPath.Count -eq 0) {
    $BenchmarkPath = Get-ChildItem -LiteralPath (Join-Path $RepoRoot $BenchmarkRoot) -Directory -Filter $BenchmarkDirectoryPattern |
        ForEach-Object { Join-Path $_.FullName "benchmark.json" } |
        Where-Object { Test-Path -LiteralPath $_ } |
        Sort-Object
}

if ($BenchmarkPath.Count -eq 0) {
    throw "No benchmark.json files found under $BenchmarkRoot matching $BenchmarkDirectoryPattern."
}

$Aggregate = [ordered]@{
    cases = 0
    left_better = 0
    right_better = 0
    tied = 0
    first_diff = 0
    hp_delta = 0
    nodes_delta = 0
    generated_delta = 0
    seeded = 0
    rollout_skips_delta = 0
}

Write-Host "turn-plan policy compare: benchmarks=$($BenchmarkPath.Count) left=$Left right=$Right max_nodes=$MaxNodes"

foreach ($Benchmark in $BenchmarkPath) {
    $BenchmarkFullPath = if ([System.IO.Path]::IsPathRooted($Benchmark)) {
        $Benchmark
    } else {
        Join-Path $RepoRoot $Benchmark
    }
    $Name = Split-Path (Split-Path $BenchmarkFullPath -Parent) -Leaf
    $Report = & $WorkerExe `
        --benchmark-spec $BenchmarkFullPath `
        --compare-turn-plan "$Left,$Right" `
        --max-nodes $MaxNodes |
        ConvertFrom-Json
    $Summary = $Report.summary

    $Aggregate.cases += [int]$Summary.cases_compared
    $Aggregate.left_better += [int]$Summary.left_better
    $Aggregate.right_better += [int]$Summary.right_better
    $Aggregate.tied += [int]$Summary.tied
    $Aggregate.first_diff += [int]$Summary.first_action_diff_cases
    $Aggregate.hp_delta += [int]$Summary.right_minus_left_final_hp_total
    $Aggregate.nodes_delta += [int]$Summary.right_minus_left_nodes_expanded_total
    $Aggregate.generated_delta += [int]$Summary.right_minus_left_nodes_generated_total
    $Aggregate.seeded += [int]$Summary.right_minus_left_turn_plan_frontier_seeded_nodes_total
    $Aggregate.rollout_skips_delta += [int]$Summary.right_minus_left_rollout_budget_skips_total

    Write-Host ("row seed_dir={0} cases={1} L={2} R={3} T={4} first_diff={5} hp_delta={6} nodes_delta={7} gen_delta={8} seeded={9} skips_delta={10}" -f `
        $Name,
        $Summary.cases_compared,
        $Summary.left_better,
        $Summary.right_better,
        $Summary.tied,
        $Summary.first_action_diff_cases,
        $Summary.right_minus_left_final_hp_total,
        $Summary.right_minus_left_nodes_expanded_total,
        $Summary.right_minus_left_nodes_generated_total,
        $Summary.right_minus_left_turn_plan_frontier_seeded_nodes_total,
        $Summary.right_minus_left_rollout_budget_skips_total)
}

Write-Host ("AGG cases={0} L={1} R={2} T={3} first_diff={4} hp_delta={5} nodes_delta={6} gen_delta={7} seeded={8} skips_delta={9}" -f `
    $Aggregate.cases,
    $Aggregate.left_better,
    $Aggregate.right_better,
    $Aggregate.tied,
    $Aggregate.first_diff,
    $Aggregate.hp_delta,
    $Aggregate.nodes_delta,
    $Aggregate.generated_delta,
    $Aggregate.seeded,
    $Aggregate.rollout_skips_delta)
