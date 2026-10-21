use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxBuildHasher, FxHasher};
use sts_core::ai::combat_state_key::{combat_exact_state_key, CombatExactStateKey};
use sts_core::sim::combat::{CombatPosition, CombatStepper, CombatTerminal};
use sts_core::state::core::{ClientInput, EngineState};

use super::policy::{
    normalized_probabilities, uniform_policy, CombatPolicyChoice, CombatStateGuide,
    SharedCombatActionPolicy,
};
#[cfg(test)]
use super::policy::{CombatGuideLaneId, CombatStateGuideRank};
use super::selection_transaction::SelectionTransactionCursor;
use super::trajectory_query::{CombatTrajectoryProgressV1, CompiledCombatTrajectoryQueryV1};
use super::types::{
    exact_hash, supported_boundary, CombatDecisionRoot, CombatPlanningCounters,
    CombatPlanningQuantum, CompleteTurnOption, GenerationInterruption, ReplaySuccessorHash,
    TurnOptionAction, TurnOptionGenerationGap, TurnOptionGenerationGapKind,
    TurnOptionGenerationReport, TurnOptionGenerationStatus, TurnOptionGeneratorConfig,
};
pub use diagnostics::LiveActionTransitionSnapshot;
use guide_frontier::GuidedGeneratorFrontier;
#[cfg(test)]
use guide_frontier::GuidedGeneratorQueueEntry;
pub(crate) use scheduling::TurnOptionGeneratorPreferredLane;
use scheduling::{GeneratorQueueEntry, GeneratorWorkPriority};
use selection_intent::SELECTION_INTENT_LANE;
pub use selection_intent::{
    SelectionIntent, SelectionIntentError, SelectionSource, SelectionSourcePage,
};
#[cfg(test)]
use transition::detail_timing_scale;
use transition::{is_potion_expenditure, ActionTransitionStatus};
use work_slots::GeneratorWorkHandle;

pub(crate) mod diagnostics;
mod guide_frontier;
mod lifecycle;
mod prefix_proposal;
mod reclamation;
mod scheduling;
mod selection_intent;
mod transition;
mod work_slots;

/// High-frequency diagnostic sub-timers sample one transition per interval.
/// Parent stage timers remain exhaustive; this keeps Windows QPC overhead
/// from becoming a material part of the search being measured.
pub const DETAIL_TIMING_SAMPLE_INTERVAL: usize = 16;

enum PlanPrefixAdvance {
    NotServiced,
    Serviced,
    Interrupted(GenerationInterruption),
}

#[derive(Clone, Debug)]
struct PartialTurnOption {
    position: CombatPosition,
    trace: Option<Arc<PendingActionTrace>>,
    atomic_depth: usize,
    negative_log_policy: f64,
    potion_expenditures: u32,
    generation_guides: Option<Arc<[CombatStateGuide]>>,
    trajectory_progress: Option<CombatTrajectoryProgressV1>,
}

#[derive(Debug)]
struct PendingActionTrace {
    // Generator branches share exact prefixes. Durable replay hashes are
    // materialized only when a complete segment is published, then cached on
    // the shared prefix node so sibling segments never re-hash that state.
    parent: Option<Arc<Self>>,
    input: ClientInput,
    successor_key: Arc<CombatExactStateKey>,
    engine_steps: usize,
    depth: usize,
}

impl PartialTurnOption {
    fn action_depth(&self) -> usize {
        self.trace.as_ref().map_or(0, |trace| trace.depth)
    }

    fn materialize_actions(&self) -> Vec<TurnOptionAction> {
        let mut actions = Vec::with_capacity(self.action_depth());
        let mut cursor = self.trace.clone();
        while let Some(trace) = cursor {
            actions.push(TurnOptionAction {
                input: trace.input.clone(),
                expected_successor_hash: ReplaySuccessorHash::from_exact_key(
                    trace.successor_key.clone(),
                ),
                engine_steps: trace.engine_steps,
            });
            cursor = trace.parent.clone();
        }
        actions.reverse();
        actions
    }
}

#[derive(Clone, Debug)]
struct ActionTransitionWork {
    parent: Arc<PartialTurnOption>,
    input: ClientInput,
    atomic_depth: usize,
    negative_log_policy: f64,
}

#[derive(Clone, Debug)]
struct AtomicActionCandidate {
    input: ClientInput,
    conditional_probability: f64,
    negative_log_policy: f64,
}

#[derive(Clone, Debug)]
struct AtomicActionCursorWork {
    parent: Arc<PartialTurnOption>,
    candidates: Vec<AtomicActionCandidate>,
    next_candidate: usize,
    guides: Arc<[CombatStateGuide]>,
}

