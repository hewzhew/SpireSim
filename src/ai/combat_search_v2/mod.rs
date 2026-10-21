use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ai::combat_state_key::{
    combat_dominance_diagnostic_parts_v1, combat_dominance_key, combat_exact_state_hash_v2,
    combat_exact_state_key, CombatDominanceKey, CombatExactStateKey,
};
use crate::content::monsters::EnemyId;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use crate::sim::combat_action::CombatActionChoice;
use crate::sim::combat_projection::monster_preview_total_damage_in_combat;
use crate::state::core::{ClientInput, EngineState};

// Core search loop and frontier ownership.
mod frontier;
pub mod oracle_action_policy;
mod outcome_score;
mod search;
mod transition;

// Action semantics: legal action facts, ordering, equivalence, and expansion shape.
mod action_effects;
mod action_equivalence;
mod action_facts;
mod action_ordering;
mod action_priority;
mod action_resource_timing;
mod attack_retaliation;
mod expansion;
mod target_fanout;

// Evaluation, value, and outcome comparison.
mod baseline;
mod recoverable_resource_urgency;
mod rollout_value;
mod value;
mod value_facts;

// Rollout policies and bounded rollout execution.
mod rollout;
mod rollout_action_selector;
mod rollout_cache;
mod rollout_estimate;
mod rollout_pending_choice;
mod rollout_profile;
mod rollout_scheduler;

// Turn-level planning and current-turn structure.
mod turn_branching;
mod turn_local_dominance;
pub(crate) mod turn_planner;
mod turn_prefix;
mod turn_sequence;
mod turn_sequence_effect;

// Combat phase and enemy mechanics facts.
mod enemy_mechanics_profile;
mod enemy_phase_transition;
mod external_payoff;
mod phase_action_ordering;
mod phase_profile;

// Pending choice and potion boundaries.
pub mod pending_choice_action_prefix;
mod pending_choice_fanout;
mod pending_choice_ordering;
mod pending_choice_profile;
mod plugins;
mod potions;

// State abstraction and exactness audits.
mod card_identity;
mod discard_order_shadow_audit;
pub mod state_abstraction;

// Reports, diagnostics, and opt-in analysis tools.
mod decision_microscope;
mod deficit_evidence;
mod diagnostics;
mod diagnostics_tags;
mod line_lab;
mod mechanism_probe;
mod rollout_probe;
mod segment_plan;
mod trajectory_report;
mod turn_plan_probe;
mod turn_plan_probe_report;
mod turn_plan_rescue;
mod turn_pool_rescue;
mod witness_guidance;

#[cfg(test)]
mod semantic_regression;
mod types;

use action_equivalence::{
    compress_equivalent_actions, ActionEquivalenceDiagnosticsCollector, ActionEquivalenceSummary,
};
use action_facts::summarize_action_facts_from_step;
use action_ordering::{
    order_indexed_action_choices, order_indexed_action_choices_with_plugins,
    ActionOrderingDiagnosticsCollector, ActionOrderingSummary, IndexedActionChoice,
};
use card_identity::{
    summarize_card_identity, CardIdentityDiagnosticsCollector, CardIdentitySummary,
};
use diagnostics::{SearchDiagnosticsCollector, SearchDiagnosticsFinish, FRONTIER_SAMPLE_LIMIT};
use diagnostics_tags::diagnosis_tags;
use expansion::{
    summarize_action_expansion, ActionExpansionDiagnosticsCollector, ActionExpansionSummary,
};
use frontier::{is_resource_covered, ResourceVector, SearchNode};
use outcome_score::CombatOutcomeScore;
use pending_choice_action_prefix::{
    PendingChoiceActionFamily, PendingChoiceActionPrefix, PendingChoiceActionWork,
};
use pending_choice_ordering::pending_choice_ordering_hint;
use pending_choice_profile::{
    summarize_pending_choice, PendingChoiceDiagnosticsCollector, PendingChoiceProfile,
};
use phase_profile::{combat_search_phase_profile, combat_search_phase_profile_report};
use rollout_action_selector::{choose_rollout_action, filtered_rollout_legal_actions};
use rollout_cache::RolloutCache;

