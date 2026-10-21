mod admission;
mod config;
mod diagnostics;
mod generation_service;
mod policy_line;
mod potion_budget;
mod public_plan_proposal;
mod reporting;
mod scheduling;
mod service_diagnostics;
mod session;
mod shared_agenda;
mod storage_diagnostics;
mod terminal_outcome;

pub use config::{
    root_initial_expansion_work_for_budget, LocalTurnGraphWitnessConfig,
    PublicPlanTurnProposalConfig, DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
    DEFAULT_ROOT_INITIAL_EXPANSION_WORK,
};
use potion_budget::*;
pub use reporting::*;
use scheduling::*;
pub use service_diagnostics::*;
use shared_agenda::SharedBoundaryAgenda;
pub use storage_diagnostics::LocalTurnGraphStorageSnapshot;
pub use terminal_outcome::LocalTurnGraphTerminalOutcomeSnapshotV1;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::FxBuildHasher;
use smallvec::SmallVec;
use sts_agent::agent::information::state::public_combat_state_v1;
use sts_agent::agent::task::{
    advance_public_combat_plan_v1, public_combat_agenda_for_plan_v1, public_combat_plan_v1,
    PublicCombatPlanStateV1,
};
use sts_combat_strategy::{
    combat_plan_action_timing_v1, combat_plan_has_timed_action_preference_v1,
    combat_plan_projection_v1, combat_plan_selection_member_timing_v1,
    combat_plan_supports_initial_policy_prefix_v1, combat_plan_transition_annotation_v1,
    combat_plan_turn_prefix_proposal_v1, CombatPlanActionTimingV1, CombatPlanPrefixServiceScopeV1,
    CombatPlanTransitionAnnotationV1,
};
use sts_core::ai::combat_state_key::combat_exact_state_key;
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal};
use sts_core::state::core::ClientInput;

use super::generator::TurnOptionGeneratorPreferredLane;
use super::policy::{
    normalized_probabilities, CombatGuideLaneId, CombatPolicyChoice, CombatStateGuide,
    CombatStateGuideRank, SharedCombatActionPolicy,
};
use super::selection_transaction::SelectionTransactionCursor;
use super::trajectory_query::{
    replay_combat_trajectory_events_v1, CombatTrajectoryProgressV1,
    CombatTrajectoryQueryCompileErrorV1, CombatTrajectoryQueryV1, CompiledCombatTrajectoryQueryV1,
};
use super::types::{
    exact_hash, CombatDecisionRoot, CombatPlanningQuantum, CompleteTurnOption,
    CompleteTurnOptionBoundary, CompleteTurnOptionSource, TurnOptionAction,
    TurnOptionGenerationGap,
};
use super::witness::{
    OracleCombatDeepStateSnapshot, OracleCombatWitness, OracleCombatWitnessDiscoverySource,
    OracleCombatWitnessProgressSnapshot, OracleCombatWitnessReplayError,
    OracleCombatWitnessSatisfaction, OracleCombatWitnessStateProgressSnapshot,
};
use super::TurnOptionGeneratorSession;
use sts_combat_option_grounder::{ground_public_combat_agenda_v1, GroundableCompleteTurnV1};
use sts_complete_turn_proposal::{
    exact_complete_turn_proposals_guided_with_deadline_v1, ExactCompleteTurnProposalBoundaryV1,
    ExactCompleteTurnProposalConfigV1, ExactCompleteTurnProposalGuideRankV1,
};

/// Replays one exact terminal candidate from its unchanged combat root.
///
/// This is the shared composition boundary for diagnostic prefixes and
/// planner-produced suffixes. It does not admit the line into a search.
pub fn replay_oracle_combat_witness(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    negative_log_policy: f64,
    discovery_source: OracleCombatWitnessDiscoverySource,
    stepper: &dyn CombatStepper,
) -> Result<OracleCombatWitness, OracleCombatWitnessReplayError> {
    scheduling::replay_witness(
        root,
        actions,
        negative_log_policy,
        discovery_source,
        stepper,
    )
}