impl AtomicActionCursorWork {
    fn new(
        parent: Arc<PartialTurnOption>,
        inputs: Vec<ClientInput>,
        probabilities: Vec<f64>,
        guides: impl Into<Arc<[CombatStateGuide]>>,
    ) -> Option<Self> {
        let mut candidates = inputs
            .into_iter()
            .zip(probabilities)
            .map(|(input, conditional_probability)| AtomicActionCandidate {
                input,
                conditional_probability,
                negative_log_policy: parent.negative_log_policy - conditional_probability.ln(),
            })
            .collect::<Vec<_>>();
        // Stable ordering preserves the simulator's canonical surface order
        // for equal policy mass while exposing the most likely concrete edge
        // first.
        candidates.sort_by(|left, right| {
            right
                .conditional_probability
                .total_cmp(&left.conditional_probability)
        });
        (!candidates.is_empty()).then_some(Self {
            parent,
            candidates,
            next_candidate: 0,
            guides: guides.into(),
        })
    }

    fn current_transition(&self) -> Option<ActionTransitionWork> {
        let candidate = self.candidates.get(self.next_candidate)?;
        Some(ActionTransitionWork {
            parent: self.parent.clone(),
            input: candidate.input.clone(),
            atomic_depth: self.parent.atomic_depth.saturating_add(1),
            negative_log_policy: candidate.negative_log_policy,
        })
    }

    fn consume_current(&mut self) {
        self.next_candidate = self.next_candidate.saturating_add(1);
    }

    fn remaining_candidate_count(&self) -> usize {
        self.candidates.len().saturating_sub(self.next_candidate)
    }

    fn priority(&self) -> Option<GeneratorWorkPriority> {
        let remaining_probability = self.candidates[self.next_candidate..]
            .iter()
            .map(|candidate| candidate.conditional_probability)
            .sum::<f64>();
        (remaining_probability > 0.0).then(|| {
            GeneratorWorkPriority::for_path(
                self.parent.atomic_depth.saturating_add(1),
                self.parent.negative_log_policy - remaining_probability.ln(),
            )
        })
    }
}

#[derive(Clone, Debug)]
struct StructuredSelectionWork {
    parent: Arc<PartialTurnOption>,
    cursor: SelectionTransactionCursor,
    family_negative_log_policy: f64,
    remaining_conditional_mass: f64,
}

impl StructuredSelectionWork {
    fn current_transition(&self) -> Option<ActionTransitionWork> {
        let input = self.cursor.peek_input()?;
        let conditional_mass =
            self.remaining_conditional_mass / self.cursor.remaining_input_count().max(1) as f64;
        Some(ActionTransitionWork {
            parent: self.parent.clone(),
            input,
            atomic_depth: self.parent.atomic_depth.saturating_add(1),
            negative_log_policy: self.family_negative_log_policy - conditional_mass.ln(),
        })
    }

    fn consume_current(&mut self) {
        let conditional_mass =
            self.remaining_conditional_mass / self.cursor.remaining_input_count().max(1) as f64;
        self.cursor.consume_current();
        self.remaining_conditional_mass -= conditional_mass;
    }

    fn priority(&self) -> Option<GeneratorWorkPriority> {
        (!self.cursor.is_exhausted()).then(|| {
            GeneratorWorkPriority::for_path(
                self.parent.atomic_depth.saturating_add(1),
                self.family_negative_log_policy - self.remaining_conditional_mass.ln(),
            )
        })
    }
}

#[derive(Clone, Debug)]
enum GeneratorWork {
    // Expansion immediately shares its immutable parent with every outgoing
    // cursor. Owning that parent through Arc here avoids inlining a full
    // CombatPosition into every reusable work slot without adding an
    // allocation to the ordinary expanded path.
    Expand(Arc<PartialTurnOption>),
    AtomicActions(AtomicActionCursorWork),
    StructuredSelection(StructuredSelectionWork),
}

impl GeneratorWork {
    fn position(&self) -> &CombatPosition {
        match self {
            Self::Expand(partial) => &partial.position,
            Self::AtomicActions(actions) => &actions.parent.position,
            Self::StructuredSelection(selection) => &selection.parent.position,
        }
    }
}

#[derive(Clone, Debug)]
struct IndexedExactStateKey {
    // This hash is private to the in-memory transposition set. Equality still
    // compares the complete typed key, so it cannot change exact-state
    // semantics or the durable v1 hashes written to witnesses.
    structural_hash: u64,
    key: Arc<CombatExactStateKey>,
    /// Finite caller-owned resources are part of constrained search identity.
    /// Without a finite potion contract, `None` preserves ordinary exact-state
    /// transposition.
    potion_expenditures: Option<u32>,
    trajectory_progress: Option<CombatTrajectoryProgressV1>,
}

