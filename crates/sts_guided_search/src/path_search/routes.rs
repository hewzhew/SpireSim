//! Complete, replay-checked victories for a fixed resource query.
//! No search trees, teacher scores, optimality labels or suffix resampling.
use super::{admission::AdmittedChoices, choice_groups, prior, PathContext, PathPrior};
use crate::continuation::validate_reference_trajectory;
use crate::observation::{combat_observation, model_choices, CombatObservation, ModelChoice};
use crate::{
    CombatBranch, CombatSearchSpace, CombatSearchState, ResourceRequirement, SearchSpace,
    Transition,
};
use serde::Serialize;
use std::time::Instant;
use sts_combat_outcome::combat_witness_materialization::{
    materialize_replay_checked_combat_outcome_v1, starting_potion_slots_targeted_by_input_v1,
    CombatInputExpendituresV1, CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits};
use sts_core::state::core::ClientInput;

pub const ROUTE_SCHEMA: u32 = 1;

/// Count limit covers virtual assembly as well as real inputs. Deadline and
/// per-input engine limit apply to both reconstruction and independent replay.
pub struct RouteLimits {
    pub max_decisions: usize,
    pub step: CombatStepLimits,
}

#[derive(Clone, Debug, Serialize)]
pub struct RouteDecision {
    /// In-memory source for the exact runtime prior interface. Disk consumers
    /// reconstruct it from root + inputs; this observation is not state identity.
    #[serde(skip)]
    pub state: CombatSearchState,
    pub observation: CombatObservation,
    pub branches: Vec<CombatBranch>,
    pub choices: Vec<ModelChoice>,
    pub groups: Vec<usize>,
    pub selected: usize,
    /// Evidence coordinates; not features or distance-to-answer labels.
    pub input_index: usize,
    pub expenditures_before: CombatInputExpendituresV1,
    pub spent_originals_before: u64,
    /// Current slot -> original bottle mask, matched by the shared identity owner.
    pub original_potion_slots: Vec<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompleteRoute {
    pub schema: u32,
    pub root: CombatPosition,
    pub goal: ResourceRequirement,
    pub inputs: Vec<ClientInput>,
    pub decisions: Vec<RouteDecision>,
    pub successor: CombatPosition,
    pub outcome: CombatWitnessMaterializedOutcomeV1,
    pub reconstruction_engine_steps: usize,
    pub replay_engine_steps: usize,
}

impl CompleteRoute {
    /// Diagnostic likelihood of this known route, not a training objective or
    /// a claim that this is the only good route. Scores exactly the admitted rows.
    pub fn log_probability<P: PathPrior>(&self, prior: &P) -> Result<f64, String> {
        self.prior_cost(prior, false)
    }

    /// The same accumulated discrepancy used by RelativePrior search ordering.
    /// This scores a supplied route, not unsearched alternatives or outcome value.
    pub fn relative_prior_cost<P: PathPrior>(&self, prior: &P) -> Result<f64, String> {
        self.prior_cost(prior, true)
    }

