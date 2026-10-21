use std::time::Instant;

use serde::Serialize;
use sts_combat_strategy::{
    CombatPlanMilestoneV1, CombatPlanProjectionV1, CombatPlanTransitionAnnotationV1,
};
use sts_core::state::core::ClientInput;

use crate::types::{TurnOptionAction, TurnOptionGenerationGap};
use crate::witness::{OracleCombatWitness, OracleCombatWitnessReplayError};

use super::LocalTurnGraphTerminalOutcomeSnapshotV1;

#[derive(Clone, Copy, Debug)]
pub struct LocalTurnGraphWitnessQuantum {
    pub additional_selections: usize,
    pub additional_generation_work: usize,
    pub additional_engine_steps: usize,
    pub deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalTurnGraphWitnessInterruption {
    SelectionBudget,
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalTurnGraphWitnessStatus {
    WitnessFound,
    Partial(LocalTurnGraphWitnessInterruption),
    FrontierExhausted,
    MechanicsGap,
    ReplayMismatch(OracleCombatWitnessReplayError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalTurnGraphPlanAnnotationEnableError {
    EdgesAlreadyMaterialized,
    TrajectoryQueryEnabled,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalTurnGraphWitnessCounters {
    pub selections: usize,
    pub node_visits: usize,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub exact_nodes: usize,
    pub exact_edges: usize,
    /// Newly materialized exact edges carrying read-only combat-plan facts.
    /// This counter never participates in scheduling or stopping.
    pub annotated_exact_edges: usize,
    pub completed_turn_options: usize,
    pub public_plan_proposal_attempts: usize,
    pub public_plan_proposal_applied_transitions: usize,
    pub public_plan_proposal_completed_options: usize,
    pub public_plan_proposal_retained_options: usize,
    pub public_plan_proposal_grounding_failures: usize,
    pub applied_action_transitions: usize,
    /// Encounter-owned current-turn proposals attempted at exact retained
    /// boundaries. They remain ordinary replayed graph options.
    pub plan_prefix_attempts: usize,
    pub plan_prefix_completed: usize,
    pub plan_prefix_rejections: usize,
    pub plan_prefix_root_enqueues: usize,
    pub plan_prefix_root_services: usize,
    pub plan_prefix_continuation_enqueues: usize,
    pub plan_prefix_continuation_services: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub duplicate_successor_edges: usize,
    /// Exact generated turn options that end in a terminal combat victory.
    pub terminal_win_options: usize,
    /// Exact terminal wins which did not satisfy the explicit path query.
    pub trajectory_query_rejected_terminal_wins: usize,
    /// Terminal candidates replayed authoritatively from the combat root.
    pub witness_replay_attempts: usize,
    /// Authoritative replays that replaced the retained incumbent witness.
    pub witness_replay_improvements: usize,
    /// Authoritative replays that added or improved one typed non-dominated
    /// terminal outcome, even when the local HP-first view did not change.
    pub witness_frontier_changes: usize,
    /// Terminal candidates proven unable to improve the incumbent before
    /// paying for authoritative replay.
    pub witness_replay_dominated_skips: usize,
    pub terminal_losses: usize,
    pub depth_limited_successors: usize,
    pub exhausted_nodes: usize,
    pub maximum_turn_depth: usize,
}

/// Wall-clock diagnostics kept outside deterministic search counters.
///
/// Search budgets and equality contracts never read this structure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphPerformanceTiming {
    pub selection_elapsed_ns: u64,
    pub generation_elapsed_ns: u64,
    pub admission_elapsed_ns: u64,
    pub atomic_expand_elapsed_ns: u64,
    pub transition_simulation_elapsed_ns: u64,
    pub transition_identity_elapsed_ns: u64,
    pub transition_key_build_elapsed_ns: u64,
    pub transition_key_index_elapsed_ns: u64,
    pub transition_admission_elapsed_ns: u64,
    pub transition_trace_elapsed_ns: u64,
    pub transition_seen_elapsed_ns: u64,
    pub transition_publish_elapsed_ns: u64,
    pub transition_publish_trace_node_elapsed_ns: u64,
    pub transition_publish_boundary_elapsed_ns: u64,
    pub transition_publish_complete_elapsed_ns: u64,
    pub transition_publish_push_elapsed_ns: u64,
    pub transition_publish_guide_elapsed_ns: u64,
    pub transition_publish_retain_elapsed_ns: u64,
    pub transition_publish_agenda_elapsed_ns: u64,
    pub admission_root_option_elapsed_ns: u64,
    pub admission_witness_filter_elapsed_ns: u64,
    pub admission_witness_replay_elapsed_ns: u64,
    pub successor_identity_elapsed_ns: u64,
    pub successor_lookup_elapsed_ns: u64,
    pub successor_node_build_elapsed_ns: u64,
    pub successor_edge_elapsed_ns: u64,
    pub successor_backup_elapsed_ns: u64,
    pub admission_refresh_elapsed_ns: u64,
}

impl LocalTurnGraphPerformanceTiming {
    pub(super) fn accumulate(&mut self, other: Self) {
        self.selection_elapsed_ns = self
            .selection_elapsed_ns
            .saturating_add(other.selection_elapsed_ns);
        self.generation_elapsed_ns = self
            .generation_elapsed_ns
            .saturating_add(other.generation_elapsed_ns);
        self.admission_elapsed_ns = self
            .admission_elapsed_ns
            .saturating_add(other.admission_elapsed_ns);
        self.atomic_expand_elapsed_ns = self
            .atomic_expand_elapsed_ns
            .saturating_add(other.atomic_expand_elapsed_ns);
        self.transition_simulation_elapsed_ns = self
            .transition_simulation_elapsed_ns
            .saturating_add(other.transition_simulation_elapsed_ns);
        self.transition_identity_elapsed_ns = self
            .transition_identity_elapsed_ns
            .saturating_add(other.transition_identity_elapsed_ns);
        self.transition_key_build_elapsed_ns = self
            .transition_key_build_elapsed_ns
            .saturating_add(other.transition_key_build_elapsed_ns);
        self.transition_key_index_elapsed_ns = self
            .transition_key_index_elapsed_ns
            .saturating_add(other.transition_key_index_elapsed_ns);
        self.transition_admission_elapsed_ns = self
            .transition_admission_elapsed_ns
            .saturating_add(other.transition_admission_elapsed_ns);
        self.transition_trace_elapsed_ns = self
            .transition_trace_elapsed_ns
            .saturating_add(other.transition_trace_elapsed_ns);
        self.transition_seen_elapsed_ns = self
            .transition_seen_elapsed_ns
            .saturating_add(other.transition_seen_elapsed_ns);
        self.transition_publish_elapsed_ns = self
            .transition_publish_elapsed_ns
            .saturating_add(other.transition_publish_elapsed_ns);
        self.transition_publish_trace_node_elapsed_ns = self
            .transition_publish_trace_node_elapsed_ns
            .saturating_add(other.transition_publish_trace_node_elapsed_ns);
        self.transition_publish_boundary_elapsed_ns = self
            .transition_publish_boundary_elapsed_ns
            .saturating_add(other.transition_publish_boundary_elapsed_ns);
        self.transition_publish_complete_elapsed_ns = self
            .transition_publish_complete_elapsed_ns
            .saturating_add(other.transition_publish_complete_elapsed_ns);
        self.transition_publish_push_elapsed_ns = self
            .transition_publish_push_elapsed_ns
            .saturating_add(other.transition_publish_push_elapsed_ns);
        self.transition_publish_guide_elapsed_ns = self
            .transition_publish_guide_elapsed_ns
            .saturating_add(other.transition_publish_guide_elapsed_ns);
        self.transition_publish_retain_elapsed_ns = self
            .transition_publish_retain_elapsed_ns
            .saturating_add(other.transition_publish_retain_elapsed_ns);
        self.transition_publish_agenda_elapsed_ns = self
            .transition_publish_agenda_elapsed_ns
            .saturating_add(other.transition_publish_agenda_elapsed_ns);
        self.admission_root_option_elapsed_ns = self
            .admission_root_option_elapsed_ns
            .saturating_add(other.admission_root_option_elapsed_ns);
        self.admission_witness_filter_elapsed_ns = self
            .admission_witness_filter_elapsed_ns
            .saturating_add(other.admission_witness_filter_elapsed_ns);
        self.admission_witness_replay_elapsed_ns = self
            .admission_witness_replay_elapsed_ns
            .saturating_add(other.admission_witness_replay_elapsed_ns);
        self.successor_identity_elapsed_ns = self
            .successor_identity_elapsed_ns
            .saturating_add(other.successor_identity_elapsed_ns);
        self.successor_lookup_elapsed_ns = self
            .successor_lookup_elapsed_ns
            .saturating_add(other.successor_lookup_elapsed_ns);
        self.successor_node_build_elapsed_ns = self
            .successor_node_build_elapsed_ns
            .saturating_add(other.successor_node_build_elapsed_ns);
        self.successor_edge_elapsed_ns = self
            .successor_edge_elapsed_ns
            .saturating_add(other.successor_edge_elapsed_ns);
        self.successor_backup_elapsed_ns = self
            .successor_backup_elapsed_ns
            .saturating_add(other.successor_backup_elapsed_ns);
        self.admission_refresh_elapsed_ns = self
            .admission_refresh_elapsed_ns
            .saturating_add(other.admission_refresh_elapsed_ns);
    }
}

#[derive(Clone, Debug)]
pub struct LocalTurnGraphWitnessReport {
    pub status: LocalTurnGraphWitnessStatus,
    pub counters: LocalTurnGraphWitnessCounters,
    pub performance_timing: LocalTurnGraphPerformanceTiming,
    pub root_visits: usize,
    pub root_generated_options: usize,
    pub root_children: usize,
    pub generation_gaps: Vec<TurnOptionGenerationGap>,
    pub witness: Option<OracleCombatWitness>,
    pub witness_frontier: Vec<LocalTurnGraphTerminalOutcomeSnapshotV1>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphPolicyLineReport {
    pub proposed_turns: usize,
    pub chosen_action_transitions: usize,
    pub proposed_actions: Vec<ClientInput>,
    pub rejected_preview_transitions: usize,
    pub deferred_actions: usize,
    pub engine_steps: usize,
    pub legal_surface_elapsed_ns: u64,
    pub policy_ranking_elapsed_ns: u64,
    pub transition_preview_elapsed_ns: u64,
    pub action_identity_elapsed_ns: u64,
    pub plan_annotation_elapsed_ns: u64,
    pub successor_admission_elapsed_ns: u64,
    pub reached_terminal_win: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphStateSnapshot {
    pub exact_state_hash: String,
    pub relative_turn_depth: usize,
    pub visits: usize,
    pub first_service_selection: Option<usize>,
    pub first_guide_service_selection: Option<usize>,
    pub generation_work: usize,
    pub generator_engine_steps: usize,
    pub retained_generator_work_items: usize,
    pub generator_anchor_work_pops: usize,
    pub generator_guided_work_pops: usize,
    pub best_retained_anchor_atomic_depth: Option<usize>,
    pub retained_guide_promises: Vec<LocalTurnGraphRetainedGuidePromiseSnapshot>,
    pub generated_options: usize,
    pub children: usize,
    pub exhausted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphRetainedGuidePromiseSnapshot {
    pub lane: u32,
    pub rank: Vec<i32>,
    pub atomic_depth: usize,
}

/// Read-only root-action attribution using the local graph's own semantics.
///
/// Descendant counts are non-exclusive reachability counts: an exact node
/// shared by two root-action families is truthfully reachable from both.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphRootActionFamilySnapshot {
    pub first_action: ClientInput,
    pub best_root_negative_log_policy: Option<f64>,
    pub completed_root_turn_options: usize,
    pub terminal_wins: usize,
    pub terminal_losses: usize,
    pub escapes: usize,
    pub unique_next_turn_successors: usize,
    pub retained_next_turn_successors: usize,
    pub reachable_exact_states: usize,
    pub reachable_retained_states: usize,
    pub reachable_generation_work: usize,
    pub reachable_completed_turn_options: usize,
    pub max_player_turn: u32,
    pub best_hp_at_max_turn: Option<i32>,
    pub lowest_enemy_hp_at_max_turn: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphGuideServiceSnapshot {
    pub lane: u32,
    pub edge_visits: usize,
    /// Rank among siblings of the observed parent. This is a local diagnostic,
    /// not the production scheduler's queue position.
    pub sibling_ordinal_rank: usize,
    pub sibling_candidate_count: usize,
    pub successor_rank: Vec<i32>,
    pub sibling_best_rank: Vec<i32>,
    /// Actual position in the shared global guide agenda. `None` means this
    /// one-shot guide has already serviced the state or was never published.
    pub global_ordinal_rank: Option<usize>,
    pub global_candidate_count: usize,
    pub global_best_rank: Vec<i32>,
}

/// One already-materialized exact edge in the local turn graph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphEdgeSnapshot {
    pub parent_visits: usize,
    pub parent_generated_options: usize,
    pub parent_children: usize,
    pub parent_widen_anchor_visits: usize,
    pub actions: Vec<TurnOptionAction>,
    pub negative_log_policy: f64,
    pub plan_prefix_proposed: bool,
    pub plan_transition_annotation: Option<CombatPlanTransitionAnnotationV1>,
    pub visits: usize,
    pub anchor_visits: usize,
    pub backed_visits: usize,
    pub successor_path_cost: f64,
    pub successor_anchor_ordinal_rank: Option<usize>,
    pub successor_anchor_candidate_count: usize,
    pub successor_anchor_service_cost: Option<f64>,
    pub best_anchor_service_cost: Option<f64>,
    pub guide_service: Vec<LocalTurnGraphGuideServiceSnapshot>,
    pub successor_visits: usize,
    pub successor_generated_options: usize,
    pub successor_children: usize,
    pub successor_exhausted: bool,
}

/// One exact graph edge carrying an encounter-owned plan annotation.
///
/// This diagnostic view deliberately exposes service facts without
/// interpreting the annotation. Encounter semantics remain owned by the
/// strategy crate and are never read by local-graph scheduling.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphPlanTransitionEdgeSnapshot {
    pub parent_exact_state_hash: String,
    pub successor_exact_state_hash: String,
    pub parent_relative_turn_depth: usize,
    pub action_count: usize,
    pub negative_log_policy: f64,
    pub plan_transition_annotation: CombatPlanTransitionAnnotationV1,
    pub edge_visits: usize,
    pub anchor_visits: usize,
    pub guide_visits: usize,
    pub backed_visits: usize,
    pub successor_visits: usize,
}

/// One replayable root-to-successor path whose final exact edge carries an
/// encounter-owned plan annotation.
///
/// Unlike the service-oriented edge snapshot above, this value owns the
/// complete retained diagnostic action prefix. It is read-only evidence: the
/// local graph never ranks or schedules from the annotation, and callers must
/// replay the actions authoritatively before accepting a milestone.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphPlanTransitionPathSnapshot {
    pub successor_exact_state_hash: String,
    pub root_actions: Vec<TurnOptionAction>,
    /// Number of root-prefix actions before the annotated complete-turn edge.
    pub parent_action_count: usize,
    pub path_negative_log_policy: f64,
    pub plan_transition_annotation: CombatPlanTransitionAnnotationV1,
}

/// One retained exact graph node whose encounter plan is ready for a bounded
/// one-turn milestone closure probe.
///
/// The graph exports only a replayable path and typed facts. It neither grants
/// work to the node nor interprets the milestone as a combat outcome.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphMilestoneClosurePathSnapshot {
    pub exact_state_hash: String,
    pub root_actions: Vec<TurnOptionAction>,
    pub path_negative_log_policy: f64,
    pub target_milestone: CombatPlanMilestoneV1,
    pub plan: CombatPlanProjectionV1,
    pub visits: usize,
    pub generation_work: usize,
}
