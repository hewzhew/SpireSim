use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::sim::combat::{CombatStepper, CombatTerminal, EngineCombatStepper};

use super::super::{
    probe_config, FrontierStateCheckpointV1, TurnQualityCorridorCensorReasonV1,
    TurnQualityCorridorCheckpointV1,
};
use super::{
    compare_representative, export_representative_case, frontier_features, TrackedPotionIdentityV1,
    TurnQualityFrontierFeaturesV1, TurnQualityRepresentativeViewV1,
};
use crate::combat_replay_tools::save_combat_inputs;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TurnQualityNextTurnProbeStatusV1 {
    TerminalQualityWinFound,
    ViableBoundaryFound,
    ExhaustedWithoutViableBoundary,
    NoViableBoundaryInCensoredSample,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TurnQualityProbeSuccessorKindV1 {
    TerminalQualityWin,
    NextTurnBoundary,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TurnQualityNextTurnProbeReportV1 {
    root_selection: &'static str,
    requested_root_limit: usize,
    frontier_roots: usize,
    roots_probed: usize,
    root_limit_censored: bool,
    enumeration_censoring_reasons: Vec<TurnQualityCorridorCensorReasonV1>,
    selected_plans: usize,
    below_boundary_hp_plans: usize,
    below_terminal_hp_wins: usize,
    quality_wins: usize,
    viable_roots: usize,
    viable_successors_before_dedup: usize,
    unique_viable_successors: usize,
    status: TurnQualityNextTurnProbeStatusV1,
    best_successors: Vec<TurnQualityProbeSuccessorV1>,
}

#[derive(Clone, Debug, Serialize)]
struct TurnQualityProbeSuccessorV1 {
    source_root_exact_state_hash: String,
    successor_exact_state_hash: String,
    kind: TurnQualityProbeSuccessorKindV1,
    features: TurnQualityFrontierFeaturesV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    case: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_case_prefix_actions: Option<PathBuf>,
}

struct ProbedSuccessor {
    source_root_exact_state_hash: String,
    kind: TurnQualityProbeSuccessorKindV1,
    state: FrontierStateCheckpointV1,
}

pub(super) fn probe_next_turn_frontier(
    base_case: &CombatCase,
    checkpoint: &TurnQualityCorridorCheckpointV1,
    root_limit: usize,
    configured_slot: Option<usize>,
    tracked: Option<&TrackedPotionIdentityV1>,
    output_dir: Option<&PathBuf>,
) -> Result<TurnQualityNextTurnProbeReportV1, String> {
    let mut roots = checkpoint.frontier.iter().collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        compare_representative(right, left, TurnQualityRepresentativeViewV1::Survival)
    });
    roots.truncate(root_limit.min(roots.len()));

    let mut censoring_reasons = BTreeSet::new();
    let mut selected_plans = 0_usize;
    let mut below_boundary_hp_plans = 0_usize;
    let mut below_terminal_hp_wins = 0_usize;
    let mut quality_wins = 0_usize;
    let mut viable_roots = 0_usize;
    let mut viable_successors_before_dedup = 0_usize;
    let mut successors = BTreeMap::<String, ProbedSuccessor>::new();

    for root in &roots {
        let enumeration =
            enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
                &root.position.engine,
                &root.position.combat,
                &probe_config(&checkpoint.settings),
            );
        let report = &enumeration.report;
        selected_plans = selected_plans.saturating_add(report.enumeration.plans);
        if report.enumeration.nodes_expanded >= checkpoint.settings.max_inner_nodes_per_state {
            censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::InnerNodeCapReached);
        }
        if report.enumeration.plans < report.enumeration.preselection_plans {
            censoring_reasons
                .insert(TurnQualityCorridorCensorReasonV1::EndStateSelectionDroppedPlans);
        }
        if report.enumeration.truncated_children > 0 {
            censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::TruncatedTransition);
        }
        if !report.root_action_mask.complete_legal_mask {
            censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::IncompleteRootActionMask);
        }

        let mut root_was_viable = false;
        for candidate in enumeration.candidates {
            let final_hp = candidate.position.combat.entities.player.current_hp;
            let kind = match EngineCombatStepper.terminal(&candidate.position) {
                CombatTerminal::Win if final_hp >= checkpoint.min_terminal_player_hp => {
                    quality_wins = quality_wins.saturating_add(1);
                    TurnQualityProbeSuccessorKindV1::TerminalQualityWin
                }
                CombatTerminal::Win => {
                    below_terminal_hp_wins = below_terminal_hp_wins.saturating_add(1);
                    continue;
                }
                CombatTerminal::Unresolved if final_hp < checkpoint.min_boundary_player_hp => {
                    below_boundary_hp_plans = below_boundary_hp_plans.saturating_add(1);
                    continue;
                }
                CombatTerminal::Unresolved
                    if candidate.position.combat.turn.turn_count
                        > root.position.combat.turn.turn_count =>
                {
                    TurnQualityProbeSuccessorKindV1::NextTurnBoundary
                }
                CombatTerminal::Unresolved => {
                    censoring_reasons
                        .insert(TurnQualityCorridorCensorReasonV1::NonTurnBoundaryCandidate);
                    continue;
                }
                CombatTerminal::Loss => {
                    below_boundary_hp_plans = below_boundary_hp_plans.saturating_add(1);
                    continue;
                }
            };
            root_was_viable = true;
            viable_successors_before_dedup = viable_successors_before_dedup.saturating_add(1);
            let mut actions = root.actions.clone();
            actions.extend(
                candidate
                    .report
                    .actions
                    .iter()
                    .map(|action| action.input.clone()),
            );
            let exact_state_hash =
                combat_exact_state_hash_v2(&candidate.position.engine, &candidate.position.combat);
            let successor = ProbedSuccessor {
                source_root_exact_state_hash: root.exact_state_hash.clone(),
                kind,
                state: FrontierStateCheckpointV1 {
                    exact_state_hash: exact_state_hash.clone(),
                    position: candidate.position,
                    actions,
                },
            };
            match successors.entry(exact_state_hash) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(successor);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if compare_probed_successor(&successor, entry.get()) == Ordering::Greater {
                        entry.insert(successor);
                    }
                }
            }
        }
        if root_was_viable {
            viable_roots = viable_roots.saturating_add(1);
        }
    }

    let unique_viable_successors = successors.len();
    let mut successors = successors.into_values().collect::<Vec<_>>();
    successors.sort_by(|left, right| compare_probed_successor(right, left));
    let mut best_successors = Vec::new();
    for (rank, successor) in successors.into_iter().take(3).enumerate() {
        let (case_path, actions_path) = if let Some(directory) = output_dir {
            let stem = format!("next_turn_viable.{rank}");
            let case_path = directory.join(format!("{stem}.case.json"));
            let actions_path = directory.join(format!("{stem}.prefix.actions.json"));
            export_representative_case(
                base_case,
                &successor.state,
                checkpoint.next_depth.saturating_add(1),
                &case_path,
            )?;
            save_combat_inputs(&actions_path, successor.state.actions.clone())?;
            (Some(case_path), Some(actions_path))
        } else {
            (None, None)
        };
        best_successors.push(TurnQualityProbeSuccessorV1 {
            source_root_exact_state_hash: successor.source_root_exact_state_hash,
            successor_exact_state_hash: successor.state.exact_state_hash.clone(),
            kind: successor.kind,
            features: frontier_features(&successor.state, configured_slot, tracked),
            case: case_path,
            original_case_prefix_actions: actions_path,
        });
    }

    let root_limit_censored = roots.len() < checkpoint.frontier.len();
    let status = if quality_wins > 0 {
        TurnQualityNextTurnProbeStatusV1::TerminalQualityWinFound
    } else if viable_roots > 0 {
        TurnQualityNextTurnProbeStatusV1::ViableBoundaryFound
    } else if !root_limit_censored && censoring_reasons.is_empty() {
        TurnQualityNextTurnProbeStatusV1::ExhaustedWithoutViableBoundary
    } else {
        TurnQualityNextTurnProbeStatusV1::NoViableBoundaryInCensoredSample
    };
    Ok(TurnQualityNextTurnProbeReportV1 {
        root_selection: "survival_ranked_exact_frontier_roots",
        requested_root_limit: root_limit,
        frontier_roots: checkpoint.frontier.len(),
        roots_probed: roots.len(),
        root_limit_censored,
        enumeration_censoring_reasons: censoring_reasons.into_iter().collect(),
        selected_plans,
        below_boundary_hp_plans,
        below_terminal_hp_wins,
        quality_wins,
        viable_roots,
        viable_successors_before_dedup,
        unique_viable_successors,
        status,
        best_successors,
    })
}

fn compare_probed_successor(left: &ProbedSuccessor, right: &ProbedSuccessor) -> Ordering {
    let kind_rank = |kind| match kind {
        TurnQualityProbeSuccessorKindV1::TerminalQualityWin => 1_u8,
        TurnQualityProbeSuccessorKindV1::NextTurnBoundary => 0_u8,
    };
    kind_rank(left.kind)
        .cmp(&kind_rank(right.kind))
        .then_with(|| {
            compare_representative(
                &left.state,
                &right.state,
                TurnQualityRepresentativeViewV1::Survival,
            )
        })
}
