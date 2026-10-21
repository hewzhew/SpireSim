use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal};

use super::policy::{
    normalized_probabilities, CombatGuideLaneId, CombatPolicyChoice, CombatStateGuideRank,
    SharedCombatActionPolicy,
};
use super::selection_transaction::SelectionTransactionCursor;
use super::types::{
    exact_hash, supported_boundary, CombatDecisionRoot, CompleteTurnOption, TurnOptionAction,
    TurnOptionGenerationGap, TurnOptionGeneratorConfig,
};

/// A bounded control for complete-turn generation that keeps unfinished
/// action prefixes separate from already-finished turn options.  Every
/// retained partial state exposes its whole finite atomic surface before the
/// next action-depth beam is selected, so early EndTurn options cannot close
/// the proposal surface merely by arriving first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepthBeamTurnConfig {
    pub generator: TurnOptionGeneratorConfig,
    /// Remaining finite potion expenditure allowance for this complete turn.
    /// `None` keeps the generator's ordinary unbounded legal surface.
    pub max_potion_expenditures: Option<u32>,
    pub partial_beam_width: usize,
    pub retained_per_view: usize,
    pub max_atomic_depth: usize,
    pub max_structured_members_per_family: usize,
}

impl Default for DepthBeamTurnConfig {
    fn default() -> Self {
        Self {
            generator: TurnOptionGeneratorConfig::default(),
            max_potion_expenditures: None,
            partial_beam_width: 32,
            retained_per_view: 6,
            max_atomic_depth: 32,
            max_structured_members_per_family: 256,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DepthBeamTurnBudget {
    pub max_applied_transitions: usize,
    pub max_engine_steps: usize,
    pub deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepthBeamTurnInterruption {
    TransitionBudget,
    EngineStepBudget,
    Deadline,
    AtomicDepthLimit,
    BeamPruned,
    StructuredFamilyLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DepthBeamTurnStatus {
    Complete,
    Partial(DepthBeamTurnInterruption),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DepthBeamTurnCounters {
    pub expanded_partial_states: usize,
    pub applied_transitions: usize,
    pub engine_steps: usize,
    pub unique_partial_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    pub retained_partial_states: usize,
    pub pruned_partial_states: usize,
    pub maximum_atomic_depth: usize,
    pub truncated_structured_families: usize,
}

#[derive(Clone, Debug)]
pub struct DepthBeamTurnReport {
    pub status: DepthBeamTurnStatus,
    pub counters: DepthBeamTurnCounters,
    pub layers: Vec<DepthBeamTurnLayerReport>,
    pub options: Vec<CompleteTurnOption>,
    pub gaps: Vec<TurnOptionGenerationGap>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepthBeamTurnLayerReport {
    pub atomic_depth: usize,
    pub expanded_partial_states: usize,
    pub generated_unique_partial_states: usize,
    pub retained_partial_states: usize,
    pub retained_exact_state_hashes: Vec<String>,
    pub new_completed_turn_options: usize,
}

#[derive(Clone, Debug)]
struct PartialTurn {
    position: CombatPosition,
    actions: Vec<TurnOptionAction>,
    negative_log_policy: f64,
    potion_expenditures: u32,
}

impl PartialTurn {
    fn atomic_depth(&self) -> usize {
        self.actions.len()
    }

    fn anchor_cost(&self) -> f64 {
        self.negative_log_policy + (self.atomic_depth().max(1) as f64).ln()
    }
}

#[derive(Clone, Debug)]
struct ConcreteAction {
    input: sts_core::state::core::ClientInput,
    probability: f64,
}

pub fn generate_depth_beam_turn_options(
    root: CombatDecisionRoot,
    config: DepthBeamTurnConfig,
    budget: DepthBeamTurnBudget,
    policy: SharedCombatActionPolicy,
    stepper: &dyn CombatStepper,
) -> DepthBeamTurnReport {
    let mut counters = DepthBeamTurnCounters::default();
    let mut gaps = Vec::new();
    let mut frontier = vec![PartialTurn {
        position: root.position().clone(),
        actions: Vec::new(),
        negative_log_policy: 0.0,
        potion_expenditures: 0,
    }];
    let mut seen_partial = HashSet::from([exact_hash(root.position())]);
    let mut completed = HashMap::<String, CompleteTurnOption>::new();
    let mut layers = Vec::new();
    let mut interruption = None;

    'depths: while !frontier.is_empty() {
        let next_depth = frontier[0].atomic_depth().saturating_add(1);
        if next_depth > config.max_atomic_depth.max(1) {
            interruption = Some(DepthBeamTurnInterruption::AtomicDepthLimit);
            break;
        }
        counters.maximum_atomic_depth = counters.maximum_atomic_depth.max(next_depth);
        let expanded_before = counters.expanded_partial_states;
        let unique_before = counters.unique_partial_states;
        let completed_before = completed.len();
        let mut next = HashMap::<String, PartialTurn>::new();

        for parent in std::mem::take(&mut frontier) {
            if deadline_reached(budget.deadline) {
                interruption = Some(DepthBeamTurnInterruption::Deadline);
                break 'depths;
            }
            counters.expanded_partial_states = counters.expanded_partial_states.saturating_add(1);
            let actions = concrete_actions(
                &parent.position,
                config,
                parent.potion_expenditures,
                policy.as_ref(),
                stepper,
                &mut counters,
                &mut gaps,
            );
            for action in actions {
                if deadline_reached(budget.deadline) {
                    interruption = Some(DepthBeamTurnInterruption::Deadline);
                    break 'depths;
                }
                if counters.applied_transitions >= budget.max_applied_transitions {
                    interruption = Some(DepthBeamTurnInterruption::TransitionBudget);
                    break 'depths;
                }
                let reservation = config.generator.max_engine_steps_per_transition.max(1);
                if budget
                    .max_engine_steps
                    .saturating_sub(counters.engine_steps)
                    < reservation
                {
                    interruption = Some(DepthBeamTurnInterruption::EngineStepBudget);
                    break 'depths;
                }
                if stepper
                    .choice_for_legal_input(&parent.position, &action.input)
                    .is_none()
                {
                    gaps.push(TurnOptionGenerationGap {
                        kind: super::types::TurnOptionGenerationGapKind::GeneratedInputRejected,
                        exact_state_hash: exact_hash(&parent.position),
                        action_depth: parent.atomic_depth(),
                    });
                    continue;
                }
                let result = stepper.apply_to_stable(
                    &parent.position,
                    action.input.clone(),
                    CombatStepLimits {
                        max_engine_steps: reservation,
                        deadline: budget.deadline,
                    },
                );
                counters.applied_transitions = counters.applied_transitions.saturating_add(1);
                counters.engine_steps = counters.engine_steps.saturating_add(result.engine_steps);
                if result.timed_out {
                    interruption = Some(DepthBeamTurnInterruption::Deadline);
                    break 'depths;
                }
                if result.truncated {
                    gaps.push(TurnOptionGenerationGap {
                        kind: super::types::TurnOptionGenerationGapKind::TransitionStepLimit,
                        exact_state_hash: exact_hash(&parent.position),
                        action_depth: parent.atomic_depth(),
                    });
                    continue;
                }

                let is_potion_expenditure = matches!(
                    action.input,
                    sts_core::state::core::ClientInput::UsePotion { .. }
                        | sts_core::state::core::ClientInput::DiscardPotion(_)
                );
                let mut trace = parent.actions.clone();
                trace.push(TurnOptionAction {
                    input: action.input,
                    expected_successor_hash: exact_hash(&result.position).into(),
                    engine_steps: result.engine_steps,
                });
                let negative_log_policy =
                    parent.negative_log_policy - action.probability.max(f64::MIN_POSITIVE).ln();
                let terminal = stepper.terminal(&result.position);
                if let Some(boundary) = supported_boundary(&root, &result.position, terminal) {
                    let option = CompleteTurnOption::new(
                        root.exact_state_hash().to_owned(),
                        trace,
                        boundary,
                        result.position,
                        negative_log_policy,
                    );
                    let hash = option.exact_successor_hash().to_owned();
                    match completed.entry(hash) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(option);
                        }
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            counters.duplicate_exact_successors =
                                counters.duplicate_exact_successors.saturating_add(1);
                            if complete_path_is_better(&option, entry.get()) {
                                entry.insert(option);
                            }
                        }
                    }
                    continue;
                }
                if terminal != CombatTerminal::Unresolved {
                    continue;
                }
                let hash = exact_hash(&result.position);
                let candidate = PartialTurn {
                    position: result.position,
                    actions: trace,
                    negative_log_policy,
                    potion_expenditures: parent
                        .potion_expenditures
                        .saturating_add(u32::from(is_potion_expenditure)),
                };
                match next.entry(hash.clone()) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        if seen_partial.insert(hash) {
                            counters.unique_partial_states =
                                counters.unique_partial_states.saturating_add(1);
                            entry.insert(candidate);
                        } else {
                            counters.duplicate_exact_successors =
                                counters.duplicate_exact_successors.saturating_add(1);
                        }
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        counters.duplicate_exact_successors =
                            counters.duplicate_exact_successors.saturating_add(1);
                        if partial_path_is_better(&candidate, entry.get()) {
                            entry.insert(candidate);
                        }
                    }
                }
            }
        }

        let next = next.into_values().collect::<Vec<_>>();
        let generated_partial_states = next.len();
        frontier = retain_partial_frontier(
            next,
            config.partial_beam_width.max(1),
            config.retained_per_view.max(1),
            policy.as_ref(),
        );
        counters.pruned_partial_states = counters
            .pruned_partial_states
            .saturating_add(generated_partial_states.saturating_sub(frontier.len()));
        counters.retained_partial_states = counters
            .retained_partial_states
            .saturating_add(frontier.len());
        layers.push(DepthBeamTurnLayerReport {
            atomic_depth: next_depth,
            expanded_partial_states: counters
                .expanded_partial_states
                .saturating_sub(expanded_before),
            generated_unique_partial_states: counters
                .unique_partial_states
                .saturating_sub(unique_before),
            retained_partial_states: frontier.len(),
            retained_exact_state_hashes: frontier
                .iter()
                .map(|partial| exact_hash(&partial.position))
                .collect(),
            new_completed_turn_options: completed.len().saturating_sub(completed_before),
        });
    }

    let mut options = completed.into_values().collect::<Vec<_>>();
    options.sort_by(|left, right| {
        left.negative_log_policy()
            .total_cmp(&right.negative_log_policy())
            .then_with(|| left.actions().len().cmp(&right.actions().len()))
            .then_with(|| {
                left.exact_successor_hash()
                    .cmp(right.exact_successor_hash())
            })
    });
    counters.completed_turn_options = options.len();
    DepthBeamTurnReport {
        status: interruption
            .or_else(|| {
                (counters.truncated_structured_families > 0)
                    .then_some(DepthBeamTurnInterruption::StructuredFamilyLimit)
            })
            .or_else(|| {
                (counters.pruned_partial_states > 0)
                    .then_some(DepthBeamTurnInterruption::BeamPruned)
            })
            .map(DepthBeamTurnStatus::Partial)
            .unwrap_or(DepthBeamTurnStatus::Complete),
        counters,
        layers,
        options,
        gaps,
    }
}

fn concrete_actions(
    position: &CombatPosition,
    config: DepthBeamTurnConfig,
    potion_expenditures: u32,
    policy: &dyn super::policy::CombatActionPolicy,
    stepper: &dyn CombatStepper,
    counters: &mut DepthBeamTurnCounters,
    gaps: &mut Vec<TurnOptionGenerationGap>,
) -> Vec<ConcreteAction> {
    let surface = crate::automatic_action_surface::automatic_action_surface(stepper, position);
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
    let weights = policy.weights(position, &choices);
    let weights = if weights.len() == choices.len() {
        weights
    } else {
        vec![1.0; choices.len()]
    };
    let probabilities = normalized_probabilities(weights, config.generator.uniform_exploration_ppm);
    let atomic_count = surface.atomic_actions.len();
    let mut concrete = surface
        .atomic_actions
        .into_iter()
        .zip(probabilities[..atomic_count].iter().copied())
        .map(|(input, probability)| ConcreteAction { input, probability })
        .filter(|action| {
            !config.max_potion_expenditures.is_some_and(|limit| {
                potion_expenditures >= limit
                    && matches!(
                        action.input,
                        sts_core::state::core::ClientInput::UsePotion { .. }
                            | sts_core::state::core::ClientInput::DiscardPotion(_)
                    )
            })
        })
        .collect::<Vec<_>>();

    for (family, family_probability) in surface
        .selection_families
        .into_iter()
        .zip(probabilities[atomic_count..].iter().copied())
    {
        let mut cursor = match SelectionTransactionCursor::new(&family) {
            Ok(cursor) => cursor,
            Err(kind) => {
                gaps.push(TurnOptionGenerationGap {
                    kind,
                    exact_state_hash: exact_hash(position),
                    action_depth: 0,
                });
                continue;
            }
        };
        let total = cursor.remaining_input_count();
        let members = std::iter::from_fn(|| cursor.next_input())
            .take(config.max_structured_members_per_family.max(1))
            .collect::<Vec<_>>();
        if members.len() < total {
            counters.truncated_structured_families =
                counters.truncated_structured_families.saturating_add(1);
        }
        if members.is_empty() {
            continue;
        }
        let member_probabilities = if family.declared_min == 1 && family.effective_max == 1 {
            let weights = policy.structured_selection_member_weights(position, &family, &members);
            let weights = if weights.len() == members.len() {
                weights
            } else {
                vec![1.0; members.len()]
            };
            normalized_probabilities(weights, config.generator.uniform_exploration_ppm)
        } else {
            vec![1.0 / total.max(1) as f64; members.len()]
        };
        concrete.extend(members.into_iter().zip(member_probabilities).map(
            |(input, probability)| ConcreteAction {
                input,
                probability: family_probability * probability,
            },
        ));
    }
    concrete
}

fn retain_partial_frontier(
    candidates: Vec<PartialTurn>,
    width: usize,
    retained_per_view: usize,
    policy: &dyn super::policy::CombatActionPolicy,
) -> Vec<PartialTurn> {
    if candidates.len() <= width {
        return candidates;
    }
    let mut views = Vec::<Vec<usize>>::new();
    let mut anchor = (0..candidates.len()).collect::<Vec<_>>();
    anchor.sort_by(|left, right| compare_partial(&candidates[*left], &candidates[*right]));
    views.push(anchor.clone());

    let mut guide_views = BTreeMap::<CombatGuideLaneId, Vec<(CombatStateGuideRank, usize)>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for guide in policy.turn_generation_guides(&candidate.position) {
            guide_views
                .entry(guide.lane)
                .or_default()
                .push((guide.rank, index));
        }
    }
    for (_, mut view) in guide_views {
        view.sort_by(|(left_rank, left_index), (right_rank, right_index)| {
            right_rank
                .cmp(left_rank)
                .then_with(|| compare_partial(&candidates[*left_index], &candidates[*right_index]))
        });
        views.push(view.into_iter().map(|(_, index)| index).collect());
    }