/// Projects one replayed terminal witness into the stable typed outcome facts
/// consumed by contract tooling.
pub fn summarize_oracle_combat_witness_outcome(
    root: &CombatPosition,
    witness: &OracleCombatWitness,
    selected_by_local_hp_view: bool,
) -> LocalTurnGraphTerminalOutcomeSnapshotV1 {
    scheduling::terminal_outcome_snapshot(root, witness, selected_by_local_hp_view)
}

fn plan_prefix_root_eligible(position: &CombatPosition) -> bool {
    combat_plan_turn_prefix_proposal_v1(position).is_some_and(|proposal| {
        proposal.service_scope == CombatPlanPrefixServiceScopeV1::RootEligible
    })
}

#[derive(Clone)]
struct LocalRootActionFamilyAccumulator {
    first_action: ClientInput,
    best_root_negative_log_policy: Option<f64>,
    completed_root_turn_options: usize,
    terminal_wins: usize,
    terminal_losses: usize,
    escapes: usize,
}

/// Guide policies currently expose only a handful of fixed semantic lanes.
/// A tree map made every exact node and edge allocate a separate tree node,
/// while all lookups still touched only this tiny set. Keep the same exact
/// lane/rank association in a contiguous inline table; lane order has no
/// search authority and updates remain exact.
#[derive(Clone, Default)]
struct GuideRankMap(SmallVec<[(CombatGuideLaneId, CombatStateGuideRank); 4]>);

impl GuideRankMap {
    fn from_guides(guides: &[CombatStateGuide]) -> Self {
        let mut ranks = Self::default();
        for guide in guides {
            ranks.update_max(guide.lane, &guide.rank);
        }
        ranks
    }

    fn get(&self, lane: &CombatGuideLaneId) -> Option<&CombatStateGuideRank> {
        self.0
            .iter()
            .find_map(|(candidate, rank)| (candidate == lane).then_some(rank))
    }

    fn update_max(&mut self, lane: CombatGuideLaneId, candidate: &CombatStateGuideRank) -> bool {
        if let Some((_, existing)) = self
            .0
            .iter_mut()
            .find(|(candidate_lane, _)| *candidate_lane == lane)
        {
            if *existing >= *candidate {
                return false;
            }
            *existing = candidate.clone();
            return true;
        }
        self.0.push((lane, candidate.clone()));
        true
    }

    fn iter(&self) -> impl Iterator<Item = &(CombatGuideLaneId, CombatStateGuideRank)> {
        self.0.iter()
    }
}

struct GraphNode {
    generator: TurnOptionGeneratorSession,
    /// Controller memory is separate from simulator truth and participates in
    /// planning identity. Equal exact states with different established plan
    /// commitments must not be merged.
    public_plan: PublicCombatPlanStateV1,
    public_plan_proposal_attempted: bool,
    /// Potion resources already expended on the retained path to this exact
    /// boundary. It is part of constrained search identity whenever the
    /// caller supplied a finite combat budget.
    potion_expenditures: u32,
    /// One exact incoming path retained for diagnostics only. Search ownership
    /// and scheduling continue to use the shared exact node.
    diagnostic_parent: Option<(usize, usize)>,
    /// Retained exact path identity used by the shared boundary agendas.
    /// Selection views rank this shared node; they do not recursively own the
    /// path below it.
    path_negative_log_policy: f64,
    path_atomic_depth: usize,
    relative_turn_depth: usize,
    visits: usize,
    first_service_selection: Option<usize>,
    first_guide_service_selection: Option<usize>,
    generated_options: usize,
    children: Vec<GraphEdge>,
    guides: Vec<CombatStateGuide>,
    generation_service_views: Vec<LocalServiceView>,
    next_generation_service_view: usize,
    widen_anchor_visits: usize,
    widen_proposal_root_visits: usize,
    widen_proposal_continuation_visits: usize,
    widen_guide_visits: BTreeMap<CombatGuideLaneId, usize>,
    boundary_anchor_services: usize,
    boundary_proposal_root_services: usize,
    boundary_proposal_continuation_services: usize,
    boundary_guide_services: usize,
    generation_anchor_services: usize,
    generation_guide_services: usize,
    /// Best exact descendant observed for each cheap semantic guide.
    backed_guides: GuideRankMap,
    synced_gaps: usize,
    exhausted: bool,
}

