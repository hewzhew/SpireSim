use crate::runtime::action::{ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use crate::state::core::EngineState;

pub(super) fn resolve_victory_hooks_immediately(combat_state: &mut CombatState) -> bool {
    let mut actions = crate::content::relics::hooks::on_victory(combat_state);
    for power in crate::content::powers::store::powers_snapshot_for(combat_state, 0) {
        let power_actions = crate::content::powers::resolve_power_on_victory(
            power.power_type,
            combat_state,
            0,
            power.amount,
        );
        for action in power_actions {
            actions.push(ActionInfo {
                action,
                insertion_mode: AddTo::Bottom,
            });
        }
    }
    if actions.is_empty() {
        return combat_state.entities.player.current_hp > 0;
    }

    combat_state.queue_actions(actions);
    while let Some(action) = combat_state.pop_next_action() {
        crate::engine::action_handlers::execute_action(action, combat_state);
        combat_state.ensure_flush_next_queued_card();
        if combat_state.entities.player.current_hp <= 0 {
            combat_state.clear_pending_actions();
            return false;
        }
    }
    true
}

pub(super) fn settle_victory_if_ready(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
) -> Option<bool> {
    if !combat_state.are_monsters_basically_dead_java() {
        return None;
    }

    if combat_state.entities.player.current_hp <= 0 {
        combat_state.clear_pending_actions();
        *engine_state = EngineState::GameOver(crate::state::core::RunResult::Defeat);
        return Some(false);
    }

    // Java lets the action manager finish consequences that were already
    // committed by the lethal card (Beat of Death, use-card hooks, monster
    // death hooks, etc.) before invoking room/relic victory hooks.  Triggering
    // victory while one of those actions is still queued can incorrectly put
    // a Burning Blood heal in front of lethal retaliation.
    if combat_state.has_pending_actions() || !combat_state.zones.queued_cards.is_empty() {
        *engine_state = EngineState::CombatProcessing;
        return Some(true);
    }

    if !combat_state.zones.limbo.is_empty() {
        let limbo_cards = std::mem::take(&mut combat_state.zones.limbo);
        for card in limbo_cards {
            combat_state.add_card_to_discard_pile_top(card);
        }
    }

    if !combat_state.turn.counters.victory_triggered {
        combat_state.turn.mark_victory_triggered();
        if !resolve_victory_hooks_immediately(combat_state) {
            *engine_state = EngineState::GameOver(crate::state::core::RunResult::Defeat);
            return Some(false);
        }
    }

    // Java does not cut off queued onUseCard / onDeath aftermath when the last monster dies.
    // Finish draining any already-queued actions (e.g. Rage block, relic hooks, death hooks)
    // before transitioning to rewards.
    if !combat_state.has_pending_actions()
        && combat_state.zones.limbo.is_empty()
        && combat_state.zones.queued_cards.is_empty()
    {
        *engine_state = EngineState::RewardScreen(crate::state::rewards::RewardState::new());
        return Some(false);
    }
    *engine_state = EngineState::CombatProcessing;
    Some(true)
}
