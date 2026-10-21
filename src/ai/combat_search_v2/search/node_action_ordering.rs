use super::super::*;
use super::loop_state::SearchLoopState;
use crate::ai::combat_search_v2::action_priority::ActionOrderingRole;

pub(super) struct OrderedNodeActions {
    pub(super) action_prior_state_hash: Option<String>,
    pub(super) ordered_choices: Vec<OrderedNodeAction>,
}

pub(super) struct OrderedNodeAction {
    pub(super) choice: IndexedActionChoice,
    pub(super) action_ordering_frontier_hint: i32,
}

pub(super) fn order_node_actions(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    legal: Vec<CombatActionChoice>,
    pending_choice: Option<&PendingChoiceProfile>,
    config: &CombatSearchV2Config,
) -> OrderedNodeActions {
    let equivalence = compress_equivalent_actions(&node.engine, &node.combat, legal);
    loop_state
        .diagnostics
        .observe_action_equivalence(&equivalence.summary);
    let plugins = CombatSearchActionOrderingPlugins::from_stack(
        config.root_action_prior.as_ref(),
        &loop_state.plugins,
    );
    let action_prior_state_hash = plugins
        .root_action_prior
        .filter(|prior| !prior.is_empty())
        .map(|_| combat_exact_state_hash_v2(&node.engine, &node.combat));
    let ordered = order_indexed_action_choices_with_plugins(
        &node.engine,
        &node.combat,
        equivalence.choices,
        plugins,
    );
    loop_state
        .diagnostics
        .observe_action_ordering(&ordered.summary);
    loop_state
        .diagnostics
        .observe_pending_choice_ordering(pending_choice, &ordered.summary);
    let continue_retaliation_protection = matches!(
        ordered.summary.first_role(),
        Some(ActionOrderingRole::CurrentTurnRetaliationProtection)
    );
    let ordered_choices = ordered
        .choices
        .into_iter()
        .enumerate()
        .map(|(index, choice)| OrderedNodeAction {
            choice,
            action_ordering_frontier_hint: i32::from(continue_retaliation_protection && index == 0),
        })
        .collect();
    OrderedNodeActions {
        action_prior_state_hash,
        ordered_choices,
    }
}