use rollout_estimate::{RolloutNodeEstimate, RolloutStopReason};
use sts_combat_knowledge::{
    combat_tactical_visible_incoming_damage_v1 as visible_incoming_damage,
    combat_tactical_visible_incoming_damage_with_player_intangible_v1 as visible_incoming_damage_with_player_intangible,
};
use target_fanout::{
    summarize_target_fanout, TargetFanoutDiagnosticsCollector, TargetFanoutSummary,
};
use trajectory_report::{summarize_state, trajectory_report};
use transition::{filtered_legal_actions, is_use_potion_input, terminal_label};
use turn_branching::{
    classify_turn_branch_transition, TurnBranchTransition, TurnBranchingDiagnosticsCollector,
    TurnBranchingStateObservation,
};
use turn_local_dominance::{
    TurnLocalDominanceDiagnosticsCollector, TurnLocalDominanceStateObservation,
};
use turn_planner::{
    build_turn_boundary_portfolio, turn_plan_frontier_seed, TurnPlanDiagnosticsCollector,
};
use turn_prefix::{
    advance_turn_prefix, summarize_turn_prefix, TurnPrefixDiagnosticsCollector, TurnPrefixState,
    TurnPrefixSummary,
};
use turn_sequence::{
    summarize_turn_sequence, TurnSequenceDiagnosticsCollector, TurnSequenceSummary,
};
use value::{combat_search_frontier_value_report, COMBAT_SEARCH_FRONTIER_VALUE_POLICY};
use value_facts::{living_enemy_count, terminal_rank, total_living_enemy_hp};

