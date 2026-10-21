param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("configure", "doctor", "test", "verify", "check-bridge", "refresh-bridge", "baseline-info", "evaluate-baseline", "train-combat", "train-combat-recovery", "evaluate-combat", "evaluate-combat-potions", "audit-combat-policy", "compare-combat-paired", "evaluate-run", "evaluate-run-potions", "compare-run-paired", "probe-run-critic", "collect-run-roots", "train-run")]
    [string]$Command,
    [string]$Python,
    [string]$MaturinPython = "python",
    [string]$Artifact,
    [string]$Behavior,
    [string]$BaselineBehavior,
    [string]$CandidateBehavior,
    [string]$StrategicBehavior,
    [string]$CriticInitializationBehavior,
    [string]$Output,
    [int]$Roots,
    [int]$RootSlot = 0,
    [int[]]$DecisionOrdinals = @(),
    [int]$Replicates = 8,
    [int]$TraceReplicatesPerRoot = 0,
    [ValidateSet("sampled", "greedy")]
    [string]$CombatDecisionRule = "sampled",
    [int]$Updates,
    [ValidateSet("reinforce", "ppo-clip", "ppo-clip-value")]
    [string]$CombatPolicyUpdate = "reinforce",
    [ValidateSet("none", "enemy-hp-progress")]
    [string]$CombatAllLossAxis = "none",
    [ValidateSet("reinforce", "ppo-clip-value", "critic-calibration")]
    [string]$RunPolicyUpdate = "reinforce",
    [ValidateSet("auto", "on", "off")]
    [string]$RunAdvantageNormalization = "auto",
    [long]$ModelSeed = 0,
    [long]$BehaviorSeedBase = 1000,
    [double]$CombatLearningRate = 0.001,
    [int]$SourceExpectedRoots = 1,
    [int]$SourceRootSlot = 0,
    [int]$Slots = 4,
    [int]$Attempts = 8,
    [int]$MaxBatchSteps = 4096,
    [long]$BehaviorSeed = 10000,
    [ValidateRange(0, 20)]
    [Nullable[int]]$Ascension,
    [long]$HeldOutSeedStart = 1000000,
    [int]$Generations = 1,
    [int]$AttemptsPerUpdate = 8,
    [long]$TrainingSeedStart = 0,
    [long]$RootSeedStart = 0,
    [int]$EvaluationAttempts = 16,
    [int]$EvaluationMaxBatchSteps = 4096,
    [long]$EvaluationBehaviorSeed = 100000,
    [int]$ProbeTrainAttempts = 24,
    [int]$ProbeHeldOutAttempts = 8,
    [int]$ProbeHeadFitSteps = 256,
    [double]$ProbeHeadFitLearningRate = 0.001,
    [int]$CriticFitSteps = 256,
    [int]$WallMs = 60000,
    [int]$MinFloor = 2,
    [Nullable[int]]$MaxFloor,
    [Nullable[int]]$RequiredPriorCombats,
    [ValidateRange(0, 100)]
    [int]$MinHpPercent = 0,
    [int]$MinUsablePotions = 1,
    [string]$CombatFightClass = "any",
    [int]$MaxArtifactBytes = 16777216,
    [string]$RequiredPotionId,
    [Nullable[int]]$RequiredPotionSlot,
    [ValidateSet("auto", "raw-return", "leave-one-out", "matched-floor", "matched-floor-context", "matched-episode-floor-context", "decision-local-gae")]
    [string]$AdvantageMode = "auto",
    [ValidateSet("independent-cohorts", "episode-root-retries")]
    [string]$SamplingMode = "independent-cohorts",
    [Nullable[int]]$EpisodeRootAttempts,
    [ValidateSet("all", "strategic")]
    [string]$DecisionScope = "all",
    [ValidateSet("all", "never", "root-slots")]
    [string]$PotionLane = "all",
    [ValidateSet("trained", "all", "never")]
    [string]$RunPotionLane = "trained",
    [ValidateSet("cpu", "cuda")]
    [string]$Device = "cpu",
    [int[]]$PotionSlots = @(),
    [string]$RequiredEncounterId,
    [switch]$DistinctEncounters,
    [string[]]$EncounterQuota = @(),
    [ValidateSet("training", "held-out")]
    [string]$RootSeedPartition = "training",
    [int]$RootHeldOutNumerator = 1,
    [int]$RootPartitionDenominator = 10,
    [ValidateSet("dev", "release")]
    [string]$BridgeProfile = "release"
)

