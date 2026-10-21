use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::ai::combat_search_v2::{
    combat_search_exact_state_hash_v1, CombatSearchV2ActionTrace, CombatSearchV2AdvanceStop,
    CombatSearchV2OutcomeOrderKeyReport, CombatSearchV2PriorityAblation,
    CombatSearchV2Satisfaction, CombatSearchV2Session, CombatSearchV2TrajectoryReport,
    CombatSearchV2WorkQuantum,
};
use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use crate::state::core::ClientInput;

use super::{CombatSearchV2LoadedStart, CombatSearchV2RunOptions};

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalProbeV1Report {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub input_label: String,
    pub contract: &'static str,
    pub config: CombatRootProposalProbeConfigV1,
    pub proposals: Vec<CombatRootProposalObservationV1>,
    pub summary: CombatRootProposalProbeSummaryV1,
    pub notes: Vec<&'static str>,
    /// Exact best trajectory retained for in-process diagnostic tools. The
    /// compact report deliberately omits it unless the caller explicitly
    /// exports the actions to a separate artifact.
    #[serde(skip)]
    pub final_best_actions: Option<Vec<ClientInput>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalProbeConfigV1 {
    pub max_nodes: usize,
    pub wall_ms: Option<u64>,
    pub quantum_nodes: usize,
    pub potion_policy: String,
    pub max_potions_used: Option<u32>,
    pub rollout_policy: String,
    pub child_rollout_policy: String,
    pub priority_ablation: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalPriorityMatrixV1Report {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub input_label: String,
    pub contract: &'static str,
    pub runs: Vec<CombatRootProposalPriorityMatrixRunV1>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalPriorityMatrixRunV1 {
    pub priority_ablation: CombatSearchV2PriorityAblation,
    pub report: CombatRootProposalProbeV1Report,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalObservationV1 {
    pub proposal_ordinal: usize,
    pub first_observed_quantum: usize,
    pub first_observed_nodes_expanded: u64,
    pub first_observed_elapsed_ms: u128,
    pub successor_exact_state_hash: String,
    pub first_turn_action_keys: Vec<String>,
    pub distinct_action_prefixes_observed: usize,
    pub best_complete_outcome: CombatRootProposalOutcomeV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalOutcomeV1 {
    pub outcome_order_key: CombatSearchV2OutcomeOrderKeyReport,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub total_action_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatRootProposalProbeSummaryV1 {
    pub quanta_run: usize,
    pub final_stop: CombatSearchV2AdvanceStop,
    pub nodes_expanded: u64,
    pub elapsed_ms: u128,
    pub final_candidate_frontier_revision: u64,
    pub retained_complete_win_count: usize,
    pub unique_root_successors_observed: usize,
    pub final_best_root_successor_hash: Option<String>,
    pub final_best_proposal_ordinal: Option<usize>,
    pub proposals_observed_before_final_best: Option<usize>,
    pub final_best_first_turn_action_keys: Vec<String>,
    pub final_best_outcome: Option<CombatRootProposalOutcomeV1>,
}

pub fn run_combat_root_proposal_probe_v1(
    loaded: &CombatSearchV2LoadedStart,
    options: CombatSearchV2RunOptions,
    requested_quantum_nodes: usize,
) -> Result<CombatRootProposalProbeV1Report, String> {
    run_combat_root_proposal_probe_with_priority_ablation_v1(
        loaded,
        options,
        requested_quantum_nodes,
        CombatSearchV2PriorityAblation::Baseline,
    )
}

pub fn run_combat_root_proposal_priority_matrix_v1(
    loaded: &CombatSearchV2LoadedStart,
    options: CombatSearchV2RunOptions,
    requested_quantum_nodes: usize,
) -> Result<CombatRootProposalPriorityMatrixV1Report, String> {
    let ablations = [
        CombatSearchV2PriorityAblation::Baseline,
        CombatSearchV2PriorityAblation::NoActionGuidance,
        CombatSearchV2PriorityAblation::NoStateValue,
        CombatSearchV2PriorityAblation::NoActionGuidanceOrStateValue,
    ];
    let mut runs = Vec::with_capacity(ablations.len());
    for priority_ablation in ablations {
        runs.push(CombatRootProposalPriorityMatrixRunV1 {
            priority_ablation,
            report: run_combat_root_proposal_probe_with_priority_ablation_v1(
                loaded,
                options.clone(),
                requested_quantum_nodes,
                priority_ablation,
            )?,
        });
    }
    Ok(CombatRootProposalPriorityMatrixV1Report {
        schema_name: "CombatRootProposalPriorityMatrixV1Report",
        schema_version: 1,
        input_label: loaded.label.clone(),
        contract: "in_process_diagnostic_priority_signal_ablation_only",
        runs,
        notes: vec![
            "all variants start from the same exact combat position in one process",
            "production CombatSearchV2Session construction always uses baseline priority",
            "the matrix diagnoses capability donors; it does not select a production policy",
        ],
    })
}

fn run_combat_root_proposal_probe_with_priority_ablation_v1(
    loaded: &CombatSearchV2LoadedStart,
    options: CombatSearchV2RunOptions,
    requested_quantum_nodes: usize,
    priority_ablation: CombatSearchV2PriorityAblation,
) -> Result<CombatRootProposalProbeV1Report, String> {
    let mut config = options.to_search_config_for_position(loaded.label.clone(), &loaded.position);
    config.satisfaction = CombatSearchV2Satisfaction::BudgetOrExhaustion;
    let max_nodes = config.max_nodes;
    let wall_time = config.wall_time;
    let max_engine_steps_per_action = config.max_engine_steps_per_action;
    let quantum_nodes = requested_quantum_nodes.max(1);
    let config_report = CombatRootProposalProbeConfigV1 {
        max_nodes,
        wall_ms: wall_time.map(duration_ms_u64),
        quantum_nodes,
        potion_policy: format!("{:?}", config.potion_policy),
        max_potions_used: config.max_potions_used,
        rollout_policy: format!("{:?}", config.rollout_policy),
        child_rollout_policy: format!("{:?}", config.child_rollout_policy),
        priority_ablation: priority_ablation.label().to_string(),
    };
    let mut session = CombatSearchV2Session::new_with_priority_ablation(
        &loaded.position.engine,
        &loaded.position.combat,
        config,
        priority_ablation,
    );
    let started = Instant::now();
    let deadline = wall_time.and_then(|duration| started.checked_add(duration));
    let mut proposals = Vec::new();
    let mut proposal_by_successor = HashMap::<String, usize>::new();
    let mut action_prefixes_by_successor = HashMap::<String, HashSet<Vec<String>>>::new();
    let mut quanta_run = 0usize;
    let mut final_stop = CombatSearchV2AdvanceStop::QuantumNodeBudget;
    let mut final_snapshot = session.snapshot();

    loop {
        let nodes_remaining = max_nodes.saturating_sub(session.nodes_expanded() as usize);
        if nodes_remaining == 0 {
            break;
        }
        let wall_remaining =
            deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if wall_remaining == Some(Duration::ZERO) {
            final_stop = CombatSearchV2AdvanceStop::QuantumWallTime;
            break;
        }
        final_stop = session.advance(CombatSearchV2WorkQuantum {
            additional_nodes: quantum_nodes.min(nodes_remaining),
            soft_wall_time: wall_remaining,
        });
        quanta_run = quanta_run.saturating_add(1);
        final_snapshot = session.snapshot();
        observe_snapshot_proposals(
            &loaded.position,
            &final_snapshot.candidate_frontier,
            quanta_run,
            final_snapshot.nodes_expanded,
            started.elapsed(),
            max_engine_steps_per_action,
            &mut proposals,
            &mut proposal_by_successor,
            &mut action_prefixes_by_successor,
        )?;
        if let Some(best) = final_snapshot.best_win.as_ref() {
            observe_snapshot_proposals(
                &loaded.position,
                std::slice::from_ref(best),
                quanta_run,
                final_snapshot.nodes_expanded,
                started.elapsed(),
                max_engine_steps_per_action,
                &mut proposals,
                &mut proposal_by_successor,
                &mut action_prefixes_by_successor,
            )?;
        }
        if matches!(
            final_stop,
            CombatSearchV2AdvanceStop::CandidateSatisfied
                | CombatSearchV2AdvanceStop::FrontierExhausted
                | CombatSearchV2AdvanceStop::AlreadyComplete
        ) {
            break;
        }
    }

    let final_best = final_snapshot
        .best_win
        .as_ref()
        .map(|trajectory| {
            proposal_successor(&loaded.position, trajectory, max_engine_steps_per_action)
                .map(|successor| (trajectory, successor))
        })
        .transpose()?;
    let final_best_root_successor_hash = final_best
        .as_ref()
        .map(|(_, successor)| successor.exact_state_hash.clone());
    let final_best_proposal_ordinal = final_best_root_successor_hash
        .as_ref()
        .and_then(|hash| proposal_by_successor.get(hash))
        .map(|index| proposals[*index].proposal_ordinal);

    Ok(CombatRootProposalProbeV1Report {
        schema_name: "CombatRootProposalProbeV1Report",
        schema_version: 1,
        input_label: loaded.label.clone(),
        contract: "diagnostic_exact_complete_win_root_successor_recall_only",
        config: config_report,
        summary: CombatRootProposalProbeSummaryV1 {
            quanta_run,
            final_stop,
            nodes_expanded: final_snapshot.nodes_expanded,
            elapsed_ms: started.elapsed().as_millis(),
            final_candidate_frontier_revision: final_snapshot.candidate_frontier_revision,
            retained_complete_win_count: final_snapshot.candidate_frontier.len(),
            unique_root_successors_observed: proposals.len(),
            final_best_root_successor_hash,
            final_best_proposal_ordinal,
            proposals_observed_before_final_best: final_best_proposal_ordinal
                .map(|ordinal| ordinal.saturating_sub(1)),
            final_best_first_turn_action_keys: final_best
                .as_ref()
                .map(|(_, successor)| successor.action_keys.clone())
                .unwrap_or_default(),
            final_best_outcome: final_best
                .as_ref()
                .map(|(trajectory, _)| proposal_outcome(trajectory)),
        },
        proposals,
        notes: vec![
            "offline diagnostic only; this report does not alter production combat search",
            "a proposal is an exact root-turn successor belonging to a retained replayable whole-combat win",
            "proposal ordinal is first observation order; ordering within one quantum follows the retained frontier vector and carries no stronger rank claim",
            "identical exact successor states reached by different action orderings are counted once",
            "all V2 work is charged to the reported node and wall budgets",
        ],
        final_best_actions: final_best.as_ref().map(|(trajectory, _)| {
            trajectory
                .actions
                .iter()
                .map(|action| action.input.clone())
                .collect()
        }),
    })
}

fn observe_snapshot_proposals(
    root: &CombatPosition,
    candidates: &[CombatSearchV2TrajectoryReport],
    quantum: usize,
    nodes_expanded: u64,
    elapsed: Duration,
    max_engine_steps_per_action: usize,
    proposals: &mut Vec<CombatRootProposalObservationV1>,
    proposal_by_successor: &mut HashMap<String, usize>,
    action_prefixes_by_successor: &mut HashMap<String, HashSet<Vec<String>>>,
) -> Result<(), String> {
    for candidate in candidates {
        let successor = proposal_successor(root, candidate, max_engine_steps_per_action)?;
        if let Some(existing_index) = proposal_by_successor
            .get(&successor.exact_state_hash)
            .copied()
        {
            let existing = &mut proposals[existing_index];
            let prefixes = action_prefixes_by_successor
                .entry(successor.exact_state_hash.clone())
                .or_default();
            prefixes.insert(successor.action_keys);
            existing.distinct_action_prefixes_observed = prefixes.len();
            let outcome = proposal_outcome(candidate);
            if outcome.outcome_order_key > existing.best_complete_outcome.outcome_order_key {
                existing.best_complete_outcome = outcome;
            }
            continue;
        }
        let index = proposals.len();
        proposals.push(CombatRootProposalObservationV1 {
            proposal_ordinal: index.saturating_add(1),
            first_observed_quantum: quantum,
            first_observed_nodes_expanded: nodes_expanded,
            first_observed_elapsed_ms: elapsed.as_millis(),
            successor_exact_state_hash: successor.exact_state_hash.clone(),
            first_turn_action_keys: successor.action_keys,
            distinct_action_prefixes_observed: 1,
            best_complete_outcome: proposal_outcome(candidate),
        });
        action_prefixes_by_successor
            .entry(successor.exact_state_hash.clone())
            .or_default()
            .insert(proposals[index].first_turn_action_keys.clone());
        proposal_by_successor.insert(successor.exact_state_hash, index);
    }
    Ok(())
}

struct ProposalSuccessor {
    exact_state_hash: String,
    action_keys: Vec<String>,
}

fn proposal_successor(
    root: &CombatPosition,
    candidate: &CombatSearchV2TrajectoryReport,
    max_engine_steps_per_action: usize,
) -> Result<ProposalSuccessor, String> {
    let prefix = first_turn_prefix(&candidate.actions);
    if prefix.is_empty() {
        return Err("retained whole-combat win contains no root action".to_string());
    }
    let mut position = root.clone();
    for (index, action) in prefix.iter().enumerate() {
        if !EngineCombatStepper.is_legal_action(&position, &action.input) {
            return Err(format!(
                "illegal retained root proposal action at index {index}: {}",
                action.action_key
            ));
        }
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.timed_out || step.truncated {
            return Err(format!(
                "retained root proposal replay did not reach a stable state at index {index}: {}",
                action.action_key
            ));
        }
        position = step.position;
    }
    Ok(ProposalSuccessor {
        exact_state_hash: combat_search_exact_state_hash_v1(&position.engine, &position.combat),
        action_keys: prefix
            .iter()
            .map(|action| action.action_key.clone())
            .collect(),
    })
}

fn first_turn_prefix(actions: &[CombatSearchV2ActionTrace]) -> &[CombatSearchV2ActionTrace] {
    let end = actions
        .iter()
        .position(|action| matches!(action.input, ClientInput::EndTurn))
        .map(|index| index.saturating_add(1))
        .unwrap_or(actions.len());
    &actions[..end]
}

fn proposal_outcome(candidate: &CombatSearchV2TrajectoryReport) -> CombatRootProposalOutcomeV1 {
    CombatRootProposalOutcomeV1 {
        outcome_order_key: candidate.outcome_order_key,
        final_hp: candidate.final_hp,
        final_max_hp: candidate.final_max_hp,
        hp_loss: candidate.hp_loss,
        turns: candidate.turns,
        potions_used: candidate.potions_used,
        potions_discarded: candidate.potions_discarded,
        cards_played: candidate.cards_played,
        total_action_count: candidate.actions.len(),
    }
}

fn duration_ms_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