impl PartialEq for IndexedExactStateKey {
    fn eq(&self, other: &Self) -> bool {
        self.structural_hash == other.structural_hash
            && self.key == other.key
            && self.potion_expenditures == other.potion_expenditures
            && self.trajectory_progress == other.trajectory_progress
    }
}

impl Eq for IndexedExactStateKey {}

impl Hash for IndexedExactStateKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.structural_hash.hash(state);
        self.potion_expenditures.hash(state);
        self.trajectory_progress.hash(state);
    }
}

impl IndexedExactStateKey {
    #[cfg(test)]
    fn new(
        key: CombatExactStateKey,
        potion_expenditures: Option<u32>,
        trajectory_progress: Option<CombatTrajectoryProgressV1>,
    ) -> Self {
        Self::from_arc(Arc::new(key), potion_expenditures, trajectory_progress)
    }

    fn from_arc(
        key: Arc<CombatExactStateKey>,
        potion_expenditures: Option<u32>,
        trajectory_progress: Option<CombatTrajectoryProgressV1>,
    ) -> Self {
        // This is a trusted, process-local bucket hash. Full typed equality is
        // still checked below, so collisions cannot merge simulator states.
        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        Self {
            structural_hash: hasher.finish(),
            key,
            potion_expenditures,
            trajectory_progress,
        }
    }
}

pub struct TurnOptionGeneratorSession {
    root: CombatDecisionRoot,
    config: TurnOptionGeneratorConfig,
    /// Maximum potion uses/discards allowed inside this generated turn.
    ///
    /// The run-level graph supplies the remaining combat allowance at each
    /// exact turn boundary. Enforcing it here prevents an over-budget potion
    /// prefix from consuming the complete-turn search before a legal line is
    /// ever released.
    max_potion_expenditures: Option<u32>,
    trajectory_query: Option<Arc<CompiledCombatTrajectoryQueryV1>>,
    initial_trajectory_progress: Option<CombatTrajectoryProgressV1>,
    policy: SharedCombatActionPolicy,
    work: Vec<Option<GeneratorWork>>,
    work_sequence_ids: Vec<u64>,
    free_work_ids: Vec<usize>,
    /// Number of guided-heap memberships published for each stable work slot.
    ///
    /// Work consumption can then maintain the live membership total without
    /// rescanning policy guides or every scheduling heap.
    guide_entries_per_work: Vec<usize>,
    anchor_frontier: BinaryHeap<GeneratorQueueEntry>,
    guided_frontiers: Vec<GuidedGeneratorFrontier>,
    selection_intent_members: Vec<ClientInput>,
    selection_source_owner: Arc<()>,
    root_selection_source: std::sync::OnceLock<SelectionSource>,
    selection_intent_source: Option<SelectionSource>,
    selection_intent_frontier: BinaryHeap<GeneratorQueueEntry>,
    next_scheduler_lane: usize,
    /// Frozen heads which still belong to the current scheduling round.
    ///
    /// This must survive `advance` boundaries. Re-snapshotting after every
    /// wall-clock or work interruption lets newly published heads repeatedly
    /// overtake the unserved tail of the prior round, making split quanta
    /// semantically different from one continuous grant.
    scheduled_round: VecDeque<(usize, GeneratorWorkHandle)>,
    live_work_items: usize,
    live_guide_entries: usize,
    /// Observational counters for deterministic stale-entry rebuilds.
    scheduling_rebuilds: usize,
    reused_work_slots: usize,
    reclaimed_anchor_entries: usize,
    reclaimed_guide_entries: usize,
    next_sequence_id: u64,
    seen: HashSet<IndexedExactStateKey, FxBuildHasher>,
    completed: Vec<CompleteTurnOption>,
    total_completed_options: usize,
    plan_prefix_attempted: bool,
    plan_prefix_attempts: usize,
    plan_prefix_completed: usize,
    plan_prefix_rejections: usize,
    gaps: Vec<TurnOptionGenerationGap>,
    applied_action_transitions: usize,
    duplicate_exact_successors: usize,
    atomic_state_expansions: usize,
    anchor_work_pops: usize,
    guided_work_pops: usize,
    atomic_expand_elapsed_ns: u64,
    transition_simulation_elapsed_ns: u64,
    transition_identity_elapsed_ns: u64,
    transition_key_build_elapsed_ns: u64,
    transition_key_index_elapsed_ns: u64,
    transition_admission_elapsed_ns: u64,
    transition_trace_elapsed_ns: u64,
    transition_seen_elapsed_ns: u64,
    transition_publish_elapsed_ns: u64,
    transition_publish_trace_node_elapsed_ns: u64,
    transition_publish_boundary_elapsed_ns: u64,
    transition_publish_complete_elapsed_ns: u64,
    transition_publish_push_elapsed_ns: u64,
    transition_publish_guide_elapsed_ns: u64,
    transition_publish_retain_elapsed_ns: u64,
    transition_publish_agenda_elapsed_ns: u64,
    used: CombatPlanningCounters,
    granted: CombatPlanningCounters,
}

