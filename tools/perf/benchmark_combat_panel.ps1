<#
.SYNOPSIS
Benchmarks a small identity-locked panel of distinct exact combats.

.DESCRIPTION
Runs light, long-horizon, large-state, and replay-verified witness cases with
one shared work budget. The checked-in panel locks deterministic search
identity while timing remains observational. Source and artifact identity are
validated by the same build receipt as the single-case benchmark and WPR tool.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [string] $Panel = "tools\perf\combat_performance_panel.json",
    [ValidateRange(1, 10)]
    [int] $Batches = 2,
    [ValidateRange(1, 20)]
    [int] $IterationsPerCase = 3,
    [ValidateRange(0, 10)]
    [int] $WarmupIterations = 1,
    [ValidateSet("Auto", "Prepare", "Measure")]
    [string] $Phase = "Auto",
    [ValidateRange(0, 3600)]
    [int] $MeasurementBudgetSeconds = 45,
    [switch] $ProfileTransitionCloneCost,
    # Backward-compatible alias for -Phase Measure.
    [switch] $SkipBuild,
    [switch] $AsJson
)

$ErrorActionPreference = "Stop"
$ProfiledCombatInputKinds = @("play_card", "end_turn", "potion", "selection", "other")
$ProfiledCombatEnginePhases = @(
    "discard_hand",
    "monster_pre_turn",
    "monster_turn_setup",
    "monster_move_resolution",
    "monster_during_turn_powers",
    "monster_action_damage_route",
    "monster_action_power_route",
    "monster_action_card_route",
    "monster_action_spawn_route",
    "monster_action_orb_route",
    "monster_action_unhandled_route",
    "monster_end_round",
    "player_turn_start"
)
$SearchTimingFields = @(
    "selection_elapsed_ns",
    "generation_elapsed_ns",
    "admission_elapsed_ns",
    "atomic_expand_elapsed_ns",
    "transition_simulation_elapsed_ns",
    "transition_identity_elapsed_ns",
    "transition_key_build_elapsed_ns",
    "transition_key_index_elapsed_ns",
    "transition_admission_elapsed_ns",
    "transition_trace_elapsed_ns",
    "transition_seen_elapsed_ns",
    "transition_publish_elapsed_ns",
    "transition_publish_trace_node_elapsed_ns",
    "transition_publish_boundary_elapsed_ns",
    "transition_publish_complete_elapsed_ns",
    "transition_publish_push_elapsed_ns",
    "transition_publish_guide_elapsed_ns",
    "transition_publish_retain_elapsed_ns",
    "transition_publish_agenda_elapsed_ns",
    "admission_root_option_elapsed_ns",
    "admission_witness_filter_elapsed_ns",
    "admission_witness_replay_elapsed_ns",
    "successor_identity_elapsed_ns",
    "successor_lookup_elapsed_ns",
    "successor_node_build_elapsed_ns",
    "successor_edge_elapsed_ns",
    "successor_backup_elapsed_ns",
    "admission_refresh_elapsed_ns"
)

. (Join-Path $PSScriptRoot "combat_contract_build_receipt.ps1")

function Get-Median([double[]] $Values) {
    if ($Values.Count -eq 0) {
        return $null
    }
    $Sorted = @($Values | Sort-Object)
    $Middle = [int] [Math]::Floor($Sorted.Count / 2)
    if (($Sorted.Count % 2) -eq 1) {
        return $Sorted[$Middle]
    }
    return ($Sorted[$Middle - 1] + $Sorted[$Middle]) / 2.0
}

function Get-RoundedMedian([double[]] $Values, [int] $Digits) {
    $Median = Get-Median $Values
    if ($null -eq $Median) {
        return $null
    }
    return [math]::Round($Median, $Digits)
}