pub use action_facts::{
    CombatSearchV2ActionAccessMechanicsFacts, CombatSearchV2ActionCardFacts,
    CombatSearchV2ActionDerivedMechanicsFacts, CombatSearchV2ActionDirectMechanicsFacts,
    CombatSearchV2ActionExactDeltaFacts, CombatSearchV2ActionFacts,
    CombatSearchV2ActionImmediateFacts, CombatSearchV2ActionMechanicsFacts,
    CombatSearchV2ActionPendingChoiceSelectionFacts,
    CombatSearchV2ActionPendingChoiceSelectionRole, CombatSearchV2ActionReactiveMechanicsFacts,
    CombatSearchV2ActionResourceTimingFacts, CombatSearchV2ActionTargetFacts,
    CombatSearchV2AttackRetaliationTargetFacts, CombatSearchV2TimedEnemyThreatTargetFacts,
};
pub use baseline::{
    compare_outcome_metrics, CombatSearchV2OutcomeMetrics, WHOLE_COMBAT_OUTCOME_CRITERIA,
};
pub use decision_microscope::{
    explain_combat_search_v2_initial_decision, CombatSearchV2ActionFactsReport,
    CombatSearchV2DecisionCandidateReport, CombatSearchV2DecisionContext,
    CombatSearchV2DecisionMicroscopeConfigReport, CombatSearchV2DecisionMicroscopeReport,
    CombatSearchV2DecisionOneStepReport, CombatSearchV2DecisionSelectedAction,
    CombatSearchV2DecisionTrajectorySummary,
};
pub use deficit_evidence::{
    derive_combat_deficit_evidence, CombatDeficitEvidenceFlag, CombatDeficitEvidenceObservations,
    CombatDeficitEvidenceReport,
};
pub use external_payoff::{
    external_burden_count, has_external_payoff_opportunity, has_healing_payoff_opportunity,
    has_persistent_or_reward_payoff_opportunity, materialized_persistent_payoff_score_v1,
    recoverable_gold_delta, recoverable_stolen_gold, unrecovered_stolen_gold,
    CombatPersistentOutcomeV1,
};
pub use line_lab::{
    run_combat_line_lab_from_parent_v0, run_combat_line_lab_v0, CombatLineLabReport,
};
pub use mechanism_probe::{
    run_combat_mechanism_horizon_probe_v1, CombatMechanismDepthReportV1,
    CombatMechanismEndpointStateV1, CombatMechanismEndpointV1, CombatMechanismHorizonProbeConfigV1,
    CombatMechanismHorizonProbeReportV1, CombatMechanismPowerObservationV1,
    COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_NAME, COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_VERSION,
};
pub use plugins::{
    CombatSearchAcceptancePlugin, CombatSearchAcceptancePluginId,
    CombatSearchActionOrderingPlugins, CombatSearchActionPriorPlugin,
    CombatSearchActionPriorPluginId, CombatSearchArtifactPlugin, CombatSearchArtifactPluginId,
    CombatSearchAttemptPolicy, CombatSearchBudgetSpec, CombatSearchChildRolloutPlugin,
    CombatSearchChildRolloutPluginId, CombatSearchEngineProfile, CombatSearchExpansionPlugin,
    CombatSearchExpansionPluginId, CombatSearchNodeEvaluatorPlugin,
    CombatSearchNodeEvaluatorPluginId, CombatSearchPhaseGuardPlugin,
    CombatSearchPhaseGuardPluginId, CombatSearchPluginStack, CombatSearchPotionPlugin,
    CombatSearchProfile, CombatSearchRolloutPlugin, CombatSearchRolloutPluginId,
    CombatSearchTurnPlanPlugin, CombatSearchTurnPlanPluginId,
};
pub use search::{
    run_combat_search_v2, run_combat_search_v2_with_stepper, CombatSearchV2DecisionSnapshot,
    CombatSearchV2Session, CombatSearchV2WorkQuantum,
};
pub use segment_plan::{plan_combat_turn_segment_v1, CombatSearchV2TurnSegmentReport};
pub use trajectory_report::trajectory_from_state;
pub use turn_plan_probe::{
    enumerate_combat_search_v2_turn_plan_probe_candidates,
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices,
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided,
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline,
    CombatSearchV2TurnPlanPartialCandidateAuditV1,
    CombatSearchV2TurnPlanPartialGuideConditionedAuditV1,
    CombatSearchV2TurnPlanPartialRetentionAuditV1, CombatSearchV2TurnPlanPartialRetentionLaneV1,
    CombatSearchV2TurnPlanPrefixCandidateV1, CombatSearchV2TurnPlanPrefixLayerV1,
    CombatSearchV2TurnPlanPrefixSessionV1, CombatSearchV2TurnPlanProbeCandidate,
    CombatSearchV2TurnPlanProbeEnumeration, CombatSearchV2TurnPlanProbeGuideRankV1,
};
pub use turn_plan_probe_report::{
    CombatSearchV2TurnPlanProbeCandidateReport, CombatSearchV2TurnPlanProbeRootReport,
    CombatSearchV2TurnPlanProbeStepReport,
};
pub use turn_plan_rescue::{find_combat_turn_plan_rescue_win_v0, CombatTurnPlanRescueWin};
pub use turn_pool_rescue::{
    find_combat_turn_pool_rescue_win_v0, run_combat_turn_pool_opening_report_v0,
    CombatTurnPoolOpeningLineReport, CombatTurnPoolOpeningReport, CombatTurnPoolRescueWin,
};
pub use types::*;
pub use witness_guidance::{
    compile_combat_search_witness_prior_v0, replay_combat_search_witness_line_v0,
    replay_combat_search_witness_line_v1, CombatSearchV2WitnessLine, CombatSearchV2WitnessPrior,
    CombatSearchV2WitnessReplay, CombatSearchV2WitnessReplayStep,
    CombatSearchV2WitnessReplayStepV1, CombatSearchV2WitnessReplayV1,
};

