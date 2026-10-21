use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use sts_core::ai::combat_state_key::{combat_exact_state_key, CombatExactStateKey};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal};
use sts_core::state::core::{ClientInput, EngineState};

use crate::atomic_witness::{replay_atomic_actions, ExactAtomicWitness};
use crate::policy::{
    normalized_probabilities, uniform_policy, CombatPolicyChoice, SharedCombatActionPolicy,
};
use crate::selection_transaction::SelectionTransactionCursor;
use crate::types::{exact_hash, CombatDecisionRoot, TurnOptionAction};

mod contract;
mod turn_macro;

pub use contract::*;

#[derive(Clone)]
struct TraceNode {
    parent: Option<Arc<TraceNode>>,
    action: Option<TurnOptionAction>,
}

impl TraceNode {
    fn root() -> Arc<Self> {
        Arc::new(Self {
            parent: None,
            action: None,
        })
    }

    fn extend(parent: Arc<Self>, action: TurnOptionAction) -> Arc<Self> {
        Arc::new(Self {
            parent: Some(parent),
            action: Some(action),
        })
    }

    fn actions(&self) -> Vec<TurnOptionAction> {
        let mut actions = Vec::new();
        let mut cursor = Some(self);
        while let Some(node) = cursor {
            if let Some(action) = &node.action {
                actions.push(action.clone());
            }
            cursor = node.parent.as_deref();
        }
        actions.reverse();
        actions
    }
}

#[derive(Clone)]
struct DiveSeed {
    position: Arc<CombatPosition>,
    trace: Arc<TraceNode>,
    discrepancy: f64,
    greedy_actions_since_deviation: usize,
    at_player_turn_boundary: bool,
}

struct ApplyDeviation {
    parent: Arc<CombatPosition>,
    trace: Arc<TraceNode>,
    input: ClientInput,
    discrepancy: f64,
    greedy_actions_since_deviation_after_apply: usize,
}

struct StructuredDeviation {
    parent: Arc<CombatPosition>,
    trace: Arc<TraceNode>,
    cursor: SelectionTransactionCursor,
    discrepancy: f64,
}

struct TurnMacroProposal {
    seed: DiveSeed,
}

enum DiscrepancyWork {
    Dive(DiveSeed),
    Apply(ApplyDeviation),
    Structured(StructuredDeviation),
    TurnMacro(TurnMacroProposal),
}

impl DiscrepancyWork {
    fn materialization_rank(&self) -> u8 {
        match self {
            Self::Dive(_) => 0,
            Self::Apply(_) => 1,
            Self::Structured(_) => 2,
            Self::TurnMacro(_) => 3,
        }
    }
}

struct QueueEntry {
    priority: f64,
    discrepancy: f64,
    sequence_id: u64,
    work: DiscrepancyWork,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority.to_bits() == other.priority.to_bits()
            && self.discrepancy.to_bits() == other.discrepancy.to_bits()
            && self.sequence_id == other.sequence_id
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then_with(|| {
                other
                    .work
                    .materialization_rank()
                    .cmp(&self.work.materialization_rank())
            })
            .then_with(|| other.sequence_id.cmp(&self.sequence_id))
    }
}

struct ConcreteCandidate {
    input: ClientInput,
    probability: f64,
}

struct LazyFamily {
    cursor: SelectionTransactionCursor,
    member_probability: f64,
}

pub struct PolicyDiscrepancySession {
    root: CombatPosition,
    config: PolicyDiscrepancyConfig,
    policy: SharedCombatActionPolicy,
    frontier: BinaryHeap<QueueEntry>,
    next_sequence_id: u64,
    best_state_discrepancy: HashMap<CombatExactStateKey, f64>,
    state_policy_dive_services: HashMap<CombatExactStateKey, usize>,
    turn_macro_selected_states: HashSet<CombatExactStateKey>,
    turn_macro_scheduled_states: HashSet<CombatExactStateKey>,
    used: PolicyDiscrepancyCounters,
    granted_applied_transitions: usize,
    granted_engine_steps: usize,
    witness: Option<ExactAtomicWitness>,
    replay_mismatch: bool,
}

impl PolicyDiscrepancySession {
    pub fn new(root: CombatDecisionRoot, config: PolicyDiscrepancyConfig) -> Self {
        Self::with_policy(root, config, uniform_policy())
    }