impl TurnOptionGeneratorSession {
    pub fn new(root: CombatDecisionRoot, config: TurnOptionGeneratorConfig) -> Self {
        Self::with_policy(root, config, uniform_policy())
    }

    pub fn with_policy(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        policy: SharedCombatActionPolicy,
    ) -> Self {
        Self::with_policy_and_constraints(root, config, policy, None, None, None)
    }

    pub(crate) fn with_policy_and_potion_limit(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        policy: SharedCombatActionPolicy,
        max_potion_expenditures: Option<u32>,
    ) -> Self {
        Self::with_policy_and_constraints(root, config, policy, max_potion_expenditures, None, None)
    }

    pub(crate) fn with_policy_potion_limit_and_trajectory_query(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        policy: SharedCombatActionPolicy,
        max_potion_expenditures: Option<u32>,
        trajectory_query: Arc<CompiledCombatTrajectoryQueryV1>,
        initial_trajectory_progress: CombatTrajectoryProgressV1,
    ) -> Self {
        Self::with_policy_and_constraints(
            root,
            config,
            policy,
            max_potion_expenditures,
            Some(trajectory_query),
            Some(initial_trajectory_progress),
        )
    }

    fn with_policy_and_constraints(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        policy: SharedCombatActionPolicy,
        max_potion_expenditures: Option<u32>,
        trajectory_query: Option<Arc<CompiledCombatTrajectoryQueryV1>>,
        initial_trajectory_progress: Option<CombatTrajectoryProgressV1>,
    ) -> Self {
        debug_assert_eq!(
            trajectory_query.is_some(),
            initial_trajectory_progress.is_some()
        );
        let max_potion_expenditures = if config.allow_potion_expenditure {
            max_potion_expenditures
        } else {
            Some(0)
        };
        let mut seen = HashSet::with_hasher(FxBuildHasher);
        let root_key = root.exact_state_key().cloned().unwrap_or_else(|| {
            Arc::new(combat_exact_state_key(
                &root.position().engine,
                &root.position().combat,
            ))
        });
        seen.insert(IndexedExactStateKey::from_arc(
            root_key,
            max_potion_expenditures.map(|_| 0),
            initial_trajectory_progress.clone(),
        ));
        let root_work = GeneratorWork::Expand(Arc::new(PartialTurnOption {
            position: root.position().clone(),
            trace: None,
            atomic_depth: 0,
            negative_log_policy: 0.0,
            potion_expenditures: 0,
            generation_guides: None,
            trajectory_progress: initial_trajectory_progress.clone(),
        }));
        let mut session = Self {
            root,
            config,
            max_potion_expenditures,
            trajectory_query,
            initial_trajectory_progress,
            policy,
            work: Vec::new(),
            work_sequence_ids: Vec::new(),
            free_work_ids: Vec::new(),
            guide_entries_per_work: Vec::new(),
            anchor_frontier: BinaryHeap::new(),
            guided_frontiers: Vec::new(),
            selection_intent_members: Vec::new(),
            selection_source_owner: Arc::new(()),
            root_selection_source: std::sync::OnceLock::new(),
            selection_intent_source: None,
            selection_intent_frontier: BinaryHeap::new(),
            next_scheduler_lane: 0,
            scheduled_round: VecDeque::new(),
            live_work_items: 0,
            live_guide_entries: 0,
            scheduling_rebuilds: 0,
            reused_work_slots: 0,
            reclaimed_anchor_entries: 0,
            reclaimed_guide_entries: 0,
            next_sequence_id: 0,
            seen,
            completed: Vec::new(),
            total_completed_options: 0,
            plan_prefix_attempted: false,
            plan_prefix_attempts: 0,
            plan_prefix_completed: 0,
            plan_prefix_rejections: 0,
            gaps: Vec::new(),
            applied_action_transitions: 0,
            duplicate_exact_successors: 0,
            atomic_state_expansions: 0,
            anchor_work_pops: 0,
            guided_work_pops: 0,
            atomic_expand_elapsed_ns: 0,
            transition_simulation_elapsed_ns: 0,
            transition_identity_elapsed_ns: 0,
            transition_key_build_elapsed_ns: 0,
            transition_key_index_elapsed_ns: 0,
            transition_admission_elapsed_ns: 0,
            transition_trace_elapsed_ns: 0,
            transition_seen_elapsed_ns: 0,
            transition_publish_elapsed_ns: 0,
            transition_publish_trace_node_elapsed_ns: 0,
            transition_publish_boundary_elapsed_ns: 0,
            transition_publish_complete_elapsed_ns: 0,
            transition_publish_push_elapsed_ns: 0,
            transition_publish_guide_elapsed_ns: 0,
            transition_publish_retain_elapsed_ns: 0,
            transition_publish_agenda_elapsed_ns: 0,
            used: CombatPlanningCounters::default(),
            granted: CombatPlanningCounters::default(),
        };
        session.push_work(root_work, GeneratorWorkPriority::for_path(0, 0.0));
        session
    }