    fn prior_cost<P: PathPrior>(&self, prior: &P, relative: bool) -> Result<f64, String> {
        let revision = prior.revision().to_owned();
        let mut sum = 0.0;
        for decision in &self.decisions {
            let logs = prior.log_probabilities(
                &decision.state,
                PathContext {
                    root: &self.root,
                    goal: &self.goal,
                    spent_originals: decision.spent_originals_before,
                },
                &decision.branches,
            )?;
            prior::validate(&logs, decision.branches.len())?;
            let selected = logs
                .get(decision.selected)
                .ok_or("invalid route choice index")?;
            sum += if relative {
                logs.iter().copied().fold(f64::NEG_INFINITY, f64::max) - selected
            } else {
                *selected
            };
            if !sum.is_finite() || prior.revision() != revision {
                return Err("invalid or changing route prior".into());
            }
        }
        Ok(sum)
    }
}

/// Rebuild a full supplied victory under current semantics. Nothing is returned
/// for a partial, illegal, ambiguous, drifted or resource-unqualified route.
/// Callers retain source provenance separately; replay does not certify a teacher.
pub fn reconstruct_route(
    root: &CombatPosition,
    goal: &ResourceRequirement,
    inputs: &[ClientInput],
    expected_successor: &CombatPosition,
    limits: RouteLimits,
) -> Result<CompleteRoute, String> {
    reconstruct_victory(root, goal, inputs, expected_successor, limits, true)
}

/// Explicit preference evidence may miss the final HP target. All original
/// potion restrictions and independent replay checks still apply. Such a victory
/// is not a qualified positive example and carries no label about its actions.
pub fn reconstruct_observed_victory(
    root: &CombatPosition,
    goal: &ResourceRequirement,
    inputs: &[ClientInput],
    expected_successor: &CombatPosition,
    limits: RouteLimits,
) -> Result<CompleteRoute, String> {
    reconstruct_victory(root, goal, inputs, expected_successor, limits, false)
}

/// Strict resource preference between already reconstructed complete routes.
/// Greater means the left outcome dominates; equal/incomparable yields None.
/// Exact roots include RNG and card identity; similar observations do not qualify.
pub fn resource_preference(
    left: &CompleteRoute,
    right: &CompleteRoute,
) -> Result<Option<std::cmp::Ordering>, String> {
    if left.root != right.root || left.goal != right.goal {
        return Err("route preference needs the same exact root and resource query".into());
    }
    Ok(if left.outcome.strictly_dominates(&right.outcome) {
        Some(std::cmp::Ordering::Greater)
    } else if right.outcome.strictly_dominates(&left.outcome) {
        Some(std::cmp::Ordering::Less)
    } else {
        None
    })
}

fn reconstruct_victory(
    root: &CombatPosition,
    goal: &ResourceRequirement,
    inputs: &[ClientInput],
    expected_successor: &CombatPosition,
    limits: RouteLimits,
    require_goal: bool,
) -> Result<CompleteRoute, String> {
    goal.validate(root)?;
    if inputs.is_empty() || inputs.len() > limits.max_decisions || limits.step.max_engine_steps == 0
    {
        return Err("route needs nonempty inputs within positive decision/engine limits".into());
    }
    let mut state = CombatSearchState::new(root.clone())?;
    let space = CombatSearchSpace::default();
    if space.terminal(&state).is_some() {
        return Err("route needs an unresolved combat root".into());
    }
    // Independent raw-input replay, without grammar conversion or prior.
    let (replayed, replay_engine_steps) =
        validate_reference_trajectory(root, inputs, expected_successor, limits.step)?;
    let outcome = materialize_replay_checked_combat_outcome_v1(root, inputs, expected_successor)?;
    if require_goal && !goal.accepts(&outcome) {
        return Err("route victory does not meet the fixed resource goal".into());
    }
    let mut decisions = Vec::new();
    let mut expenditures = CombatInputExpendituresV1::default();
    let mut spent_originals = 0;
    let mut engine_steps = 0;
    for (input_index, input) in inputs.iter().enumerate() {
        if space.terminal(&state).is_some() {
            return Err("route contains inputs after combat ended".into());
        }
        let mut prepared = AdmittedChoices::new(root, &state, goal)?;
        let path = prepared.input_path(input)?;
        for (step_index, branch) in path.iter().enumerate() {
            if decisions.len() >= limits.max_decisions {
                return Err("route reconstruction reached the decision limit".into());
            }
            if limits.step.deadline.is_some_and(|d| Instant::now() >= d) {
                return Err("route reconstruction reached the deadline".into());
            }
            if step_index != 0 {
                prepared = AdmittedChoices::new(root, &state, goal)?;
            }
            let branches = prepared.branches();
            let mut matches = branches.iter().enumerate().filter(|(_, b)| *b == branch);
            let selected = matches
                .next()
                .map(|(i, _)| i)
                .ok_or("recorded route choice is excluded by current admission")?;
            if matches.next().is_some() {
                return Err("ambiguous admitted route choice".into());
            }
            let compiled = prepared.input(selected)?;
            if (step_index + 1 == path.len()) != compiled.is_some()
                || compiled.as_ref().is_some_and(|actual| actual != input)
            {
                return Err("route grammar does not reproduce the recorded input".into());
            }
            let choices = model_choices(&state, &branches.iter().collect::<Vec<_>>())?;
            let groups = choice_groups(&state, &branches)?;
            let observation = combat_observation(&state)?;
            let original_potion_slots = state
                .position()
                .combat
                .entities
                .potions
                .iter()
                .enumerate()
                .map(|(slot, _)| {
                    starting_potion_slots_targeted_by_input_v1(
                        root,
                        state.position(),
                        &ClientInput::DiscardPotion(slot),
                    )
                })
                .collect();
            let (next, used) = match prepared.transition(selected, limits.step)? {
                Transition::Settled {
                    state,
                    engine_steps,
                } => (state, engine_steps),
                Transition::Interrupted { cause, .. } => {
                    return Err(format!("route reconstruction interrupted: {cause:?}"));
                }
            };
            engine_steps += used;
            let mut next_expenditures = expenditures;
            if let Some(actual) = &compiled {
                next_expenditures.record(actual);
                if next.position() != &replayed[input_index + 1] {
                    return Err("route grammar differs from independent replay successor".into());
                }
            }
            let next_spent = goal.spent_originals(root, next_expenditures, next.position());
            if next_spent & goal.preserve_starting_potion_slots != 0 {
                return Err("route successor spends a reserved original potion".into());
            }
            decisions.push(RouteDecision {
                state,
                observation,
                branches,
                choices,
                groups,
                selected,
                input_index,
                expenditures_before: expenditures,
                spent_originals_before: spent_originals,
                original_potion_slots,
            });
            state = next;
            expenditures = next_expenditures;
            spent_originals = next_spent;
        }
    }
    if space.terminal(&state) != Some(true) || state.position() != expected_successor {
        return Err("reconstructed route did not reach the supplied exact victory".into());
    }
    Ok(CompleteRoute {
        schema: ROUTE_SCHEMA,
        root: root.clone(),
        goal: goal.clone(),
        inputs: inputs.to_vec(),
        decisions,
        successor: state.position().clone(),
        outcome,
        reconstruction_engine_steps: engine_steps,
        replay_engine_steps,
    })
}

#[cfg(test)]
mod tests;