pub fn combat_search_exact_state_hash_v1(engine: &EngineState, combat: &CombatState) -> String {
    combat_exact_state_hash_v2(engine, combat)
}

/// Runs the mature bounded no-potion tactical policy as a proposal donor.
/// The caller must replay the returned inputs exactly before treating the
/// proposal as a terminal witness.
#[derive(Clone, Debug)]
pub struct OracleRolloutWitnessProposalV1 {
    pub actions: Vec<ClientInput>,
    pub final_hp_hint: i32,
}

pub fn oracle_rollout_witness_proposal_v1(
    position: &CombatPosition,
    max_actions: usize,
    deadline: Option<Instant>,
) -> Option<OracleRolloutWitnessProposalV1> {
    oracle_rollout_witness_proposal_with_potion_contract_v1(
        position,
        max_actions,
        deadline,
        None,
        None,
    )
}

pub fn oracle_rollout_witness_proposal_with_potion_contract_v1(
    position: &CombatPosition,
    max_actions: usize,
    deadline: Option<Instant>,
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
) -> Option<OracleRolloutWitnessProposalV1> {
    let config = CombatSearchV2Config::default();
    let mut performance = rollout_profile::RolloutPerformanceCounters::default();
    let baseline = oracle_no_potion_suffix_proposal_v1(
        SearchNode::root(position.engine.clone(), position.combat.clone()),
        Vec::new(),
        &config,
        max_actions,
        deadline,
        &mut performance,
    )
    .filter(|proposal| rollout_proposal_exactly_replays_to_win(position, proposal, &config));
    let severe_no_potion_loss = baseline.as_ref().is_none_or(|baseline| {
        let player = &position.combat.entities.player;
        let material_loss = (player.current_hp / 4).max(6);
        player.current_hp.saturating_sub(baseline.final_hp_hint) >= material_loss
    });
    let mut best = baseline;

    // Potions remain a scarce strategic resource, so the mature rollout does
    // not consume them on its own. At the exact root, however, enumerate each
    // legal use as one bounded prefix and let the same no-potion policy solve
    // the suffix. A potion may replace the conserved baseline only when that
    // baseline pays a material fraction of the HP actually available at the
    // encounter (or finds no win). The
    // caller still replays every input and successor hash before accepting a
    // witness.
    let potion_prefixes = if max_potions_used == Some(0) {
        Vec::new()
    } else {
        transition::filtered_legal_actions(
            EngineCombatStepper.atomic_action_choices(position),
            CombatSearchV2PotionPolicy::All,
            allowed_potion_slots,
            None,
            &position.combat,
        )
        .into_iter()
        .filter(|choice| transition::is_use_potion_input(&choice.input))
        .collect::<Vec<_>>()
    };
    for choice in potion_prefixes {
        if deadline.is_some_and(|limit| Instant::now() >= limit) || max_actions == 0 {
            break;
        }
        let result = EngineCombatStepper.apply_to_stable(
            position,
            choice.input.clone(),
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline,
            },
        );
        if result.truncated || result.timed_out {
            continue;
        }
        let candidate = oracle_no_potion_suffix_proposal_v1(
            SearchNode::root(result.position.engine, result.position.combat),
            vec![choice.input],
            &config,
            max_actions.saturating_sub(1),
            deadline,
            &mut performance,
        )
        .filter(|proposal| rollout_proposal_exactly_replays_to_win(position, proposal, &config));
        if severe_no_potion_loss
            && candidate.as_ref().is_some_and(|candidate| {
                best.as_ref()
                    .is_none_or(|current| candidate.final_hp_hint > current.final_hp_hint)
            })
        {
            best = candidate;
        }
    }
    best
}

fn rollout_proposal_exactly_replays_to_win(
    root: &CombatPosition,
    proposal: &OracleRolloutWitnessProposalV1,
    config: &CombatSearchV2Config,
) -> bool {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    for input in &proposal.actions {
        if stepper.choice_for_legal_input(&position, input).is_none() {
            return false;
        }
        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return false;
        }
        position = result.position;
    }
    EngineCombatStepper.terminal(&position) == CombatTerminal::Win
        && !position.combat.runtime.combat_smoked
}

