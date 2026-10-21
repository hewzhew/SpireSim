use super::super::*;
use super::loop_state::SearchLoopState;

pub(super) struct NodeActionSurface {
    pub(super) legal: Vec<CombatActionChoice>,
    pub(super) pending_choice: Option<PendingChoiceProfile>,
}

pub(super) fn collect_node_action_surface(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    position: &CombatPosition,
    stepper: &impl CombatStepper,
) -> NodeActionSurface {
    let legal = filtered_legal_actions(
        stepper.atomic_action_choices(position),
        loop_state.plugins.potion.policy,
        loop_state.plugins.potion.allowed_potion_slots,
        loop_state.plugins.potion.allow_potion_discard,
        &node.combat,
    );
    let pending_choice = summarize_pending_choice(&node.engine);
    loop_state
        .diagnostics
        .observe_pending_choice(pending_choice.as_ref());
    let expansion = summarize_action_expansion(&node.engine, &node.combat, &legal);
    loop_state.diagnostics.observe_legal_actions(&expansion);
    let turn_prefix = summarize_turn_prefix(&node.turn_prefix, legal.len());
    loop_state.diagnostics.observe_turn_prefix(&turn_prefix);
    let turn_sequence = summarize_turn_sequence(node, legal.len());
    loop_state
        .diagnostics
        .observe_turn_sequence(&turn_sequence, node);
    let card_identity = summarize_card_identity(&node.combat);
    loop_state.diagnostics.observe_card_identity(&card_identity);
    let target_fanout = summarize_target_fanout(&node.combat, &legal);
    loop_state.diagnostics.observe_target_fanout(&target_fanout);

    NodeActionSurface {
        legal,
        pending_choice,
    }
}