$ErrorActionPreference = "Stop"

$learningRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $learningRoot "..")).Path
$sourceRoot = Join-Path $learningRoot "src"
$testRoot = Join-Path $learningRoot "tests"
$hostRoot = Join-Path $repositoryRoot ".oracle-lab\hosts"
$pythonFile = Join-Path $hostRoot "learning-python.txt"
$reportRoot = Join-Path $repositoryRoot ".oracle-lab\reports"
$pytestRoot = Join-Path $repositoryRoot ".oracle-lab\pytest"
$pytestCacheRoot = Join-Path $pytestRoot "cache\learning"
$pytestRunRoot = Join-Path $pytestRoot "runs"
$potionArguments = @("--potion-lane", $PotionLane)
foreach ($potionSlot in $PotionSlots) {
    $potionArguments += @("--potion-slot", $potionSlot)
}

function Resolve-PythonExecutable([string]$Candidate) {
    if (-not $Candidate) {
        throw "missing -Python <python-3.12-or-newer-with-numpy-torch-and-sts_learning_bridge>"
    }
    if (-not (Test-Path -LiteralPath $Candidate -PathType Leaf)) {
        throw "Python executable does not exist: $Candidate"
    }
    return (Resolve-Path -LiteralPath $Candidate).Path
}

function Get-ConfiguredPython {
    if (-not (Test-Path -LiteralPath $pythonFile -PathType Leaf)) {
        throw "learning Python is not configured; run: .\learning\dev.ps1 configure -Python <python.exe>"
    }
    return Resolve-PythonExecutable ((Get-Content -LiteralPath $pythonFile -Raw).Trim())
}

function Invoke-WithLearningPath([scriptblock]$Body) {
    $savedPythonPath = [Environment]::GetEnvironmentVariable("PYTHONPATH", "Process")
    $env:PYTHONPATH = "$sourceRoot;$repositoryRoot"
    try {
        & $Body
    }
    finally {
        [Environment]::SetEnvironmentVariable("PYTHONPATH", $savedPythonPath, "Process")
    }
}

