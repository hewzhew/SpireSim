use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;
use super::node_action_ordering::{order_node_actions, OrderedNodeAction};
use super::node_action_surface::collect_node_action_surface;
use super::node_child_observers::initialize_node_child_observers;

pub(super) struct PreparedNodeExpansion {
    pub(super) position: CombatPosition,
    pub(super) pending_choice: Option<PendingChoiceProfile>,
    pub(super) action_prior_state_hash: Option<String>,
    pub(super) ordered_choices: Vec<OrderedNodeAction>,
    pub(super) turn_branching: TurnBranchingStateObservation,
    pub(super) turn_local_dominance: TurnLocalDominanceStateObservation,
}

pub(super) fn prepare_node_expansion(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
) -> Option<PreparedNodeExpansion> {
    loop_state.record_node_expanded(node);
    let expansion_started = Instant::now();
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    let surface = collect_node_action_surface(loop_state, node, &position, stepper);
    if surface.legal.is_empty() {
        loop_state.mark_unmaterialized_root_surface_complete(node);
        loop_state.record_unresolved_leaf(node);
        record_expansion_elapsed(loop_state, expansion_started);
        return None;
    }

    let ordered = order_node_actions(
        loop_state,
        node,
        surface.legal,
        surface.pending_choice.as_ref(),
        config,
    );
    loop_state.observe_enumerated_root_surface(node, &ordered.ordered_choices);
    let (turn_branching, turn_local_dominance) =
        initialize_node_child_observers(node, ordered.ordered_choices.len());
    record_expansion_elapsed(loop_state, expansion_started);

    Some(PreparedNodeExpansion {
        position,
        pending_choice: surface.pending_choice,
        action_prior_state_hash: ordered.action_prior_state_hash,
        ordered_choices: ordered.ordered_choices,
        turn_branching,
        turn_local_dominance,
    })
}

fn record_expansion_elapsed(loop_state: &mut SearchLoopState, started: Instant) {
    loop_state.performance.expansion_elapsed_us = loop_state
        .performance
        .expansion_elapsed_us
        .saturating_add(started.elapsed().as_micros());
}
