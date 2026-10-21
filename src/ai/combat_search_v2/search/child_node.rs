use crate::sim::combat::CombatStepResult;

use super::super::*;

pub(super) struct BuiltChildNode {
    pub(super) node: SearchNode,
    pub(super) turn_transition: TurnBranchTransition,
    pub(super) truncated: bool,
}

pub(super) fn build_child_node(
    parent: &SearchNode,
    step: CombatStepResult,
    ordered_choice: IndexedActionChoice,
    action_ordering_frontier_hint: i32,
    action_prior_state_hash: Option<&str>,
    potion_tactical_priority: Option<i32>,
    config: &CombatSearchV2Config,
) -> BuiltChildNode {
    let action_id = ordered_choice.original_action_id;
    let choice = ordered_choice.choice;
    let truncated = step.truncated;
    let mut child = parent.clone_for_child(step.position.engine, step.position.combat);
    let turn_transition = classify_turn_branch_transition(
        &parent.engine,
        &parent.combat,
        &choice.input,
        &child.engine,
        &child.combat,
    );
    child.note_turn_prefix(&parent.combat, &choice.input, turn_transition);
    child.note_input(&choice.input);
    child.note_action_prior_score(action_prior_state_hash.and_then(|state_hash| {
        config
            .root_action_prior
            .as_ref()
            .and_then(|prior| prior.score(state_hash, &choice.action_key))
    }));
    child.note_action_ordering_frontier_hint(action_ordering_frontier_hint);
    child.note_potion_tactical_priority(potion_tactical_priority);
    child.note_turn_branch_priority(turn_transition.frontier_priority_hint());
    child.push_action(CombatSearchV2ActionTrace {
        step_index: parent.actions.len(),
        action_id,
        action_key: choice.action_key,
        action_debug: choice.action_debug,
        input: choice.input,
    });
    BuiltChildNode {
        node: child,
        turn_transition,
        truncated,
    }
}