impl GraphNode {
    fn path_cost(&self) -> f64 {
        local_path_base(self.path_atomic_depth, self.path_negative_log_policy)
    }
}

struct GraphEdge {
    successor: usize,
    actions: Vec<TurnOptionAction>,
    negative_log_policy: f64,
    plan_prefix_proposed: bool,
    plan_transition_annotation: Option<CombatPlanTransitionAnnotationV1>,
    visits: usize,
    anchor_visits: usize,
    guide_visits: BTreeMap<CombatGuideLaneId, usize>,
    /// Best exact descendant observed through this edge for each cheap guide.
    backed_guides: GuideRankMap,
    backed_visits: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LocalServiceView {
    Anchor,
    ProposalRoot,
    ProposalContinuation,
    Guide(CombatGuideLaneId),
}

enum SelectedWork {
    Widen {
        node_id: usize,
        path: Vec<(usize, usize)>,
        boundary_service_view: LocalServiceView,
        generation_view: LocalServiceView,
        requested_work: usize,
    },
    Exhausted,
}

fn selected_boundary_generation_work(
    config: &LocalTurnGraphWitnessConfig,
    node_id: usize,
    generator_work: usize,
    service_view: LocalServiceView,
) -> usize {
    if generator_work > 0 {
        // The first semantic guide to reach a fresh exact state pays for one
        // coherent grounding batch. Other independent guides may agree on that
        // same state, but their agreement must not multiply the large batch
        // once per lane. After grounding, every view resumes the shared
        // generator at the ordinary preemption quantum.
        return config.generation_quantum_work;
    }
    if node_id == 0 {
        config.root_initial_expansion_work
    } else if matches!(
        service_view,
        LocalServiceView::ProposalRoot
            | LocalServiceView::ProposalContinuation
            | LocalServiceView::Guide(_)
    ) {
        config.backed_generation_quantum_work
    } else {
        config.initial_expansion_work
    }
}

/// A resumable session. Exact successor nodes and their service statistics are
/// shared across all incoming edges.
pub struct LocalTurnGraphWitnessSession {
    original_root: CombatPosition,
    config: LocalTurnGraphWitnessConfig,
    policy: SharedCombatActionPolicy,
    trajectory_query: Option<Arc<CompiledCombatTrajectoryQueryV1>>,
    public_plan_turn_proposal: Option<PublicPlanTurnProposalConfig>,
    collect_plan_transition_annotations: bool,
    shared_agenda: SharedBoundaryAgenda,
    nodes: Vec<GraphNode>,
    nodes_by_exact_key: HashMap<ConstrainedExactStateKey, usize, FxBuildHasher>,
    used: LocalTurnGraphWitnessCounters,
    performance_timing: LocalTurnGraphPerformanceTiming,
    granted_selections: usize,
    granted_generation_work: usize,
    granted_engine_steps: usize,
    generation_gaps: Vec<TurnOptionGenerationGap>,
    root_action_families: Vec<LocalRootActionFamilyAccumulator>,
    witness: Option<OracleCombatWitness>,
    /// Exact terminal outcomes that are not strictly dominated on typed
    /// combat-resource dimensions. The selected `witness` remains the local
    /// HP-first compatibility view; run-control may adjudicate this frontier
    /// with continuation context.
    witness_frontier: Vec<OracleCombatWitness>,
    replay_failure: Option<OracleCombatWitnessReplayError>,
}

impl Drop for LocalTurnGraphWitnessSession {
    fn drop(&mut self) {
        const MIN_PARALLEL_DROP_NODES: usize = 1_024;
        const MAX_DROP_WORKERS: usize = 4;

        if self.nodes.len() < MIN_PARALLEL_DROP_NODES {
            return;
        }

        // Search is already over and GraphNode ownership is flat: edges refer
        // to successor ids rather than owning child nodes. Release agenda and
        // transposition references first, then let independent node-owned turn
        // generators tear down concurrently. This changes only destruction;
        // no live search state or result ordering is observable here.
        self.shared_agenda.clear();
        self.nodes_by_exact_key.clear();
        let mut nodes = std::mem::take(&mut self.nodes);
        let workers = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .min(MAX_DROP_WORKERS)
            .min(nodes.len());
        if workers <= 1 {
            drop(nodes);
            return;
        }

        let chunk_len = nodes.len().div_ceil(workers);
        let mut chunks = Vec::with_capacity(workers);
        while nodes.len() > chunk_len {
            let split_at = nodes.len() - chunk_len;
            chunks.push(nodes.split_off(split_at));
        }
        chunks.push(nodes);
        let Some(main_chunk) = chunks.pop() else {
            return;
        };
        std::thread::scope(|scope| {
            for chunk in chunks {
                scope.spawn(move || drop(chunk));
            }
            drop(main_chunk);
        });
    }
}

impl LocalTurnGraphWitnessSession {
    /// Enables the shared, plan-aware complete-turn proposal owner before any
    /// graph edge is materialized. Legacy diagnostics remain explicit by not
    /// opting in; the reference actor opts in deliberately.
    pub fn enable_public_plan_turn_proposals(
        &mut self,
        config: PublicPlanTurnProposalConfig,
    ) -> Result<(), LocalTurnGraphPlanAnnotationEnableError> {
        if self.used.exact_edges > 0 {
            return Err(LocalTurnGraphPlanAnnotationEnableError::EdgesAlreadyMaterialized);
        }
        if self.trajectory_query.is_some() {
            return Err(LocalTurnGraphPlanAnnotationEnableError::TrajectoryQueryEnabled);
        }
        self.public_plan_turn_proposal = Some(config);
        Ok(())
    }