    pub fn root(&self) -> &CombatDecisionRoot {
        &self.root
    }

    pub fn completed_options(&self) -> &[CompleteTurnOption] {
        &self.completed
    }

    pub(crate) fn take_completed_options(&mut self) -> Vec<CompleteTurnOption> {
        std::mem::take(&mut self.completed)
    }

    pub(crate) fn publish_external_completed_options(
        &mut self,
        options: impl IntoIterator<Item = CompleteTurnOption>,
    ) {
        assert!(
            self.trajectory_query.is_none(),
            "external complete-turn proposals must project trajectory progress before query admission"
        );
        for option in options {
            self.publish_completed(option);
        }
    }

    pub(crate) fn total_completed_options(&self) -> usize {
        self.total_completed_options
    }

    pub fn is_finished(&self) -> bool {
        self.live_work_items == 0
    }

    pub fn advance(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: CombatPlanningQuantum,
    ) -> TurnOptionGenerationReport {
        self.advance_internal(stepper, quantum, false, false, false)
    }

    pub(crate) fn advance_with_root_proposal(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: CombatPlanningQuantum,
    ) -> TurnOptionGenerationReport {
        self.advance_internal(stepper, quantum, true, false, true)
    }

    pub(crate) fn advance_with_continuation_proposal(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: CombatPlanningQuantum,
    ) -> TurnOptionGenerationReport {
        self.advance_internal(stepper, quantum, true, true, true)
    }