/// Runs the mature complete tactical search as a bounded proposal donor when
/// a winning line needs potion use interleaved with ordinary card play.  The
/// caller remains responsible for replaying every input and exact successor;
/// this function never promotes the legacy report to authoritative evidence.
pub fn oracle_search_witness_proposal_v1(
    position: &CombatPosition,
    max_nodes: usize,
    deadline: Option<Instant>,
) -> Option<OracleRolloutWitnessProposalV1> {
    // The caller owns a separately bounded authoritative replay window.
    // Reserving that work here as well cuts proposal search twice and can
    // discard wins found at the edge of the tactical quantum.
    let deadline = deadline?;
    let remaining = deadline.checked_duration_since(Instant::now())?;
    if remaining.is_zero() {
        return None;
    }
    let has_potion = position.combat.entities.potions.iter().any(Option::is_some);

    // Mixed potion search can spend nearly all of a tactical quantum inside
    // attractive consumable branches and miss a strictly better conserved
    // line.  Establish a bounded no-potion quality baseline first.  Exact
    // Act 3 cases need only a small fraction of the quantum for that baseline;
    // the rest remains available to prove that spending a potion is necessary
    // or materially better.
    let baseline_duration = if has_potion {
        Duration::from_millis(1_000).min(remaining)
    } else {
        remaining
    };
    let baseline_deadline = Instant::now()
        .checked_add(baseline_duration)
        .map_or(deadline, |candidate| candidate.min(deadline));
    let baseline_nodes = if has_potion {
        (max_nodes / 10).max(1)
    } else {
        max_nodes
    };
    let baseline = oracle_search_witness_proposal_with_potions_v1(
        position,
        baseline_nodes,
        baseline_deadline,
        CombatSearchV2PotionPolicy::Never,
        Some(0),
    );
    if !has_potion {
        return baseline;
    }

    let player = &position.combat.entities.player;
    let material_loss = (player.current_hp / 4).max(6);
    let severe_no_potion_loss = baseline.as_ref().is_none_or(|proposal| {
        player.current_hp.saturating_sub(proposal.final_hp_hint) >= material_loss
    });
    if !severe_no_potion_loss {
        return baseline;
    }

    let remaining_nodes = max_nodes.saturating_sub(baseline_nodes).max(1);
    // Once a conserved line exists, spend the remaining donor budget improving
    // that line instead of opening the much larger consumable action tree.  A
    // mixed search is useful only when the quick conserved pass found no win at
    // all.  This keeps scarce potions for the run while still letting them
    // unlock combats that the deck cannot solve unaided.
    let (potion_policy, max_potions_used) = if baseline.is_some() {
        (CombatSearchV2PotionPolicy::Never, Some(0))
    } else {
        (CombatSearchV2PotionPolicy::All, Some(3))
    };
    let searched = oracle_search_witness_proposal_with_potions_v1(
        position,
        remaining_nodes,
        deadline,
        potion_policy,
        max_potions_used,
    );
    match (baseline, searched) {
        (Some(baseline), Some(searched)) if searched.final_hp_hint > baseline.final_hp_hint => {
            Some(searched)
        }
        (Some(baseline), _) => Some(baseline),
        (None, searched) => searched,
    }
}

