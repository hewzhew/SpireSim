use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agent::task::TaskResearchPlanOutcomeV1;
use crate::ai::combat_search_v2::{
    CombatSearchV2ActionTrace, CombatSearchV2AdvanceStop, CombatSearchV2PotionPolicy,
    CombatSearchV2Satisfaction, CombatSearchV2Session, CombatSearchV2TrajectoryReport,
    CombatSearchV2WorkQuantum,
};
use crate::content::potions::PotionId;
use crate::eval::combat_witness_materialization::{
    materialize_combat_witness_outcome_v1, CombatWitnessMaterializedOutcomeV1,
};
use crate::eval::combat_witness_storyboard::{
    build_combat_witness_storyboard_v1, CombatWitnessStoryboardV1,
};

use super::{CombatSearchV2LoadedStart, CombatSearchV2RunOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatResourceFrontierProbeStopV1 {
    AllLanesExhausted,
    TotalNodeBudget,
    WallTime,
    NoProgress,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierProbeV1Report {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub input_label: String,
    pub contract: &'static str,
    pub config: CombatResourceFrontierProbeConfigV1,
    pub stop: CombatResourceFrontierProbeStopV1,
    pub frontier_open: bool,
    pub total_nodes_expanded: u64,
    pub elapsed_ms: u128,
    pub frontier_revision: u64,
    pub lanes: Vec<CombatResourceFrontierLaneReportV1>,
    pub frontier: Vec<CombatResourceFrontierCandidateV1>,
    pub improvements: Vec<CombatResourceFrontierImprovementV1>,
    pub service_trace: Vec<CombatResourceFrontierServiceObservationV1>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierProbeConfigV1 {
    pub max_total_nodes: usize,
    pub wall_ms: Option<u64>,
    pub quantum_nodes: usize,
    pub max_engine_steps_per_action: usize,
    pub spending_lane_potion_policy: String,
    pub turn_plan_policy: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierLaneReportV1 {
    pub lane_id: String,
    pub entry_potion_slot: Option<usize>,
    pub entry_potion_id: Option<PotionId>,
    pub allowed_entry_potion_slots: u64,
    pub max_potions_used: u32,
    pub nodes_expanded: u64,
    pub candidate_frontier_revision: u64,
    pub retained_complete_win_count: usize,
    pub remaining_work_items: usize,
    pub exact_state_keys: usize,
    pub last_stop: Option<CombatSearchV2AdvanceStop>,
    pub exhausted: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierCandidateV1 {
    pub source_lane_id: String,
    pub first_observed_total_nodes: u64,
    pub action_count: usize,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub storyboard: CombatWitnessStoryboardV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierImprovementV1 {
    pub frontier_revision: u64,
    pub served_lane_id: String,
    pub total_nodes_expanded: u64,
    pub lane_nodes_expanded: u64,
    pub frontier_size: usize,
    pub preserved_entry_potion_best_hp: Option<i32>,
    pub spending_best_hp: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResourceFrontierServiceObservationV1 {
    pub service_index: usize,
    pub lane_id: String,
    pub requested_additional_nodes: usize,
    pub nodes_expanded_this_service: u64,
    pub total_nodes_expanded: u64,
    pub lane_nodes_expanded: u64,
    pub stop: CombatSearchV2AdvanceStop,
    pub lane_candidate_frontier_revision: u64,
    pub lane_retained_complete_win_count: usize,
    pub lane_best_win_hp: Option<i32>,
    pub remaining_work_items: usize,
    pub exact_state_keys: usize,
    pub resource_frontier_changed: bool,
    pub observed_resource_frontier_revision: u64,
}

struct ResourceLane {
    lane_id: String,
    entry_potion_slot: Option<usize>,
    entry_potion_id: Option<PotionId>,
    allowed_entry_potion_slots: u64,
    max_potions_used: u32,
    session: CombatSearchV2Session,
    last_snapshot: crate::ai::combat_search_v2::CombatSearchV2DecisionSnapshot,
    last_stop: Option<CombatSearchV2AdvanceStop>,
    exhausted: bool,
}

struct RetainedCandidate {
    source_lane_id: String,
    first_observed_total_nodes: u64,
    trajectory: CombatSearchV2TrajectoryReport,
    outcome: CombatWitnessMaterializedOutcomeV1,
    public_outcome: TaskResearchPlanOutcomeV1,
    storyboard: CombatWitnessStoryboardV1,
}

pub fn run_combat_resource_frontier_probe_v1(
    loaded: &CombatSearchV2LoadedStart,
    options: CombatSearchV2RunOptions,
    requested_quantum_nodes: usize,
) -> Result<CombatResourceFrontierProbeV1Report, String> {
    let mut base_config =
        options.to_search_config_for_position(loaded.label.clone(), &loaded.position);
    base_config.satisfaction = CombatSearchV2Satisfaction::BudgetOrExhaustion;
    base_config.allow_potion_discard = Some(false);
    let max_total_nodes = base_config.max_nodes;
    let wall_time = base_config.wall_time;
    let quantum_nodes = requested_quantum_nodes.max(1);
    let max_engine_steps_per_action = base_config.max_engine_steps_per_action;
    let spending_lane_potion_policy = options
        .potion_policy
        .unwrap_or(CombatSearchV2PotionPolicy::All);
    let config_report = CombatResourceFrontierProbeConfigV1 {
        max_total_nodes,
        wall_ms: wall_time.map(duration_ms_u64),
        quantum_nodes,
        max_engine_steps_per_action,
        spending_lane_potion_policy: format!("{spending_lane_potion_policy:?}"),
        turn_plan_policy: format!("{:?}", base_config.turn_plan_policy),
    };
    base_config.wall_time = None;

    let lane_specs = resource_lane_specs(&loaded.position.combat, spending_lane_potion_policy);
    let mut lanes = lane_specs
        .into_iter()
        .map(|spec| {
            let mut config = base_config.clone();
            config.input_label = Some(format!("{}#{}", loaded.label, spec.lane_id));
            config.potion_policy = spec.potion_policy;
            config.max_potions_used = Some(spec.max_potions_used);
            config.allowed_potion_slots = Some(spec.allowed_entry_potion_slots);
            let session = CombatSearchV2Session::new(
                &loaded.position.engine,
                &loaded.position.combat,
                config,
            );
            let last_snapshot = session.snapshot();
            ResourceLane {
                lane_id: spec.lane_id,
                entry_potion_slot: spec.entry_potion_slot,
                entry_potion_id: spec.entry_potion_id,
                allowed_entry_potion_slots: spec.allowed_entry_potion_slots,
                max_potions_used: spec.max_potions_used,
                session,
                last_snapshot,
                last_stop: None,
                exhausted: false,
            }
        })
        .collect::<Vec<_>>();

    let started = Instant::now();
    let deadline = wall_time.and_then(|duration| started.checked_add(duration));
    let mut retained = Vec::<RetainedCandidate>::new();
    let mut improvements = Vec::new();
    let mut service_trace = Vec::new();
    let mut frontier_revision = 0_u64;
    let mut total_nodes_expanded = 0_u64;
    let stop = 'search: loop {
        if total_nodes_expanded >= max_total_nodes as u64 {
            break CombatResourceFrontierProbeStopV1::TotalNodeBudget;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break CombatResourceFrontierProbeStopV1::WallTime;
        }
        if lanes.iter().all(|lane| lane.exhausted) {
            break CombatResourceFrontierProbeStopV1::AllLanesExhausted;
        }

        let round_nodes_before = total_nodes_expanded;
        for lane in lanes.iter_mut().filter(|lane| !lane.exhausted) {
            let total_nodes_remaining =
                max_total_nodes.saturating_sub(total_nodes_expanded as usize);
            if total_nodes_remaining == 0 {
                break 'search CombatResourceFrontierProbeStopV1::TotalNodeBudget;
            }
            let wall_remaining =
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            if wall_remaining == Some(Duration::ZERO) {
                break 'search CombatResourceFrontierProbeStopV1::WallTime;
            }
            let lane_nodes_before = lane.session.nodes_expanded();
            let requested_additional_nodes = quantum_nodes.min(total_nodes_remaining);
            let stop = lane.session.advance(CombatSearchV2WorkQuantum {
                additional_nodes: requested_additional_nodes,
                soft_wall_time: wall_remaining,
            });
            lane.last_stop = Some(stop);
            lane.exhausted = matches!(
                stop,
                CombatSearchV2AdvanceStop::FrontierExhausted
                    | CombatSearchV2AdvanceStop::AlreadyComplete
                    | CombatSearchV2AdvanceStop::CandidateSatisfied
            );
            lane.last_snapshot = lane.session.snapshot();
            total_nodes_expanded = total_nodes_expanded.saturating_add(
                lane.session
                    .nodes_expanded()
                    .saturating_sub(lane_nodes_before),
            );
            let changed = merge_snapshot_candidates(
                &loaded.position,
                max_engine_steps_per_action,
                &lane.lane_id,
                total_nodes_expanded,
                &lane.last_snapshot,
                &mut retained,
            )?;
            if changed {
                frontier_revision = frontier_revision.saturating_add(1);
                improvements.push(frontier_improvement(
                    frontier_revision,
                    lane,
                    total_nodes_expanded,
                    &retained,
                ));
            }
            service_trace.push(service_observation(
                service_trace.len(),
                lane,
                requested_additional_nodes,
                lane_nodes_before,
                total_nodes_expanded,
                stop,
                changed,
                frontier_revision,
            ));
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break 'search CombatResourceFrontierProbeStopV1::WallTime;
            }
        }
        if total_nodes_expanded == round_nodes_before {
            break CombatResourceFrontierProbeStopV1::NoProgress;
        }
    };

    retained.sort_by(|left, right| {
        left.outcome
            .total_potion_expenditures
            .cmp(&right.outcome.total_potion_expenditures)
            .then_with(|| right.outcome.final_hp.cmp(&left.outcome.final_hp))
            .then_with(|| {
                left.trajectory
                    .actions
                    .len()
                    .cmp(&right.trajectory.actions.len())
            })
            .then_with(|| left.source_lane_id.cmp(&right.source_lane_id))
    });
    let frontier = retained
        .into_iter()
        .map(|candidate| CombatResourceFrontierCandidateV1 {
            source_lane_id: candidate.source_lane_id,
            first_observed_total_nodes: candidate.first_observed_total_nodes,
            action_count: candidate.trajectory.actions.len(),
            actions: candidate.trajectory.actions,
            outcome: candidate.public_outcome,
            storyboard: candidate.storyboard,
        })
        .collect();
    let frontier_open = stop != CombatResourceFrontierProbeStopV1::AllLanesExhausted;
    Ok(CombatResourceFrontierProbeV1Report {
        schema_name: "CombatResourceFrontierProbeV1Report",
        schema_version: 1,
        input_label: loaded.label.clone(),
        contract: "fixed_root_budgeted_exact_resource_lane_frontier",
        config: config_report,
        stop,
        frontier_open,
        total_nodes_expanded,
        elapsed_ms: started.elapsed().as_millis(),
        frontier_revision,
        lanes: lanes.into_iter().map(lane_report).collect(),
        frontier,
        improvements,
        service_trace,
        notes: vec![
            "every retained line replayed exactly from the unchanged combat root",
            "the frontier contains only resource-nondominated observed wins; it is not a proof of global Pareto optimality",
            "the preserve-all lane and each concrete one-entry-potion lane receive round-robin quanta under one total node and wall budget",
            "a one-potion lane permits but does not require spending its admitted potion, so its search remains anytime rather than exhaustive over a fixed spend class",
            "multi-potion synergy is outside this deliberately small V1 lane set",
            "HP is never converted into potion value; incomparable outcomes remain together",
        ],
    })
}

struct ResourceLaneSpec {
    lane_id: String,
    entry_potion_slot: Option<usize>,
    entry_potion_id: Option<PotionId>,
    allowed_entry_potion_slots: u64,
    max_potions_used: u32,
    potion_policy: CombatSearchV2PotionPolicy,
}

fn resource_lane_specs(
    combat: &crate::runtime::combat::CombatState,
    spending_policy: CombatSearchV2PotionPolicy,
) -> Vec<ResourceLaneSpec> {
    let mut specs = vec![ResourceLaneSpec {
        lane_id: "preserve_all_entry_potions".to_string(),
        entry_potion_slot: None,
        entry_potion_id: None,
        allowed_entry_potion_slots: 0,
        max_potions_used: 0,
        potion_policy: CombatSearchV2PotionPolicy::Never,
    }];
    if spending_policy == CombatSearchV2PotionPolicy::Never {
        return specs;
    }
    specs.extend(
        combat
            .entities
            .potions
            .iter()
            .enumerate()
            .filter_map(|(slot, potion)| {
                let potion = potion.as_ref().filter(|potion| potion.can_use)?;
                let slot_bit = u32::try_from(slot)
                    .ok()
                    .and_then(|slot| 1_u64.checked_shl(slot))?;
                Some(ResourceLaneSpec {
                    lane_id: format!("allow_entry_potion_slot_{slot}"),
                    entry_potion_slot: Some(slot),
                    entry_potion_id: Some(potion.id),
                    allowed_entry_potion_slots: slot_bit,
                    max_potions_used: 1,
                    potion_policy: spending_policy,
                })
            }),
    );
    specs
}

fn merge_snapshot_candidates(
    root: &crate::sim::combat::CombatPosition,
    max_engine_steps_per_action: usize,
    lane_id: &str,
    total_nodes_expanded: u64,
    snapshot: &crate::ai::combat_search_v2::CombatSearchV2DecisionSnapshot,
    retained: &mut Vec<RetainedCandidate>,
) -> Result<bool, String> {
    let mut changed = false;
    for trajectory in snapshot
        .candidate_frontier
        .iter()
        .chain(snapshot.best_win.iter())
    {
        if retained
            .iter()
            .any(|existing| same_action_path(&existing.trajectory, trajectory))
        {
            continue;
        }
        let inputs = trajectory
            .actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>();
        let outcome =
            materialize_combat_witness_outcome_v1(root, &inputs, max_engine_steps_per_action)?;
        let public_outcome = outcome.to_public_outcome_v1(root)?;
        if retained
            .iter()
            .any(|existing| existing.outcome.strictly_dominates(&outcome))
        {
            continue;
        }
        if let Some(equivalent_index) = retained
            .iter()
            .position(|existing| existing.public_outcome == public_outcome)
        {
            if trajectory.actions.len() >= retained[equivalent_index].trajectory.actions.len() {
                continue;
            }
            retained.remove(equivalent_index);
        }
        retained.retain(|existing| !outcome.strictly_dominates(&existing.outcome));
        let storyboard =
            build_combat_witness_storyboard_v1(root, &inputs, max_engine_steps_per_action)?;
        retained.push(RetainedCandidate {
            source_lane_id: lane_id.to_string(),
            first_observed_total_nodes: total_nodes_expanded,
            trajectory: trajectory.clone(),
            outcome,
            public_outcome,
            storyboard,
        });
        changed = true;
    }
    Ok(changed)
}

fn same_action_path(
    left: &CombatSearchV2TrajectoryReport,
    right: &CombatSearchV2TrajectoryReport,
) -> bool {
    left.actions.len() == right.actions.len()
        && left
            .actions
            .iter()
            .zip(&right.actions)
            .all(|(left, right)| left.input == right.input)
}

fn frontier_improvement(
    frontier_revision: u64,
    lane: &ResourceLane,
    total_nodes_expanded: u64,
    retained: &[RetainedCandidate],
) -> CombatResourceFrontierImprovementV1 {
    CombatResourceFrontierImprovementV1 {
        frontier_revision,
        served_lane_id: lane.lane_id.clone(),
        total_nodes_expanded,
        lane_nodes_expanded: lane.session.nodes_expanded(),
        frontier_size: retained.len(),
        preserved_entry_potion_best_hp: retained
            .iter()
            .filter(|candidate| candidate.outcome.starting_potion_expenditure_slots == 0)
            .map(|candidate| candidate.outcome.final_hp)
            .max(),
        spending_best_hp: retained
            .iter()
            .filter(|candidate| candidate.outcome.starting_potion_expenditure_slots != 0)
            .map(|candidate| candidate.outcome.final_hp)
            .max(),
    }
}

#[allow(clippy::too_many_arguments)]
fn service_observation(
    service_index: usize,
    lane: &ResourceLane,
    requested_additional_nodes: usize,
    lane_nodes_before: u64,
    total_nodes_expanded: u64,
    stop: CombatSearchV2AdvanceStop,
    resource_frontier_changed: bool,
    observed_resource_frontier_revision: u64,
) -> CombatResourceFrontierServiceObservationV1 {
    CombatResourceFrontierServiceObservationV1 {
        service_index,
        lane_id: lane.lane_id.clone(),
        requested_additional_nodes,
        nodes_expanded_this_service: lane
            .last_snapshot
            .nodes_expanded
            .saturating_sub(lane_nodes_before),
        total_nodes_expanded,
        lane_nodes_expanded: lane.last_snapshot.nodes_expanded,
        stop,
        lane_candidate_frontier_revision: lane.last_snapshot.candidate_frontier_revision,
        lane_retained_complete_win_count: lane.last_snapshot.candidate_frontier.len(),
        lane_best_win_hp: lane
            .last_snapshot
            .best_win
            .as_ref()
            .map(|trajectory| trajectory.final_hp),
        remaining_work_items: lane.last_snapshot.frontier_work_items,
        exact_state_keys: lane.last_snapshot.exact_state_keys,
        resource_frontier_changed,
        observed_resource_frontier_revision,
    }
}

fn lane_report(lane: ResourceLane) -> CombatResourceFrontierLaneReportV1 {
    CombatResourceFrontierLaneReportV1 {
        lane_id: lane.lane_id,
        entry_potion_slot: lane.entry_potion_slot,
        entry_potion_id: lane.entry_potion_id,
        allowed_entry_potion_slots: lane.allowed_entry_potion_slots,
        max_potions_used: lane.max_potions_used,
        nodes_expanded: lane.last_snapshot.nodes_expanded,
        candidate_frontier_revision: lane.last_snapshot.candidate_frontier_revision,
        retained_complete_win_count: lane.last_snapshot.candidate_frontier.len(),
        remaining_work_items: lane.last_snapshot.frontier_work_items,
        exact_state_keys: lane.last_snapshot.exact_state_keys,
        last_stop: lane.last_stop,
        exhausted: lane.exhausted,
    }
}

fn duration_ms_u64(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::{Potion, PotionId};

    #[test]
    fn lane_specs_protect_preservation_and_isolate_each_usable_entry_potion() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::SteroidPotion, 1)),
            Some(Potion::new(PotionId::FairyPotion, 2)),
            Some(Potion::new(PotionId::FirePotion, 3)),
        ];

        let specs = resource_lane_specs(&combat, CombatSearchV2PotionPolicy::All);

        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].allowed_entry_potion_slots, 0);
        assert_eq!(specs[0].max_potions_used, 0);
        assert_eq!(specs[1].entry_potion_slot, Some(0));
        assert_eq!(specs[1].allowed_entry_potion_slots, 0b001);
        assert_eq!(specs[2].entry_potion_slot, Some(2));
        assert_eq!(specs[2].allowed_entry_potion_slots, 0b100);
    }
}