function Install-TestDependencies([string]$PythonPath) {
    $projectFile = Join-Path $learningRoot "pyproject.toml"
    $requirements = @(& $PythonPath -c @'
import pathlib
import sys
import tomllib

project = tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
requirements = project["project"]["optional-dependencies"]["test"]
if not requirements:
    raise SystemExit("sts-learning test extra is empty")
for requirement in requirements:
    print(requirement)
'@ $projectFile)
    if ($LASTEXITCODE -ne 0 -or $requirements.Count -eq 0) {
        throw "failed to resolve sts-learning test dependencies"
    }
    & $PythonPath -m pip install `
        --disable-pip-version-check `
        @requirements
    if ($LASTEXITCODE -ne 0) {
        throw "failed to install sts-learning test dependencies"
    }
}

function Invoke-Doctor([string]$PythonPath) {
    $doctor = @'
import pathlib
import sys

if sys.version_info[:2] < (3, 12):
    raise SystemExit(f"expected Python >=3.12, got {sys.version.split()[0]}")

import numpy
try:
    import pytest
except ModuleNotFoundError as error:
    raise SystemExit(
        "pytest is missing; rerun .\\learning\\dev.ps1 configure "
        "-Python <python.exe>"
    ) from error
import torch
import sts_learning
import sts_learning_bridge
from sts_learning_bridge import CombatLearningBatchEnv, LearningBatchEnv

source_root = pathlib.Path(sys.argv[1]).resolve()
package_root = pathlib.Path(sts_learning.__file__).resolve()
if source_root not in package_root.parents:
    raise SystemExit(f"sts_learning came from unexpected path: {package_root}")

required_bridge_methods = (
    "from_combat_root_artifact_bytes",
    "merge_combat_root_artifact_bytes",
    "supported_potion_ids",
    "combat_root_artifact_bytes",
    "combat_root_audit",
    "strategic_decision_audit_json",
    "combat_group",
    "combat_root_contexts",
)
missing = [
    name
    for name in required_bridge_methods
    if not callable(getattr(LearningBatchEnv, name, None))
]
if missing:
    raise SystemExit(
        "installed bridge is stale; missing LearningBatchEnv methods: "
        + ", ".join(missing)
        + "; run: .\\learning\\dev.ps1 refresh-bridge [-Python <python.exe>]"
    )

required_combat_group_methods = (
    "capture_recovery_root",
    "combat_decision_audit_json",
    "decision_progress",
)
missing = [
    name
    for name in required_combat_group_methods
    if not callable(getattr(CombatLearningBatchEnv, name, None))
]
if missing:
    raise SystemExit(
        "installed bridge is stale; missing CombatLearningBatchEnv methods: "
        + ", ".join(missing)
        + "; run: .\\learning\\dev.ps1 refresh-bridge [-Python <python.exe>]"
    )

print(f"python={sys.executable}")
print(f"numpy={numpy.__version__}")
print(f"pytest={pytest.__version__}")
print(f"torch={torch.__version__}")
print(f"bridge={sts_learning_bridge.__file__}")
print(f"learning={package_root}")
'@
    Invoke-WithLearningPath {
        & $PythonPath -c $doctor $sourceRoot
        if ($LASTEXITCODE -ne 0) {
            throw "learning runtime doctor failed"
        }
    }
}

function Invoke-LearningTests([string]$PythonPath) {
    Invoke-Doctor $PythonPath
    New-Item -ItemType Directory -Path $reportRoot -Force | Out-Null
    $runId = (Get-Date -Format "yyyyMMdd-HHmmss") + "-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
    $log = Join-Path $reportRoot "learning-tests-$runId.log"
    $baseTemp = Join-Path $pytestRunRoot $runId
    New-Item -ItemType Directory -Path $pytestCacheRoot -Force | Out-Null
    New-Item -ItemType Directory -Path $baseTemp -Force | Out-Null
    $savedErrorPreference = $ErrorActionPreference
    Push-Location $repositoryRoot
    try {
        $ErrorActionPreference = "Continue"
        Invoke-WithLearningPath {
            & $PythonPath -m pytest `
                $testRoot `
                -q `
                --basetemp $baseTemp `
                -o "cache_dir=$pytestCacheRoot" `
                *> $log
            $script:testExit = $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
        $ErrorActionPreference = $savedErrorPreference
    }
    if ($script:testExit -ne 0) {
        Get-Content -LiteralPath $log -Tail 80
        throw "learning tests failed; full log: $log"
    }
    $summary = Get-Content -LiteralPath $log | Where-Object { $_.Trim() } | Select-Object -Last 1
    Remove-Item -LiteralPath $baseTemp -Recurse -Force
    Remove-Item -LiteralPath $log -Force
    Write-Output $summary
    Write-Output "learning_tests=passed"
    Write-Output "success_artifacts=cleaned"
}

switch ($Command) {
    "configure" {
        $pythonPath = Resolve-PythonExecutable $Python
        Install-TestDependencies $pythonPath
        Invoke-Doctor $pythonPath
        New-Item -ItemType Directory -Path $hostRoot -Force | Out-Null
        Set-Content -LiteralPath $pythonFile -Value $pythonPath -NoNewline
        Write-Output ("configured=" + $pythonPath)
    }
    "doctor" {
        Invoke-Doctor (Get-ConfiguredPython)
    }
    "test" {
        Invoke-LearningTests (Get-ConfiguredPython)
    }
    "baseline-info" {
        $pythonPath = Get-ConfiguredPython
        Invoke-WithLearningPath {
            & $pythonPath -c "import json; from sts_learning_bridge import public_legality_bootstrap_manifest as manifest; print(json.dumps(manifest(), indent=2))"
            if ($LASTEXITCODE -ne 0) {
                throw "public baseline manifest query failed; run refresh-bridge"
            }
        }
    }
    "evaluate-baseline" {
        if ($null -eq $Ascension) {
            throw "evaluate-baseline requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.evaluate_public_baseline `
                --output $Output `
                --ascension $Ascension `
                --seed-start $HeldOutSeedStart `
                --attempts $Attempts `
                --slots $Slots `
                --max-batch-steps $MaxBatchSteps
            if ($LASTEXITCODE -ne 0) {
                throw "public baseline evaluation command failed"
            }
        }
    }
    "train-combat" {
        $pythonPath = Get-ConfiguredPython
        $warmStartArguments = @()
        if ($Behavior) {
            $warmStartArguments = @("--warm-start-behavior", $Behavior)
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.train_combat `
                --artifact $Artifact `
                --output $Output `
                --roots $Roots `
                --replicates $Replicates `
                --updates $Updates `
                --model-seed $ModelSeed `
                --behavior-seed-base $BehaviorSeedBase `
                --learning-rate $CombatLearningRate `
                --policy-update $CombatPolicyUpdate `
                --all-loss-axis $CombatAllLossAxis `
                @warmStartArguments `
                @potionArguments
            if ($LASTEXITCODE -ne 0) {
                throw "combat training command failed"
            }
        }
    }
    "train-combat-recovery" {
        $pythonPath = Get-ConfiguredPython
        $warmStartArguments = @()
        if ($Behavior) {
            $warmStartArguments = @("--warm-start-behavior", $Behavior)
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.train_combat_recovery `
                --artifact $Artifact `
                --output $Output `
                --roots $Roots `
                --replicates $Replicates `
                --updates $Updates `
                --model-seed $ModelSeed `
                --behavior-seed-base $BehaviorSeedBase `
                --learning-rate $CombatLearningRate `
                --source-expected-roots $SourceExpectedRoots `
                --source-root-slot $SourceRootSlot `
                --policy-update $CombatPolicyUpdate `
                @warmStartArguments `
                @potionArguments
            if ($LASTEXITCODE -ne 0) {
                throw "combat recovery training command failed"
            }
        }
    }
    "evaluate-combat" {
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.evaluate_combat `
                --artifact $Artifact `
                --behavior $Behavior `
                --output $Output `
                --roots $Roots `
                --replicates $Replicates `
                --trace-replicates-per-root $TraceReplicatesPerRoot `
                --decision-rule $CombatDecisionRule `
                --behavior-seed-base $BehaviorSeedBase `
                @potionArguments
            if ($LASTEXITCODE -ne 0) {
                throw "combat evaluation command failed"
            }
        }
    }
    "evaluate-combat-potions" {
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.evaluate_combat_potions `
                --artifact $Artifact `
                --behavior $Behavior `
                --output $Output `
                --roots $Roots `
                --replicates $Replicates `
                --behavior-seed-base $BehaviorSeedBase
            if ($LASTEXITCODE -ne 0) {
                throw "combat potion sweep command failed"
            }
        }
    }
    "audit-combat-policy" {
        $pythonPath = Get-ConfiguredPython
        $auditPotionLane = if ($PSBoundParameters.ContainsKey("PotionLane")) {
            $PotionLane
        }
        else {
            "never"
        }
        $auditPotionArguments = @("--potion-lane", $auditPotionLane)
        foreach ($potionSlot in $PotionSlots) {
            $auditPotionArguments += @("--potion-slot", $potionSlot)
        }
        $auditDecisionArguments = @()
        foreach ($decisionOrdinal in $DecisionOrdinals) {
            $auditDecisionArguments += @("--decision-ordinal", $decisionOrdinal)
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.fixed_combat_policy_audit `
                --artifact $Artifact `
                --baseline-behavior $BaselineBehavior `
                --candidate-behavior $CandidateBehavior `
                --output $Output `
                --roots $Roots `
                --root-slot $RootSlot `
                @auditDecisionArguments `
                @auditPotionArguments
            if ($LASTEXITCODE -ne 0) {
                throw "fixed combat policy audit command failed"
            }
        }
    }
    "compare-combat-paired" {
        $pythonPath = Get-ConfiguredPython
        $pairDecisionRule = if ($PSBoundParameters.ContainsKey("CombatDecisionRule")) {
            $CombatDecisionRule
        }
        else {
            "greedy"
        }
        $pairPotionLane = if ($PSBoundParameters.ContainsKey("PotionLane")) {
            $PotionLane
        }
        else {
            "never"
        }
        $pairPotionArguments = @("--potion-lane", $pairPotionLane)
        foreach ($potionSlot in $PotionSlots) {
            $pairPotionArguments += @("--potion-slot", $potionSlot)
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.paired_combat_compare `
                --artifact $Artifact `
                --baseline-behavior $BaselineBehavior `
                --candidate-behavior $CandidateBehavior `
                --output $Output `
                --roots $Roots `
                --replicates $Replicates `
                --behavior-seed-base $BehaviorSeedBase `
                --decision-rule $pairDecisionRule `
                @pairPotionArguments
            if ($LASTEXITCODE -ne 0) {
                throw "paired combat comparison command failed"
            }
        }
    }
    "evaluate-run" {
        if ($null -eq $Ascension) {
            throw "evaluate-run requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.evaluate_run `
                --behavior $Behavior `
                --output $Output `
                --slots 1 `
                --attempts $Attempts `
                --max-batch-steps $MaxBatchSteps `
                --behavior-seed $BehaviorSeed `
                --ascension $Ascension `
                --held-out-seed-start $HeldOutSeedStart `
                --potion-lane $RunPotionLane
            if ($LASTEXITCODE -ne 0) {
                throw "run evaluation command failed"
            }
        }
    }
    "evaluate-run-potions" {
        if ($null -eq $Ascension) {
            throw "evaluate-run-potions requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.evaluate_run_potions `
                --behavior $Behavior `
                --output $Output `
                --attempts $Attempts `
                --max-batch-steps $MaxBatchSteps `
                --behavior-seed $BehaviorSeed `
                --ascension $Ascension `
                --held-out-seed-start $HeldOutSeedStart
            if ($LASTEXITCODE -ne 0) {
                throw "run potion comparison command failed"
            }
        }
    }
    "compare-run-paired" {
        if ($null -eq $Ascension) {
            throw "compare-run-paired requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        $pairedRunPotionLane = if ($PSBoundParameters.ContainsKey("RunPotionLane")) {
            $RunPotionLane
        }
        else {
            "never"
        }
        $pairedRunScopeArguments = @()
        if ($StrategicBehavior) {
            $pairedRunScopeArguments += @(
                "--strategic-behavior", $StrategicBehavior
            )
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.paired_run_compare `
                --baseline-behavior $BaselineBehavior `
                --candidate-behavior $CandidateBehavior `
                --output $Output `
                --attempts $Attempts `
                --max-batch-steps $MaxBatchSteps `
                --behavior-seed $BehaviorSeed `
                --ascension $Ascension `
                --held-out-seed-start $HeldOutSeedStart `
                --potion-lane $pairedRunPotionLane `
                @pairedRunScopeArguments
            if ($LASTEXITCODE -ne 0) {
                throw "paired run comparison command failed"
            }
        }
    }
    "probe-run-critic" {
        if ($null -eq $Ascension) {
            throw "probe-run-critic requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.run_critic_probe `
                --behavior $Behavior `
                --output $Output `
                --ascension $Ascension `
                --train-attempts $ProbeTrainAttempts `
                --held-out-attempts $ProbeHeldOutAttempts `
                --max-batch-steps $MaxBatchSteps `
                --behavior-seed $BehaviorSeed `
                --held-out-seed-start $HeldOutSeedStart `
                --head-fit-steps $ProbeHeadFitSteps `
                --head-fit-learning-rate $ProbeHeadFitLearningRate `
                --model-seed $ModelSeed `
                --potion-lane $RunPotionLane
            if ($LASTEXITCODE -ne 0) {
                throw "run critic probe command failed"
            }
        }
    }
    "collect-run-roots" {
        if ($null -eq $Ascension) {
            throw "collect-run-roots requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        $selectorArguments = @()
        $captureArguments = @()
        $rootArguments = @()
        $collectorScopeArguments = @()
        if ($StrategicBehavior) {
            $collectorScopeArguments += @(
                "--strategic-behavior", $StrategicBehavior
            )
        }
        if ($null -ne $RequiredPriorCombats) {
            $captureArguments += @(
                "--required-prior-combats", $RequiredPriorCombats
            )
        }
        if ($null -ne $MaxFloor) {
            $captureArguments += @("--max-floor", $MaxFloor)
        }
        if ($RequiredPotionId -or $null -ne $RequiredPotionSlot) {
            if (-not $RequiredPotionId -or $null -eq $RequiredPotionSlot) {
                throw "collect-run-roots requires both -RequiredPotionId and -RequiredPotionSlot"
            }
            $selectorArguments = @(
                "--required-potion-id", $RequiredPotionId,
                "--required-potion-slot", $RequiredPotionSlot
            )
        }
        if ($DistinctEncounters) {
            $selectorArguments += @("--distinct-encounters")
        }
        if ($RequiredEncounterId) {
            $selectorArguments += @(
                "--required-encounter-id", $RequiredEncounterId
            )
        }
        if ($EncounterQuota.Count -gt 0) {
            if ($DistinctEncounters -or $RequiredEncounterId) {
                throw "collect-run-roots encounter quotas cannot be combined with another encounter selector"
            }
            foreach ($quota in $EncounterQuota) {
                $selectorArguments += @("--encounter-quota", $quota)
            }
            if ($Roots -gt 0) {
                $rootArguments = @("--roots", $Roots)
            }
        } else {
            if ($Roots -le 0) {
                throw "collect-run-roots requires -Roots or at least one -EncounterQuota"
            }
            $rootArguments = @("--roots", $Roots)
        }
        $collectorCombatDecisionRule = if ($PSBoundParameters.ContainsKey("CombatDecisionRule")) {
            $CombatDecisionRule
        } else {
            "greedy"
        }
        $collectorSeedPartition = $RootSeedPartition.Replace("-", "_")
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.collect_run_combat_roots `
                --behavior $Behavior `
                @collectorScopeArguments `
                --output $Output `
                @rootArguments `
                --max-batch-steps $MaxBatchSteps `
                --wall-ms $WallMs `
                --behavior-seed $BehaviorSeed `
                --seed-start $RootSeedStart `
                --seed-partition $collectorSeedPartition `
                --held-out-numerator $RootHeldOutNumerator `
                --partition-denominator $RootPartitionDenominator `
                --ascension $Ascension `
                --combat-decision-rule $collectorCombatDecisionRule `
                --min-floor $MinFloor `
                --min-hp-percent $MinHpPercent `
                --min-usable-potions $MinUsablePotions `
                --fight-class $CombatFightClass `
                --max-artifact-bytes $MaxArtifactBytes `
                --potion-lane $RunPotionLane `
                @captureArguments `
                @selectorArguments
            if ($LASTEXITCODE -ne 0) {
                throw "run combat-root collection failed"
            }
        }
    }
    "train-run" {
        if ($null -eq $Ascension) {
            throw "train-run requires -Ascension 0..20"
        }
        $pythonPath = Get-ConfiguredPython
        $episodeRootArguments = @()
        if ($null -ne $EpisodeRootAttempts) {
            $episodeRootArguments = @(
                "--episode-root-attempts",
                $EpisodeRootAttempts
            )
        }
        $criticInitializationArguments = @()
        if ($CriticInitializationBehavior) {
            $criticInitializationArguments = @(
                "--critic-initialization-behavior",
                $CriticInitializationBehavior
            )
        }
        Invoke-Doctor $pythonPath
        Invoke-WithLearningPath {
            & $pythonPath -m sts_learning.train_run `
                --warm-start-behavior $Behavior `
                @criticInitializationArguments `
                --output $Output `
                --slots $Slots `
                --generations $Generations `
                --attempts-per-update $AttemptsPerUpdate `
                --max-batch-steps $MaxBatchSteps `
                --model-seed $ModelSeed `
                --behavior-seed $BehaviorSeed `
                --training-seed-start $TrainingSeedStart `
                --evaluation-attempts $EvaluationAttempts `
                --evaluation-max-batch-steps $EvaluationMaxBatchSteps `
                --evaluation-behavior-seed $EvaluationBehaviorSeed `
                --held-out-seed-start $HeldOutSeedStart `
                --ascension $Ascension `
                --advantage-mode $AdvantageMode `
                --decision-scope $DecisionScope `
                --combat-decision-rule $CombatDecisionRule `
                --run-policy-update $RunPolicyUpdate `
                --run-advantage-normalization $RunAdvantageNormalization `
                --critic-fit-steps $CriticFitSteps `
                --sampling-mode $SamplingMode `
                @episodeRootArguments `
                --potion-lane $RunPotionLane `
                --device $Device
            if ($LASTEXITCODE -ne 0) {
                throw "run training command failed"
            }
        }
    }
    "verify" {
        $pythonPath = Get-ConfiguredPython
        Invoke-LearningTests $pythonPath
        & (Join-Path $repositoryRoot "bindings\python_learning\verify.ps1") `
            -Python $pythonPath `
            -MaturinPython $MaturinPython
        if ($LASTEXITCODE -ne 0) {
            throw "isolated bridge verification failed"
        }
    }
    "check-bridge" {
        $pythonPath = if ($Python) {
            Resolve-PythonExecutable $Python
        }
        else {
            Get-ConfiguredPython
        }
        & (Join-Path $repositoryRoot "bindings\python_learning\verify.ps1") `
            -Python $pythonPath `
            -MaturinPython $MaturinPython `
            -Fast
        if ($LASTEXITCODE -ne 0) {
            throw "isolated dev-profile bridge check failed"
        }
    }
    "refresh-bridge" {
        $pythonPath = if ($Python) {
            Resolve-PythonExecutable $Python
        }
        else {
            Get-ConfiguredPython
        }
        if ($Python) {
            Install-TestDependencies $pythonPath
        }
        & (Join-Path $repositoryRoot "bindings\python_learning\verify.ps1") `
            -Python $pythonPath `
            -MaturinPython $MaturinPython `
            -InstallTarget `
            -Fast:($BridgeProfile -eq "dev")
        if ($LASTEXITCODE -ne 0) {
            throw "learning bridge refresh failed"
        }
        Invoke-Doctor $pythonPath
        if ($Python) {
            New-Item -ItemType Directory -Path $hostRoot -Force | Out-Null
            Set-Content -LiteralPath $pythonFile -Value $pythonPath -NoNewline
            Write-Output ("configured=" + $pythonPath)
        }
    }
}