fn oracle_search_witness_proposal_with_potions_v1(
    position: &CombatPosition,
    max_nodes: usize,
    deadline: Instant,
    potion_policy: CombatSearchV2PotionPolicy,
    max_potions_used: Option<u32>,
) -> Option<OracleRolloutWitnessProposalV1> {
    let remaining = deadline.checked_duration_since(Instant::now())?;
    if remaining.is_zero() {
        return None;
    }
    // The donor's useful legacy capability is quality improvement after the
    // first survivable line. Authoritative replay now has its own small,
    // bounded window in the caller, so stopping at the first complete win
    // would throw that capability away (often preserving a near-death win).
    let config = CombatSearchV2Config {
        max_nodes,
        wall_time: Some(remaining),
        satisfaction: CombatSearchV2Satisfaction::ZeroLossOrBudget,
        potion_policy,
        max_potions_used,
        ..Default::default()
    };
    let report = run_combat_search_v2(&position.engine, &position.combat, config);
    let trajectory = report.best_win_trajectory?;
    Some(OracleRolloutWitnessProposalV1 {
        actions: trajectory
            .actions
            .into_iter()
            .map(|action| action.input)
            .collect(),
        final_hp_hint: trajectory.final_hp,
    })
}

fn oracle_no_potion_suffix_proposal_v1(
    node: SearchNode,
    mut prefix: Vec<ClientInput>,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut rollout_profile::RolloutPerformanceCounters,
) -> Option<OracleRolloutWitnessProposalV1> {
    let profile = combat_search_phase_profile(&node.engine, &node.combat);
    let phase_aware = profile
        .enemy_mechanics
        .finite_survival_damage_mitigation_target_count
        > 0
        || profile.enemy_mechanics.guardian_open_count > 0
        || profile.enemy_mechanics.guardian_defensive_count > 0
        || profile.enemy_mechanics.bronze_automaton_count > 0
        || profile.enemy_mechanics.bronze_orb_count > 0;
    let estimate = if phase_aware {
        rollout::phase_aware_no_potion_rollout(
            &node,
            &EngineCombatStepper,
            config,
            max_actions,
            deadline,
            performance,
        )
    } else {
        rollout::conservative_no_potion_rollout(
            &node,
            &EngineCombatStepper,
            config,
            max_actions,
            deadline,
            performance,
        )
    };
    if !estimate.is_replayable_terminal_win() {
        return None;
    }
    prefix.extend(
        estimate
            .action_preview
            .into_iter()
            .map(|action| action.input),
    );
    Some(OracleRolloutWitnessProposalV1 {
        actions: prefix,
        final_hp_hint: estimate.final_hp,
    })
}

pub fn combat_search_action_ordering_role_label_for_state(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
) -> &'static str {
    combat_search_action_ordering_role_label_for_state_with_plugins(
        engine,
        combat,
        input,
        CombatSearchActionOrderingPlugins::default(),
    )
}

pub(crate) fn combat_search_action_ordering_role_label_for_state_with_plugins(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
    plugins: CombatSearchActionOrderingPlugins<'_>,
) -> &'static str {
    action_priority::priority_for_input_with_plugins(engine, combat, input, plugins)
        .role
        .label()
}

pub fn combat_search_phase_profile_report_for_state(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatSearchV2PhaseProfileReport {
    combat_search_phase_profile_report(combat_search_phase_profile(engine, combat))
}

pub fn filter_combat_search_legal_actions(
    choices: Vec<CombatActionChoice>,
    potion_policy: CombatSearchV2PotionPolicy,
    allowed_potion_slots: Option<u64>,
    combat: &CombatState,
) -> Vec<CombatActionChoice> {
    transition::filtered_legal_actions(choices, potion_policy, allowed_potion_slots, None, combat)
}

pub fn filter_combat_search_legal_actions_with_potion_plugin(
    choices: Vec<CombatActionChoice>,
    potion: CombatSearchPotionPlugin,
    combat: &CombatState,
) -> Vec<CombatActionChoice> {
    transition::filtered_legal_actions(
        choices,
        potion.policy,
        potion.allowed_potion_slots,
        potion.allow_potion_discard,
        combat,
    )
}

#[cfg(test)]
const _: fn(
    &crate::sim::combat::CombatPosition,
    &CombatSearchV2WitnessLine,
    usize,
) -> Result<CombatSearchV2WitnessReplayV1, String> = replay_combat_search_witness_line_v1;