    pub fn advance(
        &mut self,
        quantum: LocalTurnGraphWitnessQuantum,
        stepper: &dyn CombatStepper,
    ) -> LocalTurnGraphWitnessReport {
        self.granted_selections = self
            .granted_selections
            .saturating_add(quantum.additional_selections);
        self.granted_generation_work = self
            .granted_generation_work
            .saturating_add(quantum.additional_generation_work);
        self.granted_engine_steps = self
            .granted_engine_steps
            .saturating_add(quantum.additional_engine_steps);

        let status = loop {
            if self.witness_satisfies() {
                break LocalTurnGraphWitnessStatus::WitnessFound;
            }
            if let Some(error) = self.replay_failure.clone() {
                break LocalTurnGraphWitnessStatus::ReplayMismatch(error);
            }
            if deadline_reached(quantum.deadline) {
                break LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::Deadline,
                );
            }
            if self.used.selections >= self.granted_selections {
                break LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::SelectionBudget,
                );
            }
            if self.used.generation_work >= self.granted_generation_work {
                break LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::GenerationWorkBudget,
                );
            }
            if self.used.engine_steps >= self.granted_engine_steps {
                break LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::EngineStepBudget,
                );
            }

            let selection_started = Instant::now();
            let selected_work = self.select_work();
            self.performance_timing.selection_elapsed_ns = self
                .performance_timing
                .selection_elapsed_ns
                .saturating_add(elapsed_nanos_u64(selection_started));
            match selected_work {
                SelectedWork::Widen {
                    node_id,
                    path,
                    boundary_service_view,
                    generation_view,
                    requested_work,
                } => {
                    self.used.selections = self.used.selections.saturating_add(1);
                    if !self.widen(
                        node_id,
                        &path,
                        boundary_service_view,
                        generation_view,
                        requested_work,
                        quantum.deadline,
                        stepper,
                    ) {
                        break LocalTurnGraphWitnessStatus::Partial(
                            if deadline_reached(quantum.deadline) {
                                LocalTurnGraphWitnessInterruption::Deadline
                            } else {
                                LocalTurnGraphWitnessInterruption::EngineStepBudget
                            },
                        );
                    }
                }
                SelectedWork::Exhausted => {
                    break if self.generation_gaps.is_empty() {
                        LocalTurnGraphWitnessStatus::FrontierExhausted
                    } else {
                        LocalTurnGraphWitnessStatus::MechanicsGap
                    };
                }
            }
        };
        self.snapshot(status)
    }

    fn record_root_option(&mut self, option: &CompleteTurnOption) {
        let Some(first_action) = option.actions().first() else {
            return;
        };
        let family_index = self
            .root_action_families
            .iter()
            .position(|family| family.first_action == first_action.input)
            .unwrap_or_else(|| {
                self.root_action_families
                    .push(LocalRootActionFamilyAccumulator {
                        first_action: first_action.input.clone(),
                        best_root_negative_log_policy: None,
                        completed_root_turn_options: 0,
                        terminal_wins: 0,
                        terminal_losses: 0,
                        escapes: 0,
                    });
                self.root_action_families.len() - 1
            });
        let family = &mut self.root_action_families[family_index];
        family.best_root_negative_log_policy = Some(
            family
                .best_root_negative_log_policy
                .map_or(option.negative_log_policy(), |current| {
                    current.min(option.negative_log_policy())
                }),
        );
        family.completed_root_turn_options = family.completed_root_turn_options.saturating_add(1);
        match option.boundary() {
            CompleteTurnOptionBoundary::TerminalWin => {
                family.terminal_wins = family.terminal_wins.saturating_add(1);
            }
            CompleteTurnOptionBoundary::TerminalLoss => {
                family.terminal_losses = family.terminal_losses.saturating_add(1);
            }
            CompleteTurnOptionBoundary::Escape => {
                family.escapes = family.escapes.saturating_add(1);
            }
            CompleteTurnOptionBoundary::NextPlayerTurn => {}
        }
    }

    fn select_work(&mut self) -> SelectedWork {
        let attempts = self.shared_agenda.view_count().max(1);
        for _ in 0..attempts {
            let service_view = self.shared_agenda.next_service_view();
            let selected = match service_view {
                LocalServiceView::Anchor => self.shared_agenda.select_anchor(&self.nodes),
                LocalServiceView::ProposalRoot => {
                    self.shared_agenda.select_proposal_root(&self.nodes)
                }
                LocalServiceView::ProposalContinuation => {
                    self.shared_agenda.select_proposal_continuation(&self.nodes)
                }
                LocalServiceView::Guide(lane) => self.shared_agenda.select_guide(lane, &self.nodes),
            };
            let Some(node_id) = selected else {
                continue;
            };
            let path = self.retained_path_to(node_id);
            self.record_shared_service(node_id, &path, service_view);

            // The agenda view has completed its only job: selecting one
            // shared exact boundary. Expansion belongs to the selected node,
            // whose private generator rotates its own independent lanes.
            let generation_view = {
                let node = &mut self.nodes[node_id];
                let view = node.generation_service_views
                    [node.next_generation_service_view % node.generation_service_views.len()];
                node.next_generation_service_view =
                    node.next_generation_service_view.saturating_add(1);
                view
            };
            if service_view == LocalServiceView::Anchor {
                self.shared_agenda
                    .republish_anchor(node_id, &self.nodes[node_id]);
            }
            let generator_work = self.nodes[node_id].generator.counters().generation_work;
            return SelectedWork::Widen {
                node_id,
                path,
                boundary_service_view: service_view,
                generation_view,
                requested_work: selected_boundary_generation_work(
                    &self.config,
                    node_id,
                    generator_work,
                    service_view,
                ),
            };
        }

        // A guide can be temporarily empty while the anchor still owns live
        // work. Give the anchor one explicit fallback before declaring the
        // shared graph exhausted.
        if let Some(node_id) = self.shared_agenda.select_anchor(&self.nodes) {
            let path = self.retained_path_to(node_id);
            self.record_shared_service(node_id, &path, LocalServiceView::Anchor);
            let generation_view = {
                let node = &mut self.nodes[node_id];
                let view = node.generation_service_views
                    [node.next_generation_service_view % node.generation_service_views.len()];
                node.next_generation_service_view =
                    node.next_generation_service_view.saturating_add(1);
                view
            };
            self.shared_agenda
                .republish_anchor(node_id, &self.nodes[node_id]);
            let generator_work = self.nodes[node_id].generator.counters().generation_work;
            return SelectedWork::Widen {
                node_id,
                path,
                boundary_service_view: LocalServiceView::Anchor,
                generation_view,
                requested_work: selected_boundary_generation_work(
                    &self.config,
                    node_id,
                    generator_work,
                    LocalServiceView::Anchor,
                ),
            };
        }
        SelectedWork::Exhausted
    }

    fn retained_path_to(&self, mut node_id: usize) -> Vec<(usize, usize)> {
        let mut reversed = Vec::with_capacity(self.nodes[node_id].relative_turn_depth);
        while let Some((parent_id, edge_index)) = self.nodes[node_id].diagnostic_parent {
            reversed.push((parent_id, edge_index));
            node_id = parent_id;
        }
        reversed.reverse();
        reversed
    }

    fn record_shared_service(
        &mut self,
        node_id: usize,
        path: &[(usize, usize)],
        view: LocalServiceView,
    ) {
        self.nodes[node_id].visits = self.nodes[node_id].visits.saturating_add(1);
        self.nodes[node_id]
            .first_service_selection
            .get_or_insert(self.used.selections.saturating_add(1));
        self.used.node_visits = self.used.node_visits.saturating_add(1);
        match view {
            LocalServiceView::Anchor => {
                self.nodes[node_id].widen_anchor_visits =
                    self.nodes[node_id].widen_anchor_visits.saturating_add(1);
            }
            LocalServiceView::ProposalRoot => {
                self.nodes[node_id].widen_proposal_root_visits = self.nodes[node_id]
                    .widen_proposal_root_visits
                    .saturating_add(1);
                self.used.plan_prefix_root_services =
                    self.used.plan_prefix_root_services.saturating_add(1);
            }
            LocalServiceView::ProposalContinuation => {
                self.nodes[node_id].widen_proposal_continuation_visits = self.nodes[node_id]
                    .widen_proposal_continuation_visits
                    .saturating_add(1);
                self.used.plan_prefix_continuation_services = self
                    .used
                    .plan_prefix_continuation_services
                    .saturating_add(1);
            }
            LocalServiceView::Guide(lane) => {
                self.nodes[node_id]
                    .first_guide_service_selection
                    .get_or_insert(self.used.selections.saturating_add(1));
                let visits = self.nodes[node_id]
                    .widen_guide_visits
                    .entry(lane)
                    .or_default();
                *visits = visits.saturating_add(1);
            }
        }
        for (parent_id, edge_index) in path {
            let edge = &mut self.nodes[*parent_id].children[*edge_index];
            edge.visits = edge.visits.saturating_add(1);
            match view {
                LocalServiceView::Anchor => {
                    edge.anchor_visits = edge.anchor_visits.saturating_add(1);
                }
                LocalServiceView::ProposalRoot | LocalServiceView::ProposalContinuation => {}
                LocalServiceView::Guide(lane) => {
                    let visits = edge.guide_visits.entry(lane).or_default();
                    *visits = visits.saturating_add(1);
                }
            }
        }
    }

    fn path_actions(&self, path: &[(usize, usize)]) -> (Vec<TurnOptionAction>, f64) {
        let action_count = path
            .iter()
            .map(|(node_id, edge_index)| self.nodes[*node_id].children[*edge_index].actions.len())
            .sum();
        let mut actions = Vec::with_capacity(action_count);
        let mut negative_log_policy = 0.0;
        for (node_id, edge_index) in path {
            let edge = &self.nodes[*node_id].children[*edge_index];
            actions.extend_from_slice(&edge.actions);
            negative_log_policy += edge.negative_log_policy;
        }
        (actions, negative_log_policy)
    }
}

#[cfg(test)]
mod tests;