    fn advance_internal(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: CombatPlanningQuantum,
        allow_root_eligible_proposal: bool,
        allow_continuation_only_proposal: bool,
        stop_after_plan_prefix_service: bool,
    ) -> TurnOptionGenerationReport {
        let before = self.used;
        let before_diagnostics = self.diagnostics();
        let completed_before = self.total_completed_options;
        self.granted = self.granted.saturating_add(CombatPlanningCounters {
            generation_work: quantum.additional_generation_work,
            engine_steps: quantum.additional_engine_steps,
        });
        let prefix_advance = self.advance_plan_prefix_proposal(
            stepper,
            quantum.deadline,
            allow_root_eligible_proposal,
            allow_continuation_only_proposal,
        );
        // Freeze one current head from every scheduling view before serving
        // the round. Without this boundary, work expanded by an earlier lane
        // can publish a new head into a later lane and repeatedly overtake the
        // item which was already first there. A finite pending transaction can
        // consequently remain live forever despite round-robin lane service.
        let (interruption, plan_prefix_serviced) = match prefix_advance {
            PlanPrefixAdvance::Interrupted(cause) => (Some(cause), false),
            PlanPrefixAdvance::Serviced if stop_after_plan_prefix_service => (None, true),
            PlanPrefixAdvance::NotServiced | PlanPrefixAdvance::Serviced => {
                let interruption = loop {
                    if self.is_finished() {
                        break None;
                    }
                    if deadline_reached(quantum.deadline) {
                        break Some(GenerationInterruption::Deadline);
                    }
                    if self.used.generation_work >= self.granted.generation_work {
                        break Some(GenerationInterruption::GenerationWorkBudget);
                    }
                    while self
                        .scheduled_round
                        .front()
                        .is_some_and(|(_, handle)| !self.is_live_work_handle(*handle))
                    {
                        self.scheduled_round.pop_front();
                    }
                    if self.scheduled_round.is_empty() {
                        self.reclaim_stale_scheduling_entries();
                        self.scheduled_round = self.snapshot_scheduling_round();
                        if self.scheduled_round.is_empty() {
                            debug_assert!(self.is_finished());
                            break None;
                        }
                    }
                    let (lane, handle) = *self
                        .scheduled_round
                        .front()
                        .expect("a non-empty scheduling round has a head");
                    let transition_reservation = self.config.max_engine_steps_per_transition.max(1);
                    if self.live_work(handle).is_some_and(|work| {
                        matches!(
                            work,
                            GeneratorWork::AtomicActions(_) | GeneratorWork::StructuredSelection(_)
                        )
                    }) && self
                        .granted
                        .engine_steps
                        .saturating_sub(self.used.engine_steps)
                        < transition_reservation
                    {
                        break Some(GenerationInterruption::EngineStepBudget);
                    }

                    self.scheduled_round.pop_front();
                    let work = self.take_live_work(handle);
                    if lane == 0 {
                        self.anchor_work_pops = self.anchor_work_pops.saturating_add(1);
                    } else {
                        self.guided_work_pops = self.guided_work_pops.saturating_add(1);
                    }
                    if lane != SELECTION_INTENT_LANE {
                        self.next_scheduler_lane =
                            (lane + 1) % self.guided_frontiers.len().saturating_add(1);
                    }
                    self.used.generation_work = self.used.generation_work.saturating_add(1);
                    match work {
                        GeneratorWork::Expand(partial) => {
                            self.atomic_state_expansions =
                                self.atomic_state_expansions.saturating_add(1);
                            let expand_started = Instant::now();
                            self.expand(stepper, partial);
                            self.atomic_expand_elapsed_ns = self
                                .atomic_expand_elapsed_ns
                                .saturating_add(elapsed_nanos_u64(expand_started));
                        }
                        GeneratorWork::AtomicActions(mut cursor) => {
                            let action = cursor
                                .current_transition()
                                .expect("a scheduled atomic cursor has a candidate");
                            if self.apply_action_transition(
                                stepper,
                                action,
                                transition_reservation,
                                quantum.deadline,
                            ) == ActionTransitionStatus::TimedOut
                            {
                                let priority = cursor
                                    .priority()
                                    .expect("a timed-out cursor retains its candidate");
                                self.push_work(GeneratorWork::AtomicActions(cursor), priority);
                                break Some(GenerationInterruption::Deadline);
                            }
                            cursor.consume_current();
                            if let Some(priority) = cursor.priority() {
                                self.push_work(GeneratorWork::AtomicActions(cursor), priority);
                            }
                        }
                        GeneratorWork::StructuredSelection(mut selection) => {
                            // Like an atomic cursor, one service settles one concrete
                            // edge while retaining the lazy family. Enqueuing members
                            // separately lets the family's aggregate probability defer
                            // every transition until almost all members are allocated.
                            // Each member still receives equal conditional mass.
                            let action = selection
                                .current_transition()
                                .expect("a scheduled selection cursor has a candidate");
                            if self.apply_action_transition(
                                stepper,
                                action,
                                transition_reservation,
                                quantum.deadline,
                            ) == ActionTransitionStatus::TimedOut
                            {
                                let priority = selection
                                    .priority()
                                    .expect("a timed-out selection retains its candidate");
                                self.push_work(
                                    GeneratorWork::StructuredSelection(selection),
                                    priority,
                                );
                                break Some(GenerationInterruption::Deadline);
                            }
                            selection.consume_current();
                            if let Some(priority) = selection.priority() {
                                self.push_work(
                                    GeneratorWork::StructuredSelection(selection),
                                    priority,
                                );
                            }
                        }
                    }
                };
                (interruption, false)
            }
        };

        let status = if plan_prefix_serviced {
            TurnOptionGenerationStatus::PlanPrefixServiced
        } else if let Some(cause) = interruption {
            TurnOptionGenerationStatus::Partial(cause)
        } else if self.gaps.is_empty() {
            TurnOptionGenerationStatus::Complete
        } else {
            TurnOptionGenerationStatus::PartialWithMechanicsGaps
        };
        TurnOptionGenerationReport {
            before,
            after: self.used,
            granted: self.granted,
            before_diagnostics,
            after_diagnostics: self.diagnostics(),
            retained_work_items: self.retained_work_items(),
            newly_completed_options: self
                .total_completed_options
                .saturating_sub(completed_before),
            total_completed_options: self.total_completed_options,
            gaps: self.gaps.clone(),
            status,
        }
    }

    /// Captures the current head of every scheduling view as one finite
    /// service round. Duplicate views of the same shared work item collapse
    /// to one service; newly published work waits for the next round.
    fn snapshot_scheduling_round(&mut self) -> VecDeque<(usize, GeneratorWorkHandle)> {
        let lane_count = self.guided_frontiers.len().saturating_add(1);
        let mut work_handles = HashSet::new();
        let mut round = VecDeque::new();
        for offset in 0..lane_count {
            let lane = (self.next_scheduler_lane + offset) % lane_count;
            let handle = if lane == 0 {
                self.peek_anchor_work_handle()
            } else {
                self.peek_guided_work_handle(lane - 1)
            };
            if let Some(handle) = handle.filter(|handle| work_handles.insert(*handle)) {
                round.push_back((lane, handle));
            }
        }
        if let Some(handle) = self.peek_selection_intent_work_handle() {
            // When both views name the same work, keep the ordinary service
            // identity so withdrawing an intention cannot revoke that head.
            let entry = round
                .iter()
                .position(|(_, ordinary)| *ordinary == handle)
                .and_then(|index| round.remove(index))
                .unwrap_or((SELECTION_INTENT_LANE, handle));
            round.push_front(entry);
        }
        round
    }