function New-InputProfileAccumulators([string[]] $Kinds) {
    $Profiles = [ordered]@{}
    foreach ($Kind in $Kinds) {
        $Profiles[$Kind] = [pscustomobject]@{
            samples = [Collections.Generic.List[double]]::new()
            sample_share = [Collections.Generic.List[double]]::new()
            execution_ns = [Collections.Generic.List[double]]::new()
            engine_steps = [Collections.Generic.List[double]]::new()
        }
    }
    return $Profiles
}

function Get-InputProfileMedians($Profiles, [string[]] $Kinds) {
    $Summary = [ordered]@{}
    foreach ($Kind in $Kinds) {
        $Profile = $Profiles[$Kind]
        $Summary[$Kind] = [ordered]@{
            samples = Get-RoundedMedian $Profile.samples 1
            sample_share = Get-RoundedMedian $Profile.sample_share 4
            execution_ns = Get-RoundedMedian $Profile.execution_ns 1
            engine_steps = Get-RoundedMedian $Profile.engine_steps 2
        }
    }
    return $Summary
}

function New-EnginePhaseProfileAccumulators([string[]] $Phases) {
    $Profiles = [ordered]@{}
    foreach ($PhaseName in $Phases) {
        $Profiles[$PhaseName] = [pscustomobject]@{
            occurrences = [Collections.Generic.List[double]]::new()
            execution_share = [Collections.Generic.List[double]]::new()
            elapsed_ns = [Collections.Generic.List[double]]::new()
        }
    }
    return $Profiles
}

function Get-EnginePhaseProfileMedians($Profiles, [string[]] $Phases) {
    $Summary = [ordered]@{}
    foreach ($PhaseName in $Phases) {
        $Profile = $Profiles[$PhaseName]
        $Summary[$PhaseName] = [ordered]@{
            occurrences = Get-RoundedMedian $Profile.occurrences 1
            execution_share = Get-RoundedMedian $Profile.execution_share 4
            elapsed_ns = Get-RoundedMedian $Profile.elapsed_ns 1
        }
    }
    return $Summary
}

function New-TimingAccumulators([string[]] $Fields) {
    $Timings = [ordered]@{}
    foreach ($Field in $Fields) {
        $Timings[$Field] = [Collections.Generic.List[double]]::new()
    }
    return $Timings
}

function Get-TimingMediansMilliseconds($Timings, [string[]] $Fields) {
    $Summary = [ordered]@{}
    foreach ($Field in $Fields) {
        $Name = $Field -replace '_elapsed_ns$', ''
        $Median = Get-Median $Timings[$Field]
        $Summary[$Name] = if ($null -eq $Median) { $null } else {
            [math]::Round($Median / 1000000.0, 3)
        }
    }
    return $Summary
}

