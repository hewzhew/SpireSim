use std::time::Instant;

use super::super::*;
use super::child_dominance::{apply_child_dominance_gate, ChildDominanceOutcome};
use super::child_frontier::enqueue_child_or_remember_leaf;
use super::child_node::{build_child_node, BuiltChildNode};
use super::child_preflight::{prepare_child_for_expansion, ChildPreflightOutcome};
use super::child_rollout::child_rollout_estimate;
use super::child_step::{apply_child_step, ChildStepOutcome};
use super::loop_state::SearchLoopState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChildExpansionOutcome {
    Applied,
    Skipped,
    DeadlineReached,
}

pub(super) struct ChildExpansionInput<'a, S: CombatStepper> {
    pub(super) parent: &'a SearchNode,
    pub(super) position: &'a CombatPosition,
    pub(super) ordered_choice: IndexedActionChoice,
    pub(super) action_ordering_frontier_hint: i32,
    pub(super) action_prior_state_hash: Option<&'a str>,
    pub(super) pending_choice: Option<&'a PendingChoiceProfile>,
    pub(super) stepper: &'a S,
    pub(super) config: &'a CombatSearchV2Config,
    pub(super) deadline: Option<Instant>,
}

pub(super) fn expand_ordered_child<S: CombatStepper>(
    loop_state: &mut SearchLoopState,
    turn_branching: &mut TurnBranchingStateObservation,
    turn_local_dominance: &mut TurnLocalDominanceStateObservation,
    input: ChildExpansionInput<'_, S>,
) -> ChildExpansionOutcome {
    let potion_tactical_priority = match prepare_child_for_expansion(
        loop_state,
        input.parent,
        &input.ordered_choice,
        input.deadline,
    ) {
        ChildPreflightOutcome::Continue {
            potion_tactical_priority,
        } => potion_tactical_priority,
        ChildPreflightOutcome::Advanced => return ChildExpansionOutcome::Skipped,
        ChildPreflightOutcome::DeadlineReached => return ChildExpansionOutcome::DeadlineReached,
    };

    let step = match apply_child_step(
        loop_state,
        input.position,
        &input.ordered_choice.choice.input,
        input.stepper,
        input.config,
        input.deadline,
    ) {
        ChildStepOutcome::Stable(step) => step,
        ChildStepOutcome::StepLimitReached => return ChildExpansionOutcome::Skipped,
        ChildStepOutcome::DeadlineReached => return ChildExpansionOutcome::DeadlineReached,
    };

    let child_bookkeeping_started = Instant::now();
    let BuiltChildNode {
        node: mut child,
        turn_transition,
        truncated,
    } = build_child_node(
        input.parent,
        step,
        input.ordered_choice,
        input.action_ordering_frontier_hint,
        input.action_prior_state_hash,
        potion_tactical_priority,
        input.config,
    );
    loop_state.materialize_root_lineage(&mut child);
    loop_state
        .diagnostics
        .observe_pending_choice_child_transition(input.pending_choice, truncated, &child.engine);
    turn_branching.observe_child(turn_transition);
    loop_state.record_node_generated(&child);
    if !truncated {
        loop_state.observe_exact_root_terminal(&child);
    }
    loop_state.performance.child_bookkeeping_elapsed_us = loop_state
        .performance
        .child_bookkeeping_elapsed_us
        .saturating_add(child_bookkeeping_started.elapsed().as_micros());

    if apply_child_dominance_gate(loop_state, turn_local_dominance, &child, truncated)
        == ChildDominanceOutcome::Pruned
    {
        return ChildExpansionOutcome::Applied;
    }

    child.rollout_estimate = child_rollout_estimate(
        loop_state,
        &child,
        input.stepper,
        input.config,
        input.deadline,
    );

    enqueue_child_or_remember_leaf(loop_state, child, truncated);
    ChildExpansionOutcome::Applied
}