    let mut selected = Vec::with_capacity(width);
    let mut seen = HashSet::new();
    for rank in 0..retained_per_view {
        for view in &views {
            if let Some(index) = view.get(rank).copied() {
                if seen.insert(index) {
                    selected.push(index);
                    if selected.len() == width {
                        break;
                    }
                }
            }
        }
        if selected.len() == width {
            break;
        }
    }
    for index in anchor {
        if selected.len() == width {
            break;
        }
        if seen.insert(index) {
            selected.push(index);
        }
    }
    selected
        .into_iter()
        .map(|index| candidates[index].clone())
        .collect()
}

fn compare_partial(left: &PartialTurn, right: &PartialTurn) -> std::cmp::Ordering {
    left.anchor_cost()
        .total_cmp(&right.anchor_cost())
        .then_with(|| {
            left.negative_log_policy
                .total_cmp(&right.negative_log_policy)
        })
        .then_with(|| exact_hash(&left.position).cmp(&exact_hash(&right.position)))
}

fn partial_path_is_better(candidate: &PartialTurn, incumbent: &PartialTurn) -> bool {
    compare_partial(candidate, incumbent).is_lt()
}

fn complete_path_is_better(candidate: &CompleteTurnOption, incumbent: &CompleteTurnOption) -> bool {
    candidate
        .negative_log_policy()
        .total_cmp(&incumbent.negative_log_policy())
        .then_with(|| candidate.actions().len().cmp(&incumbent.actions().len()))
        .is_lt()
}

fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}