    pub fn with_policy(
        root: CombatDecisionRoot,
        config: PolicyDiscrepancyConfig,
        policy: SharedCombatActionPolicy,
    ) -> Self {
        let position = root.position().clone();
        let exact_key = combat_exact_state_key(&position.engine, &position.combat);
        let mut session = Self {
            root: position.clone(),
            config,
            policy,
            frontier: BinaryHeap::new(),
            next_sequence_id: 0,
            best_state_discrepancy: HashMap::from([(exact_key, 0.0)]),
            state_policy_dive_services: HashMap::new(),
            turn_macro_selected_states: HashSet::new(),
            turn_macro_scheduled_states: HashSet::new(),
            used: PolicyDiscrepancyCounters {
                exact_states: 1,
                ..PolicyDiscrepancyCounters::default()
            },
            granted_applied_transitions: 0,
            granted_engine_steps: 0,
            witness: None,
            replay_mismatch: false,
        };
        session.push_work(
            0.0,
            DiscrepancyWork::Dive(DiveSeed {
                position: Arc::new(position),
                trace: TraceNode::root(),
                discrepancy: 0.0,
                greedy_actions_since_deviation: 0,
                at_player_turn_boundary: true,
            }),
        );
        session
    }

    pub fn advance(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: PolicyDiscrepancyQuantum,
    ) -> PolicyDiscrepancyReport {
        let before = self.used;
        self.granted_applied_transitions = self
            .granted_applied_transitions
            .saturating_add(quantum.additional_applied_transitions);
        self.granted_engine_steps = self
            .granted_engine_steps
            .saturating_add(quantum.additional_engine_steps);

        let status = loop {
            if self.witness.is_some() {
                break PolicyDiscrepancyStatus::WitnessFound;
            }
            if self.replay_mismatch {
                break PolicyDiscrepancyStatus::ReplayMismatch;
            }
            if deadline_reached(quantum.deadline) {
                break PolicyDiscrepancyStatus::Partial(PolicyDiscrepancyInterruption::Deadline);
            }
            let Some(entry) = self.frontier.pop() else {
                break PolicyDiscrepancyStatus::FrontierExhausted;
            };
            let interrupted = match entry.work {
                DiscrepancyWork::Dive(seed) => {
                    self.run_policy_dive(stepper, seed, quantum.deadline)
                }
                DiscrepancyWork::Apply(work) => {
                    self.apply_deviation(stepper, work, quantum.deadline)
                }
                DiscrepancyWork::Structured(mut work) => {
                    let Some(input) = work.cursor.next_input() else {
                        continue;
                    };
                    self.used.structured_inputs_materialized =
                        self.used.structured_inputs_materialized.saturating_add(1);
                    let apply = ApplyDeviation {
                        parent: work.parent.clone(),
                        trace: work.trace.clone(),
                        input,
                        discrepancy: work.discrepancy,
                        greedy_actions_since_deviation_after_apply: 0,
                    };
                    if !work.cursor.is_exhausted() {
                        self.push_work(work.discrepancy, DiscrepancyWork::Structured(work));
                    }
                    self.apply_deviation(stepper, apply, quantum.deadline)
                }
                DiscrepancyWork::TurnMacro(work) => {
                    self.run_turn_macro(stepper, work, quantum.deadline)
                }
            };
            if let Some(interruption) = interrupted {
                break PolicyDiscrepancyStatus::Partial(interruption);
            }
        };

        PolicyDiscrepancyReport {
            before,
            after: self.used,
            frontier_entries: self.frontier.len(),
            best_queued_priority: self.frontier.peek().map(|entry| entry.priority),
            best_queued_discrepancy: self
                .frontier
                .iter()
                .map(|entry| entry.discrepancy)
                .min_by(f64::total_cmp),
            status,
            witness: self.witness.clone(),
        }
    }

    pub fn counters(&self) -> PolicyDiscrepancyCounters {
        self.used
    }

    pub fn frontier_entries(&self) -> usize {
        self.frontier.len()
    }

    pub fn retained_state_work(&self) -> usize {
        self.frontier
            .len()
            .saturating_add(self.best_state_discrepancy.len())
    }