function Invoke-CombatPanelCase(
    [string] $Executable,
    [string] $CasePath,
    [bool] $ProfileCloneCost
) {
    $Arguments = @(
        "--case", $CasePath,
        "--max-nodes", "20000",
        "--max-selections", "20000",
        "--wall-ms", "5000",
        "--max-potions-used", "2",
        "--improve-incumbent",
        "--typed-plan-guide",
        "--typed-plan-selection-timing",
        "--performance-only"
    )
    if ($ProfileCloneCost) {
        $Arguments += "--profile-transition-clone-cost"
    }
    $StartInfo = [Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = $Executable
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.RedirectStandardOutput = $true
    $StartInfo.RedirectStandardError = $true
    foreach ($Argument in $Arguments) {
        $StartInfo.ArgumentList.Add($Argument)
    }

    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $StartInfo
    $Stopwatch = [Diagnostics.Stopwatch]::StartNew()
    if (-not $Process.Start()) {
        throw "failed to start combat panel case '$CasePath'"
    }
    $StandardOutput = $Process.StandardOutput.ReadToEndAsync()
    $StandardError = $Process.StandardError.ReadToEndAsync()
    $Process.WaitForExit()
    $Stopwatch.Stop()
    $Raw = $StandardOutput.GetAwaiter().GetResult().Trim()
    $ErrorText = $StandardError.GetAwaiter().GetResult().Trim()
    if ($Process.ExitCode -ne 0) {
        throw "combat panel case failed with exit code $($Process.ExitCode)`n$ErrorText`n$Raw"
    }
    try {
        $Report = $Raw | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "combat panel case did not return valid JSON`n$Raw"
    }
    if ($Report.schema_name -ne "CombatCasePerformanceProfileV2" -or
        [int] $Report.schema_version -ne 2) {
        throw "combat panel case returned unsupported profile schema '$($Report.schema_name)' v$($Report.schema_version)"
    }
    return [pscustomobject]@{
        report = $Report
        process_milliseconds = $Stopwatch.Elapsed.TotalMilliseconds
    }
}

function Get-CombatPanelIdentity($Report) {
    return [ordered]@{
        status = $Report.status
        generation_work = $Report.counters.generation_work
        applied_action_transitions = $Report.counters.applied_action_transitions
        exact_nodes = $Report.counters.exact_nodes
        completed_turn_options = $Report.counters.completed_turn_options
        unique_successor_states = $Report.counters.unique_successor_states
        duplicate_exact_successors = $Report.counters.duplicate_exact_successors
        terminal_win_options = $Report.counters.terminal_win_options
        witness = if ($null -eq $Report.witness) {
            $null
        }
        else {
            [ordered]@{
                final_hp = $Report.witness.final_hp
                actions = $Report.witness.actions
            }
        }
    }
}

function Assert-CombatPanelIdentity($Case, $Report) {
    $Expected = $Case.expected | ConvertTo-Json -Depth 20 -Compress
    $Actual = Get-CombatPanelIdentity $Report | ConvertTo-Json -Depth 20 -Compress
    if ($Actual -cne $Expected) {
        throw "combat panel identity changed for '$($Case.name)'`nexpected: $Expected`nactual:   $Actual"
    }
}

$RepoRoot = [IO.Path]::GetFullPath(
    (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
)
$PanelPath = if ([IO.Path]::IsPathRooted($Panel)) {
    [IO.Path]::GetFullPath($Panel)
}
else {
    [IO.Path]::GetFullPath((Join-Path $RepoRoot $Panel))
}
if (-not (Test-Path -LiteralPath $PanelPath -PathType Leaf)) {
    throw "combat performance panel is missing at '$PanelPath'"
}
$PanelDefinition = Get-Content -LiteralPath $PanelPath -Raw | ConvertFrom-Json
if ($PanelDefinition.schema_name -ne "StsCombatPerformancePanelV1" -or
    [int] $PanelDefinition.schema_version -ne 1) {
    throw "unsupported combat performance panel at '$PanelPath'"
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
        throw "combat panel is not prepared: $($BuildStatus.reason); run once with -Phase Prepare"
    }
    if ($Phase -eq "Prepare" -or ($Phase -eq "Auto" -and -not $BuildStatus.valid)) {
        & cargo build --locked --profile profiling -p sts_combat_contract --bin combat_contract
        if ($LASTEXITCODE -ne 0) {
            throw "combat panel profiling build failed"
        }
        $BuildReceipt = Write-StsCombatContractBuildReceipt $RepoRoot $Executable
        $Preparation = [ordered]@{
            schema_name = "CombatPerformancePanelPreparationV1"
            schema_version = 1
            prepared = $true
            measurement_ran = $false
            requested_phase = $Phase
            previous_receipt_valid = [bool] $BuildStatus.valid
            previous_receipt_problem = $BuildStatus.reason
            build_source_fingerprint = $BuildReceipt.source_fingerprint
            executable_sha256 = $BuildReceipt.executable_sha256
            next_command = ".\tools\perf\benchmark_combat_panel.ps1 -Phase Measure"
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

    $Cases = @($PanelDefinition.cases | ForEach-Object {
            $Definition = $_
            $CasePath = if ([IO.Path]::IsPathRooted([string] $Definition.path)) {
                [IO.Path]::GetFullPath([string] $Definition.path)
            }
            else {
                [IO.Path]::GetFullPath((Join-Path $RepoRoot ([string] $Definition.path)))
            }
            if (-not (Test-Path -LiteralPath $CasePath -PathType Leaf)) {
                throw "combat panel case is missing at '$CasePath'"
            }
            [pscustomobject]@{
                definition = $Definition
                path = $CasePath
                process_ms = [Collections.Generic.List[double]]::new()
                search_ms = [Collections.Generic.List[double]]::new()
                search_timing_ns = New-TimingAccumulators $SearchTimingFields
                simulation_ns = [Collections.Generic.List[double]]::new()
                identity_ns = [Collections.Generic.List[double]]::new()
                key_build_ns = [Collections.Generic.List[double]]::new()
                key_engine_ns = [Collections.Generic.List[double]]::new()
                key_turn_ns = [Collections.Generic.List[double]]::new()
                key_meta_ns = [Collections.Generic.List[double]]::new()
                key_zones_ns = [Collections.Generic.List[double]]::new()
                key_monsters_ns = [Collections.Generic.List[double]]::new()
                key_powers_ns = [Collections.Generic.List[double]]::new()
                key_potions_ns = [Collections.Generic.List[double]]::new()
                key_queue_ns = [Collections.Generic.List[double]]::new()
                key_runtime_ns = [Collections.Generic.List[double]]::new()
                key_rng_ns = [Collections.Generic.List[double]]::new()
                key_player_ns = [Collections.Generic.List[double]]::new()
                publish_ns = [Collections.Generic.List[double]]::new()
                engine_clone_ns = [Collections.Generic.List[double]]::new()
                combat_clone_ns = [Collections.Generic.List[double]]::new()
                transition_execution_ns = [Collections.Generic.List[double]]::new()
                transition_execution_by_input = New-InputProfileAccumulators $ProfiledCombatInputKinds
                transition_execution_by_engine_phase = New-EnginePhaseProfileAccumulators $ProfiledCombatEnginePhases
                combat_meta_clone_ns = [Collections.Generic.List[double]]::new()
                combat_turn_clone_ns = [Collections.Generic.List[double]]::new()
                combat_zones_clone_ns = [Collections.Generic.List[double]]::new()
                combat_entities_clone_ns = [Collections.Generic.List[double]]::new()
                combat_engine_clone_ns = [Collections.Generic.List[double]]::new()
                combat_rng_clone_ns = [Collections.Generic.List[double]]::new()
                combat_runtime_clone_ns = [Collections.Generic.List[double]]::new()
                zone_draw_pile_clone_ns = [Collections.Generic.List[double]]::new()
                zone_hand_clone_ns = [Collections.Generic.List[double]]::new()
                zone_discard_pile_clone_ns = [Collections.Generic.List[double]]::new()
                zone_exhaust_pile_clone_ns = [Collections.Generic.List[double]]::new()
                zone_limbo_clone_ns = [Collections.Generic.List[double]]::new()
                zone_queued_cards_clone_ns = [Collections.Generic.List[double]]::new()
                entity_player_clone_ns = [Collections.Generic.List[double]]::new()
                entity_monsters_clone_ns = [Collections.Generic.List[double]]::new()
                entity_potions_clone_ns = [Collections.Generic.List[double]]::new()
                entity_power_db_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_colorless_pool_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_emitted_events_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_engine_diagnostics_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_pending_rewards_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_last_drawn_cards_clone_ns = [Collections.Generic.List[double]]::new()
                runtime_monster_protocol_clone_ns = [Collections.Generic.List[double]]::new()
                mean_emitted_events = [Collections.Generic.List[double]]::new()
                max_emitted_events = [Collections.Generic.List[double]]::new()
                mean_engine_diagnostics = [Collections.Generic.List[double]]::new()
                max_engine_diagnostics = [Collections.Generic.List[double]]::new()
                mean_monster_protocol = [Collections.Generic.List[double]]::new()
                max_monster_protocol = [Collections.Generic.List[double]]::new()
            }
        })
    $ProfiledTypeSizes = $null

    for ($Warmup = 0; $Warmup -lt $WarmupIterations; $Warmup++) {
        foreach ($Case in $Cases) {
            $Run = Invoke-CombatPanelCase $Executable $Case.path $ProfileTransitionCloneCost
            Assert-CombatPanelIdentity $Case.definition $Run.report
        }
    }
    $MeasurementStopwatch = [Diagnostics.Stopwatch]::StartNew()
    $CompletedIterationsPerCase = 0
    $StoppedForBudget = $false
    :BatchLoop for ($Batch = 0; $Batch -lt $Batches; $Batch++) {
        for ($Iteration = 0; $Iteration -lt $IterationsPerCase; $Iteration++) {
            if ($CompletedIterationsPerCase -gt 0 -and
                $MeasurementBudgetSeconds -gt 0 -and
                $MeasurementStopwatch.Elapsed.TotalSeconds -ge $MeasurementBudgetSeconds) {
                $StoppedForBudget = $true
                break BatchLoop
            }
            foreach ($Case in $Cases) {
                $Run = Invoke-CombatPanelCase $Executable $Case.path $ProfileTransitionCloneCost
                Assert-CombatPanelIdentity $Case.definition $Run.report
                $Case.process_ms.Add($Run.process_milliseconds)
                $Case.search_ms.Add($Run.report.search_elapsed_ns / 1000000.0)
                foreach ($TimingField in $SearchTimingFields) {
                    $Case.search_timing_ns[$TimingField].Add(
                        [double] $Run.report.timing_ns.$TimingField
                    )
                }
                $Case.simulation_ns.Add($Run.report.ns_per_applied_transition.simulation)
                $Case.identity_ns.Add($Run.report.ns_per_applied_transition.identity)
                $Case.key_build_ns.Add($Run.report.ns_per_applied_transition.key_build)
                if ($ProfileTransitionCloneCost) {
                    $KeyComponents = $Run.report.transition_clone_profile.mean_ns_per_sample.key_build_components
                    $Case.key_engine_ns.Add($KeyComponents.engine)
                    $Case.key_turn_ns.Add($KeyComponents.turn)
                    $Case.key_meta_ns.Add($KeyComponents.meta)
                    $Case.key_zones_ns.Add($KeyComponents.zones)
                    $Case.key_monsters_ns.Add($KeyComponents.monsters)
                    $Case.key_powers_ns.Add($KeyComponents.powers)
                    $Case.key_potions_ns.Add($KeyComponents.potions)
                    $Case.key_queue_ns.Add($KeyComponents.queue)
                    $Case.key_runtime_ns.Add($KeyComponents.runtime)
                    $Case.key_rng_ns.Add($KeyComponents.rng)
                    $Case.key_player_ns.Add($KeyComponents.player)
                }
                $Case.publish_ns.Add($Run.report.ns_per_applied_transition.publish)
                if ($ProfileTransitionCloneCost) {
                    $Profile = $Run.report.transition_clone_profile
                    if ($null -eq $Profile -or [int] $Profile.samples -le 0) {
                        throw "combat panel clone profile was not populated for '$($Case.definition.name)'"
                    }
                    if ($null -eq $ProfiledTypeSizes) {
                        $ProfiledTypeSizes = $Profile.type_size_bytes
                    }
                    $Case.engine_clone_ns.Add($Profile.mean_ns_per_sample.engine_clone)
                    $Case.combat_clone_ns.Add($Profile.mean_ns_per_sample.combat_clone)
                    $Case.transition_execution_ns.Add($Profile.mean_ns_per_sample.execution)
                    foreach ($InputKind in $ProfiledCombatInputKinds) {
                        $InputProfile = $Profile.execution_by_input.$InputKind
                        $Accumulator = $Case.transition_execution_by_input[$InputKind]
                        $Accumulator.samples.Add($InputProfile.samples)
                        $Accumulator.sample_share.Add($InputProfile.sample_share)
                        if ([int] $InputProfile.samples -gt 0) {
                            $Accumulator.execution_ns.Add($InputProfile.mean_execution_ns)
                            $Accumulator.engine_steps.Add($InputProfile.mean_engine_steps)
                        }
                    }
                    foreach ($PhaseName in $ProfiledCombatEnginePhases) {
                        $PhaseProfile = $Profile.execution_by_engine_phase.$PhaseName
                        $Accumulator = $Case.transition_execution_by_engine_phase[$PhaseName]
                        $Accumulator.occurrences.Add($PhaseProfile.occurrences)
                        $Accumulator.execution_share.Add($PhaseProfile.share_of_execution)
                        if ([int] $PhaseProfile.occurrences -gt 0) {
                            $Accumulator.elapsed_ns.Add($PhaseProfile.mean_ns_per_occurrence)
                        }
                    }
                    $Components = $Profile.mean_ns_per_sample.combat_clone_components
                    $Case.combat_meta_clone_ns.Add($Components.meta)
                    $Case.combat_turn_clone_ns.Add($Components.turn)
                    $Case.combat_zones_clone_ns.Add($Components.zones)
                    $Case.combat_entities_clone_ns.Add($Components.entities)
                    $Case.combat_engine_clone_ns.Add($Components.engine)
                    $Case.combat_rng_clone_ns.Add($Components.rng)
                    $Case.combat_runtime_clone_ns.Add($Components.runtime)
                    $ZoneComponents = $Profile.mean_ns_per_sample.zone_components
                    $Case.zone_draw_pile_clone_ns.Add($ZoneComponents.draw_pile)
                    $Case.zone_hand_clone_ns.Add($ZoneComponents.hand)
                    $Case.zone_discard_pile_clone_ns.Add($ZoneComponents.discard_pile)
                    $Case.zone_exhaust_pile_clone_ns.Add($ZoneComponents.exhaust_pile)
                    $Case.zone_limbo_clone_ns.Add($ZoneComponents.limbo)
                    $Case.zone_queued_cards_clone_ns.Add($ZoneComponents.queued_cards)
                    $EntityComponents = $Profile.mean_ns_per_sample.entity_components
                    $Case.entity_player_clone_ns.Add($EntityComponents.player)
                    $Case.entity_monsters_clone_ns.Add($EntityComponents.monsters)
                    $Case.entity_potions_clone_ns.Add($EntityComponents.potions)
                    $Case.entity_power_db_clone_ns.Add($EntityComponents.power_db)
                    $RuntimeComponents = $Profile.mean_ns_per_sample.runtime_components
                    $Case.runtime_colorless_pool_clone_ns.Add($RuntimeComponents.colorless_pool)
                    $Case.runtime_emitted_events_clone_ns.Add($RuntimeComponents.emitted_events)
                    $Case.runtime_engine_diagnostics_clone_ns.Add($RuntimeComponents.engine_diagnostics)
                    $Case.runtime_pending_rewards_clone_ns.Add($RuntimeComponents.pending_rewards)
                    $Case.runtime_last_drawn_cards_clone_ns.Add($RuntimeComponents.last_drawn_cards)
                    $Case.runtime_monster_protocol_clone_ns.Add($RuntimeComponents.monster_protocol)
                    $Lengths = $Profile.sampled_collection_lengths
                    $Case.mean_emitted_events.Add($Lengths.mean_emitted_events)
                    $Case.max_emitted_events.Add($Lengths.max_emitted_events)
                    $Case.mean_engine_diagnostics.Add($Lengths.mean_engine_diagnostics)
                    $Case.max_engine_diagnostics.Add($Lengths.max_engine_diagnostics)
                    $Case.mean_monster_protocol.Add($Lengths.mean_monster_protocol)
                    $Case.max_monster_protocol.Add($Lengths.max_monster_protocol)
                }
            }
            $CompletedIterationsPerCase++
        }
    }
    $MeasurementStopwatch.Stop()
    if ($CompletedIterationsPerCase -eq 0) {
        throw "measurement budget ended before one complete panel iteration"
    }

    $Rows = @($Cases | ForEach-Object {
            [pscustomobject]@{
                case = $_.definition.name
                process_ms = [math]::Round((Get-Median $_.process_ms), 2)
                search_ms = [math]::Round((Get-Median $_.search_ms), 2)
                search_phase_ms = Get-TimingMediansMilliseconds $_.search_timing_ns $SearchTimingFields
                simulation_ns = [math]::Round((Get-Median $_.simulation_ns), 1)
                identity_ns = [math]::Round((Get-Median $_.identity_ns), 1)
                key_build_ns = [math]::Round((Get-Median $_.key_build_ns), 1)
                key_build_components_ns = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        engine = [math]::Round((Get-Median $_.key_engine_ns), 1)
                        turn = [math]::Round((Get-Median $_.key_turn_ns), 1)
                        meta = [math]::Round((Get-Median $_.key_meta_ns), 1)
                        zones = [math]::Round((Get-Median $_.key_zones_ns), 1)
                        monsters = [math]::Round((Get-Median $_.key_monsters_ns), 1)
                        powers = [math]::Round((Get-Median $_.key_powers_ns), 1)
                        potions = [math]::Round((Get-Median $_.key_potions_ns), 1)
                        queue = [math]::Round((Get-Median $_.key_queue_ns), 1)
                        runtime = [math]::Round((Get-Median $_.key_runtime_ns), 1)
                        rng = [math]::Round((Get-Median $_.key_rng_ns), 1)
                        player = [math]::Round((Get-Median $_.key_player_ns), 1)
                    }
                } else { $null }
                publish_ns = [math]::Round((Get-Median $_.publish_ns), 1)
                engine_clone_ns = if ($ProfileTransitionCloneCost) {
                    [math]::Round((Get-Median $_.engine_clone_ns), 1)
                } else { $null }
                combat_clone_ns = if ($ProfileTransitionCloneCost) {
                    [math]::Round((Get-Median $_.combat_clone_ns), 1)
                } else { $null }
                transition_execution_ns = if ($ProfileTransitionCloneCost) {
                    [math]::Round((Get-Median $_.transition_execution_ns), 1)
                } else { $null }
                transition_execution_by_input = if ($ProfileTransitionCloneCost) {
                    Get-InputProfileMedians $_.transition_execution_by_input $ProfiledCombatInputKinds
                } else { $null }
                transition_execution_by_engine_phase = if ($ProfileTransitionCloneCost) {
                    Get-EnginePhaseProfileMedians $_.transition_execution_by_engine_phase $ProfiledCombatEnginePhases
                } else { $null }
                combat_clone_components_ns = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        meta = [math]::Round((Get-Median $_.combat_meta_clone_ns), 1)
                        turn = [math]::Round((Get-Median $_.combat_turn_clone_ns), 1)
                        zones = [math]::Round((Get-Median $_.combat_zones_clone_ns), 1)
                        entities = [math]::Round((Get-Median $_.combat_entities_clone_ns), 1)
                        engine = [math]::Round((Get-Median $_.combat_engine_clone_ns), 1)
                        rng = [math]::Round((Get-Median $_.combat_rng_clone_ns), 1)
                        runtime = [math]::Round((Get-Median $_.combat_runtime_clone_ns), 1)
                    }
                } else { $null }
                zone_clone_components_ns = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        draw_pile = [math]::Round((Get-Median $_.zone_draw_pile_clone_ns), 1)
                        hand = [math]::Round((Get-Median $_.zone_hand_clone_ns), 1)
                        discard_pile = [math]::Round((Get-Median $_.zone_discard_pile_clone_ns), 1)
                        exhaust_pile = [math]::Round((Get-Median $_.zone_exhaust_pile_clone_ns), 1)
                        limbo = [math]::Round((Get-Median $_.zone_limbo_clone_ns), 1)
                        queued_cards = [math]::Round((Get-Median $_.zone_queued_cards_clone_ns), 1)
                    }
                } else { $null }
                entity_clone_components_ns = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        player = [math]::Round((Get-Median $_.entity_player_clone_ns), 1)
                        monsters = [math]::Round((Get-Median $_.entity_monsters_clone_ns), 1)
                        potions = [math]::Round((Get-Median $_.entity_potions_clone_ns), 1)
                        power_db = [math]::Round((Get-Median $_.entity_power_db_clone_ns), 1)
                    }
                } else { $null }
                runtime_clone_components_ns = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        colorless_pool = [math]::Round((Get-Median $_.runtime_colorless_pool_clone_ns), 1)
                        emitted_events = [math]::Round((Get-Median $_.runtime_emitted_events_clone_ns), 1)
                        engine_diagnostics = [math]::Round((Get-Median $_.runtime_engine_diagnostics_clone_ns), 1)
                        pending_rewards = [math]::Round((Get-Median $_.runtime_pending_rewards_clone_ns), 1)
                        last_drawn_cards = [math]::Round((Get-Median $_.runtime_last_drawn_cards_clone_ns), 1)
                        monster_protocol = [math]::Round((Get-Median $_.runtime_monster_protocol_clone_ns), 1)
                    }
                } else { $null }
                sampled_collection_lengths = if ($ProfileTransitionCloneCost) {
                    [ordered]@{
                        mean_emitted_events = [math]::Round((Get-Median $_.mean_emitted_events), 1)
                        max_emitted_events = [math]::Round((Get-Median $_.max_emitted_events), 1)
                        mean_engine_diagnostics = [math]::Round((Get-Median $_.mean_engine_diagnostics), 1)
                        max_engine_diagnostics = [math]::Round((Get-Median $_.max_engine_diagnostics), 1)
                        mean_monster_protocol = [math]::Round((Get-Median $_.mean_monster_protocol), 1)
                        max_monster_protocol = [math]::Round((Get-Median $_.max_monster_protocol), 1)
                    }
                } else { $null }
                transitions = [int] $_.definition.expected.applied_action_transitions
                exact_nodes = [int] $_.definition.expected.exact_nodes
                witness_hp = if ($null -eq $_.definition.expected.witness) {
                    $null
                }
                else {
                    $_.definition.expected.witness.final_hp
                }
            }
        })
    $Result = [ordered]@{
        schema_name = "CombatPerformancePanelBenchmarkV2"
        schema_version = 2
        git_commit = (& git rev-parse HEAD).Trim()
        git_dirty = -not [string]::IsNullOrWhiteSpace((& git status --porcelain) -join "`n")
        build_source_fingerprint = $BuildReceipt.source_fingerprint
        executable_sha256 = $BuildReceipt.executable_sha256
        batches = $Batches
        iterations_per_case = $IterationsPerCase
        warmup_iterations = $WarmupIterations
        requested_iterations_per_case = $Batches * $IterationsPerCase
        completed_iterations_per_case = $CompletedIterationsPerCase
        measurement_budget_seconds = $MeasurementBudgetSeconds
        measurement_elapsed_seconds = [math]::Round($MeasurementStopwatch.Elapsed.TotalSeconds, 3)
        stopped_for_measurement_budget = $StoppedForBudget
        transition_clone_profile_enabled = [bool] $ProfileTransitionCloneCost
        profiled_type_size_bytes = $ProfiledTypeSizes
        cases = $Rows
    }
    if ($AsJson) {
        $Result | ConvertTo-Json -Depth 20
    }
    else {
        Write-Host "build source: $($BuildReceipt.source_fingerprint.Substring(0, 12))"
        Write-Host "executable:   $($BuildReceipt.executable_sha256.Substring(0, 12))"
        $Rows | Format-Table -AutoSize
    }
}
finally {
    Pop-Location
}