    fn expand(&mut self, stepper: &dyn CombatStepper, mut partial: Arc<PartialTurnOption>) {
        let terminal = stepper.terminal(&partial.position);
        if let Some(boundary) = supported_boundary(&self.root, &partial.position, terminal) {
            let partial = Arc::unwrap_or_clone(partial);
            let trace_started = Instant::now();
            let actions = partial.materialize_actions();
            self.transition_trace_elapsed_ns = self
                .transition_trace_elapsed_ns
                .saturating_add(elapsed_nanos_u64(trace_started));
            self.publish_completed(
                CompleteTurnOption::new(
                    self.root.exact_state_identity().clone(),
                    actions,
                    boundary,
                    partial.position,
                    partial.negative_log_policy,
                )
                .with_trajectory_progress(partial.trajectory_progress),
            );
            return;
        }

        if terminal != CombatTerminal::Unresolved
            || !matches!(
                partial.position.engine,
                EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
            )
            || (matches!(partial.position.engine, EngineState::CombatPlayerTurn)
                && partial.position.combat.turn.turn_count != self.root.turn_count())
        {
            self.record_gap(
                TurnOptionGenerationGapKind::UnsupportedStableBoundary,
                &partial,
            );
            return;
        }

        let mut surface =
            crate::automatic_action_surface::automatic_action_surface(stepper, &partial.position);
        if let Some(allowed_slots) = self.config.allowed_potion_slots {
            surface.atomic_actions.retain(|input| {
                let slot = match input {
                    ClientInput::UsePotion { potion_index, .. } => Some(*potion_index),
                    ClientInput::DiscardPotion(slot) => Some(*slot),
                    _ => None,
                };
                slot.is_none_or(|slot| {
                    slot < u64::BITS as usize && allowed_slots & (1_u64 << slot) != 0
                })
            });
        }
        if !self.config.allow_potion_discard {
            surface
                .atomic_actions
                .retain(|input| !matches!(input, ClientInput::DiscardPotion(_)));
        }
        if self
            .max_potion_expenditures
            .is_some_and(|limit| partial.potion_expenditures >= limit)
        {
            surface
                .atomic_actions
                .retain(|input| !is_potion_expenditure(input));
        }
        let surface_is_empty =
            surface.atomic_actions.is_empty() && surface.selection_families.is_empty();
        let policy_choices = surface
            .atomic_actions
            .iter()
            .map(CombatPolicyChoice::Atomic)
            .chain(
                surface
                    .selection_families
                    .iter()
                    .map(CombatPolicyChoice::StructuredSelection),
            )
            .collect::<Vec<_>>();
        let weights = self.policy.weights(&partial.position, &policy_choices);
        let weights = if weights.len() == policy_choices.len() {
            weights
        } else {
            vec![1.0; policy_choices.len()]
        };
        let probabilities = normalized_probabilities(weights, self.config.uniform_exploration_ppm);
        let atomic_action_count = surface.atomic_actions.len();
        let atomic_probabilities = probabilities[..atomic_action_count].to_vec();
        let selection_probabilities = probabilities[atomic_action_count..].to_vec();
        let base_generation_guides = if let Some(guides) = partial.generation_guides.as_ref() {
            guides.clone()
        } else {
            let guides: Arc<[CombatStateGuide]> =
                self.policy.turn_generation_guides(&partial.position).into();
            Arc::make_mut(&mut partial).generation_guides = Some(guides.clone());
            guides
        };
        let parent_guides = base_generation_guides;
        // Every outgoing action observes the same immutable parent position.
        // Sharing it avoids one full combat-state and action-prefix clone for
        // every legal action while preserving the exact search graph.
        let is_intent_source = !self.selection_intent_members.is_empty()
            && self.intent_source().matches_partial(&partial);
        let parent = partial;
        if let Some(cursor) = AtomicActionCursorWork::new(
            parent.clone(),
            surface.atomic_actions,
            atomic_probabilities,
            parent_guides.clone(),
        ) {
            let priority = cursor
                .priority()
                .expect("a new atomic cursor contains probability mass");
            self.push_work(GeneratorWork::AtomicActions(cursor), priority);
        }
        if !surface.selection_families.is_empty()
            && !stepper.supports_canonical_pending_choice_actions()
        {
            self.record_gap(
                TurnOptionGenerationGapKind::UnsupportedStructuredChoice,
                &parent,
            );
        } else {
            for (family, probability) in surface
                .selection_families
                .into_iter()
                .zip(selection_probabilities)
            {
                match SelectionTransactionCursor::new(&family) {
                    Ok(mut cursor) if family.declared_min == 1 && family.effective_max == 1 => {
                        let members =
                            std::iter::from_fn(|| cursor.next_input()).collect::<Vec<_>>();
                        let weights = self.policy.structured_selection_member_weights(
                            &parent.position,
                            &family,
                            &members,
                        );
                        let weights = if weights.len() == members.len() {
                            weights
                        } else {
                            vec![1.0; members.len()]
                        };
                        let probabilities =
                            normalized_probabilities(weights, self.config.uniform_exploration_ppm)
                                .into_iter()
                                .map(|member_probability| probability * member_probability)
                                .collect::<Vec<_>>();
                        if let Some(cursor) = AtomicActionCursorWork::new(
                            parent.clone(),
                            members,
                            probabilities,
                            parent_guides.clone(),
                        ) {
                            let priority = cursor
                                .priority()
                                .expect("ranked selection retains probability mass");
                            self.push_work(GeneratorWork::AtomicActions(cursor), priority);
                        }
                    }
                    Ok(mut cursor) if !cursor.is_exhausted() => {
                        let member_count = cursor.remaining_input_count();
                        let mut remaining_conditional_mass = 1.0;
                        // A sparse preference partitions concrete members out of
                        // the lazy family. Uniform mass still covers every member;
                        // no preferred input is later counted or simulated twice.
                        if self.config.uniform_exploration_ppm < 1_000_000 {
                            let suggestions = self
                                .policy
                                .structured_selection_preferences(&parent.position, &family);
                            let members = suggestions
                                .members()
                                .iter()
                                .filter(|input| {
                                    stepper
                                        .choice_for_legal_input(&parent.position, input)
                                        .is_some()
                                        && cursor.exclude_input(input)
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            if !members.is_empty() {
                                // Even an explicitly zero uniform mix must not
                                // turn a sparse preference into hard pruning.
                                let epsilon =
                                    self.config.uniform_exploration_ppm.max(1) as f64 / 1_000_000.0;
                                let weights = self.policy.structured_selection_member_weights(
                                    &parent.position,
                                    &family,
                                    &members,
                                );
                                let weights = if weights.len() == members.len() {
                                    weights
                                } else {
                                    vec![1.0; members.len()]
                                };
                                let probabilities = normalized_probabilities(weights, 0)
                                    .into_iter()
                                    .map(|weight| {
                                        probability
                                            * ((1.0 - epsilon) * weight
                                                + epsilon / member_count as f64)
                                    })
                                    .collect();
                                remaining_conditional_mass = epsilon
                                    * cursor.remaining_input_count() as f64
                                    / member_count as f64;
                                let preferred = AtomicActionCursorWork::new(
                                    parent.clone(),
                                    members,
                                    probabilities,
                                    parent_guides.clone(),
                                )
                                .expect("validated preferences have positive probability");
                                let priority = preferred.priority().expect("non-empty preferences");
                                self.push_work(GeneratorWork::AtomicActions(preferred), priority);
                            }
                        }
                        if cursor.is_exhausted() {
                            continue;
                        }
                        let family_negative_log_policy =
                            parent.negative_log_policy - probability.ln();
                        let selection = StructuredSelectionWork {
                            parent: parent.clone(),
                            cursor,
                            family_negative_log_policy,
                            remaining_conditional_mass,
                        };
                        let priority = selection.priority().expect("non-empty selection remainder");
                        self.push_work(GeneratorWork::StructuredSelection(selection), priority);
                    }
                    Ok(_) => {}
                    Err(kind) => self.record_gap(kind, &parent),
                }
            }
        }
        if surface_is_empty {
            self.record_gap(
                TurnOptionGenerationGapKind::EmptyLegalActionSurface,
                &parent,
            );
        }
        if is_intent_source && !self.selection_intent_members.is_empty() {
            self.refresh_selection_intent();
        }
    }

    fn record_gap(&mut self, kind: TurnOptionGenerationGapKind, partial: &PartialTurnOption) {
        self.gaps.push(TurnOptionGenerationGap {
            kind,
            exact_state_hash: exact_hash(&partial.position),
            action_depth: partial.action_depth(),
        });
    }

    fn publish_completed(&mut self, option: CompleteTurnOption) {
        self.total_completed_options = self.total_completed_options.saturating_add(1);
        self.completed.push(option);
    }
}

fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn elapsed_nanos_u64(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod priority_tests;