    pub fn state_diagnostic(&self, position: &CombatPosition) -> PolicyDiscrepancyStateDiagnostic {
        let key = combat_exact_state_key(&position.engine, &position.combat);
        PolicyDiscrepancyStateDiagnostic {
            exact_state_hash: exact_hash(position),
            discovered: self.best_state_discrepancy.contains_key(&key),
            best_discrepancy: self.best_state_discrepancy.get(&key).copied(),
            policy_dive_services: self
                .state_policy_dive_services
                .get(&key)
                .copied()
                .unwrap_or_default(),
            selected_by_turn_macro: self.turn_macro_selected_states.contains(&key),
            turn_macro_scheduled: self.turn_macro_scheduled_states.contains(&key),
        }
    }

    /// Replays an exact trajectory through the same hierarchical
    /// atomic/structured action surface used by discrepancy search. This is
    /// diagnostic only: it does not enqueue work or assert optimality.
    pub fn audit_trajectory(
        &mut self,
        stepper: &dyn CombatStepper,
        inputs: &[ClientInput],
    ) -> Result<PolicyDiscrepancyTrajectoryAudit, String> {
        let mut position = self.root.clone();
        let mut total_weighted_discrepancy = 0.0;
        let mut deviations = Vec::new();
        for (action_index, demonstrated_input) in inputs.iter().enumerate() {
            if !stepper.is_legal_action(&position, demonstrated_input) {
                return Err(format!(
                    "policy-discrepancy audit action {action_index} is not legal"
                ));
            }
            let (mut candidates, mut lazy_families) = self.policy_candidates(stepper, &position);
            if candidates.is_empty() {
                return Err(format!(
                    "policy-discrepancy audit action {action_index} has no concrete policy candidate"
                ));
            }
            candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
            let greedy = &candidates[0];
            let mut demonstrated_probability = candidates
                .iter()
                .find(|candidate| candidate.input == *demonstrated_input)
                .map(|candidate| (candidate.probability, false));
            if demonstrated_probability.is_none() {
                'families: for family in &mut lazy_families {
                    while let Some(input) = family.cursor.next_input() {
                        if input == *demonstrated_input {
                            demonstrated_probability = Some((family.member_probability, true));
                            break 'families;
                        }
                    }
                }
            }
            let Some((demonstrated_probability, demonstrated_was_lazy)) = demonstrated_probability
            else {
                return Err(format!(
                    "policy-discrepancy audit action {action_index} is absent from the runtime policy surface"
                ));
            };
            if greedy.input != *demonstrated_input {
                let discrepancy_increment = (greedy.probability.max(f64::MIN_POSITIVE)
                    / demonstrated_probability.max(f64::MIN_POSITIVE))
                .ln();
                total_weighted_discrepancy += discrepancy_increment;
                deviations.push(PolicyDiscrepancyTrajectoryDeviation {
                    action_index,
                    player_turn: position.combat.turn.turn_count,
                    demonstrated_input: demonstrated_input.clone(),
                    greedy_input: greedy.input.clone(),
                    demonstrated_probability,
                    greedy_probability: greedy.probability,
                    discrepancy_increment,
                    cumulative_discrepancy: total_weighted_discrepancy,
                    demonstrated_was_lazy,
                });
            }
            let result = stepper.apply_to_stable(
                &position,
                demonstrated_input.clone(),
                CombatStepLimits {
                    max_engine_steps: self.config.max_engine_steps_per_transition.max(1),
                    deadline: None,
                },
            );
            if result.truncated || result.timed_out {
                return Err(format!(
                    "policy-discrepancy audit action {action_index} did not reach a stable successor"
                ));
            }
            position = result.position;
        }
        Ok(PolicyDiscrepancyTrajectoryAudit {
            source_action_count: inputs.len(),
            non_greedy_action_count: deviations.len(),
            total_weighted_discrepancy,
            terminal: stepper.terminal(&position),
            deviations,
        })
    }

    fn run_policy_dive(
        &mut self,
        stepper: &dyn CombatStepper,
        mut seed: DiveSeed,
        deadline: Option<Instant>,
    ) -> Option<PolicyDiscrepancyInterruption> {
        self.used.policy_dives = self.used.policy_dives.saturating_add(1);
        let seed_key = combat_exact_state_key(&seed.position.engine, &seed.position.combat);
        self.state_policy_dive_services
            .entry(seed_key)
            .and_modify(|services| *services = services.saturating_add(1))
            .or_insert(1);
        for _ in 0..self.config.max_greedy_actions_per_dive.max(1) {
            match stepper.terminal(&seed.position) {
                CombatTerminal::Win if stepper.is_exact_victory(&seed.position) => {
                    self.finish_witness(stepper, &seed);
                    return None;
                }
                CombatTerminal::Win => return None,
                CombatTerminal::Loss => return None,
                CombatTerminal::Unresolved => {}
            }
            self.enqueue_turn_macro_if_needed(&seed);
            let (mut candidates, lazy_families) = self.policy_candidates(stepper, &seed.position);
            if candidates.is_empty() {
                self.used.unsupported_stable_boundaries =
                    self.used.unsupported_stable_boundaries.saturating_add(1);
                return None;
            }
            candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
            let greedy = candidates.remove(0);
            let best_probability = greedy.probability.max(f64::MIN_POSITIVE);
            for candidate in candidates {
                let discrepancy = seed.discrepancy
                    + (best_probability / candidate.probability.max(f64::MIN_POSITIVE)).ln();
                self.push_work(
                    discrepancy,
                    DiscrepancyWork::Apply(ApplyDeviation {
                        parent: seed.position.clone(),
                        trace: seed.trace.clone(),
                        input: candidate.input,
                        discrepancy,
                        greedy_actions_since_deviation_after_apply: 0,
                    }),
                );
                self.used.queued_discrepancies = self.used.queued_discrepancies.saturating_add(1);
            }
            for family in lazy_families {
                let discrepancy = seed.discrepancy
                    + (best_probability / family.member_probability.max(f64::MIN_POSITIVE)).ln();
                self.push_work(
                    discrepancy,
                    DiscrepancyWork::Structured(StructuredDeviation {
                        parent: seed.position.clone(),
                        trace: seed.trace.clone(),
                        cursor: family.cursor,
                        discrepancy,
                    }),
                );
                self.used.queued_discrepancies = self.used.queued_discrepancies.saturating_add(1);
            }
            let greedy_actions_since_deviation =
                seed.greedy_actions_since_deviation.saturating_add(1);
            let next = match self.apply_input(
                stepper,
                seed.position,
                seed.trace,
                greedy.input,
                seed.discrepancy,
                greedy_actions_since_deviation,
                deadline,
            ) {
                Ok(Some(next)) => next,
                Ok(None) => return None,
                Err(interruption) => return Some(interruption),
            };
            seed = next;
        }
        self.used.greedy_depth_limit_hits = self.used.greedy_depth_limit_hits.saturating_add(1);
        let continuation_priority =
            seed.discrepancy + (seed.greedy_actions_since_deviation.saturating_add(1) as f64).ln();
        self.push_work_with_priority(
            continuation_priority,
            seed.discrepancy,
            DiscrepancyWork::Dive(seed),
        );
        None
    }

    fn apply_deviation(
        &mut self,
        stepper: &dyn CombatStepper,
        work: ApplyDeviation,
        deadline: Option<Instant>,
    ) -> Option<PolicyDiscrepancyInterruption> {
        let seed = match self.apply_input(
            stepper,
            work.parent,
            work.trace,
            work.input,
            work.discrepancy,
            work.greedy_actions_since_deviation_after_apply,
            deadline,
        ) {
            Ok(Some(seed)) => seed,
            Ok(None) => return None,
            Err(interruption) => return Some(interruption),
        };
        self.run_policy_dive(stepper, seed, deadline)
    }

    fn apply_input(
        &mut self,
        stepper: &dyn CombatStepper,
        parent: Arc<CombatPosition>,
        trace: Arc<TraceNode>,
        input: ClientInput,
        discrepancy: f64,
        greedy_actions_since_deviation_after_apply: usize,
        deadline: Option<Instant>,
    ) -> Result<Option<DiveSeed>, PolicyDiscrepancyInterruption> {
        if deadline_reached(deadline) {
            self.requeue_apply(ApplyDeviation {
                parent,
                trace,
                input,
                discrepancy,
                greedy_actions_since_deviation_after_apply,
            });
            return Err(PolicyDiscrepancyInterruption::Deadline);
        }
        if self.used.applied_action_transitions >= self.granted_applied_transitions {
            self.requeue_apply(ApplyDeviation {
                parent,
                trace,
                input,
                discrepancy,
                greedy_actions_since_deviation_after_apply,
            });
            return Err(PolicyDiscrepancyInterruption::AppliedTransitionBudget);
        }
        let reservation = self.config.max_engine_steps_per_transition.max(1);
        if self
            .granted_engine_steps
            .saturating_sub(self.used.engine_steps)
            < reservation
        {
            self.requeue_apply(ApplyDeviation {
                parent,
                trace,
                input,
                discrepancy,
                greedy_actions_since_deviation_after_apply,
            });
            return Err(PolicyDiscrepancyInterruption::EngineStepBudget);
        }
        if !stepper.is_legal_action(&parent, &input) {
            self.used.unsupported_stable_boundaries =
                self.used.unsupported_stable_boundaries.saturating_add(1);
            return Ok(None);
        }
        let parent_turn = parent.combat.turn.turn_count;
        let result = stepper.apply_to_stable(
            &parent,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: reservation,
                deadline,
            },
        );
        self.used.engine_steps = self.used.engine_steps.saturating_add(result.engine_steps);
        if result.timed_out {
            self.requeue_apply(ApplyDeviation {
                parent,
                trace,
                input,
                discrepancy,
                greedy_actions_since_deviation_after_apply,
            });
            return Err(PolicyDiscrepancyInterruption::Deadline);
        }
        self.used.applied_action_transitions =
            self.used.applied_action_transitions.saturating_add(1);
        if result.truncated {
            self.used.transition_step_limit_gaps =
                self.used.transition_step_limit_gaps.saturating_add(1);
            return Ok(None);
        }
        let successor_hash = exact_hash(&result.position);
        let action = TurnOptionAction {
            input,
            expected_successor_hash: successor_hash.into(),
            engine_steps: result.engine_steps,
        };
        let trace = TraceNode::extend(trace, action);
        let key = combat_exact_state_key(&result.position.engine, &result.position.combat);
        match self.best_state_discrepancy.get(&key).copied() {
            Some(previous) if previous <= discrepancy => {
                self.used.duplicate_or_dominated_states =
                    self.used.duplicate_or_dominated_states.saturating_add(1);
                Ok(None)
            }
            previous => {
                self.best_state_discrepancy.insert(key, discrepancy);
                if previous.is_none() {
                    self.used.exact_states = self.used.exact_states.saturating_add(1);
                }
                let at_player_turn_boundary =
                    matches!(result.position.engine, EngineState::CombatPlayerTurn)
                        && result.position.combat.turn.turn_count > parent_turn;
                Ok(Some(DiveSeed {
                    position: Arc::new(result.position),
                    trace,
                    discrepancy,
                    greedy_actions_since_deviation: greedy_actions_since_deviation_after_apply,
                    at_player_turn_boundary,
                }))
            }
        }
    }

    fn policy_candidates(
        &mut self,
        stepper: &dyn CombatStepper,
        position: &CombatPosition,
    ) -> (Vec<ConcreteCandidate>, Vec<LazyFamily>) {
        let mut surface =
            crate::automatic_action_surface::automatic_action_surface(stepper, position);
        if !self.config.allow_potion_discard {
            surface
                .atomic_actions
                .retain(|input| !matches!(input, ClientInput::DiscardPotion(_)));
        }
        if self.config.max_potions_used == Some(0) || self.config.allowed_potion_slots == Some(0) {
            surface.atomic_actions.retain(|input| {
                !matches!(
                    input,
                    ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                )
            });
        } else if let Some(allowed_slots) = self.config.allowed_potion_slots {
            surface.atomic_actions.retain(|input| {
                crate::witness::potion_input_uses_allowed_slot(input, Some(allowed_slots))
            });
        }
        let choices = surface
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
        if choices.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let weights = self.policy.weights(position, &choices);
        let weights = if weights.len() == choices.len() {
            weights
        } else {
            vec![1.0; choices.len()]
        };
        let probabilities = normalized_probabilities(weights, self.config.uniform_exploration_ppm);
        let atomic_count = surface.atomic_actions.len();
        let mut concrete = surface
            .atomic_actions
            .into_iter()
            .zip(probabilities[..atomic_count].iter().copied())
            .map(|(input, probability)| ConcreteCandidate { input, probability })
            .collect::<Vec<_>>();
        let mut lazy = Vec::new();
        if !surface.selection_families.is_empty()
            && !stepper.supports_canonical_pending_choice_actions()
        {
            self.used.unsupported_stable_boundaries =
                self.used.unsupported_stable_boundaries.saturating_add(1);
            return (concrete, lazy);
        }
        for (family, family_probability) in surface
            .selection_families
            .into_iter()
            .zip(probabilities[atomic_count..].iter().copied())
        {
            let Ok(mut cursor) = SelectionTransactionCursor::new(&family) else {
                self.used.unsupported_stable_boundaries =
                    self.used.unsupported_stable_boundaries.saturating_add(1);
                continue;
            };
            let member_count = cursor.remaining_input_count();
            if member_count == 0 {
                continue;
            }
            if family.declared_min == 1 && family.effective_max == 1 {
                let members = std::iter::from_fn(|| cursor.next_input()).collect::<Vec<_>>();
                let member_weights = self
                    .policy
                    .structured_selection_member_weights(position, &family, &members);
                let member_weights = if member_weights.len() == members.len() {
                    member_weights
                } else {
                    vec![1.0; members.len()]
                };
                let member_probabilities =
                    normalized_probabilities(member_weights, self.config.uniform_exploration_ppm);
                concrete.extend(members.into_iter().zip(member_probabilities).map(
                    |(input, member_probability)| ConcreteCandidate {
                        input,
                        probability: family_probability * member_probability,
                    },
                ));
                self.used.structured_inputs_materialized = self
                    .used
                    .structured_inputs_materialized
                    .saturating_add(member_count);
            } else if let Some(input) = cursor.next_input() {
                let member_probability = family_probability / member_count as f64;
                concrete.push(ConcreteCandidate {
                    input,
                    probability: member_probability,
                });
                self.used.structured_inputs_materialized =
                    self.used.structured_inputs_materialized.saturating_add(1);
                if !cursor.is_exhausted() {
                    lazy.push(LazyFamily {
                        cursor,
                        member_probability,
                    });
                }
            }
        }
        (concrete, lazy)
    }

    fn finish_witness(&mut self, stepper: &dyn CombatStepper, seed: &DiveSeed) {
        let actions = seed.trace.actions();
        let potion_expenditures = actions
            .iter()
            .filter(|action| {
                matches!(
                    action.input,
                    ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                )
            })
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        if self
            .config
            .max_potions_used
            .is_some_and(|limit| potion_expenditures > limit)
        {
            return;
        }
        if !crate::witness::actions_use_only_allowed_potion_slots(
            &actions,
            self.config.allowed_potion_slots,
        ) {
            return;
        }
        match replay_atomic_actions(
            stepper,
            &self.root,
            &actions,
            self.config.max_engine_steps_per_transition,
        ) {
            Ok((final_position, replay_engine_steps))
                if stepper.is_exact_victory(&final_position) =>
            {
                if crate::witness::trajectory_within_potion_contract(
                    &self.root,
                    &actions,
                    &final_position,
                    self.config.max_potions_used,
                    self.config.allowed_potion_slots,
                ) {
                    self.witness = Some(ExactAtomicWitness {
                        actions,
                        final_position,
                        negative_log_policy: seed.discrepancy,
                        replay_engine_steps,
                    });
                }
            }
            _ => self.replay_mismatch = true,
        }
    }

    fn push_work(&mut self, discrepancy: f64, work: DiscrepancyWork) {
        self.push_work_with_priority(discrepancy, discrepancy, work);
    }

    fn requeue_apply(&mut self, work: ApplyDeviation) {
        let priority = work.discrepancy
            + (work
                .greedy_actions_since_deviation_after_apply
                .saturating_add(1) as f64)
                .ln();
        self.push_work_with_priority(priority, work.discrepancy, DiscrepancyWork::Apply(work));
    }

    fn push_work_with_priority(&mut self, priority: f64, discrepancy: f64, work: DiscrepancyWork) {
        let sequence_id = self.next_sequence_id;
        self.next_sequence_id = self.next_sequence_id.saturating_add(1);
        self.frontier.push(QueueEntry {
            priority,
            discrepancy,
            sequence_id,
            work,
        });
    }
}

fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}
