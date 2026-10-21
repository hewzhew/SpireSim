use crate::content::powers::{store, PowerId};
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatPhase, CombatState};

/// Java Unceasing Top:
/// - atPreBattle: canDraw = false
/// - atTurnStart: canDraw = true, disabledUntilEndOfTurn = false
/// - onRefreshHand: if the action queue is empty and the hand is empty during
///   player combat control, draw 1 unless No Draw or the relic is disabled.
///
/// Rust stores canDraw in `amount` and disabledUntilEndOfTurn in `used_up`.
pub fn at_pre_battle(relic: &mut RelicState) {
    relic.amount = 0;
    relic.used_up = false;
}

pub fn at_turn_start(relic: &mut RelicState) {
    relic.amount = 1;
    relic.used_up = false;
}

pub fn disable_until_turn_ends(relic: &mut RelicState) {
    relic.used_up = true;
}

pub fn maybe_on_refresh_hand(state: &mut CombatState) -> bool {
    if state.turn.current_phase != CombatPhase::PlayerTurn
        || state.has_pending_actions()
        || !state.zones.queued_cards.is_empty()
        || !state.zones.hand.is_empty()
        || store::has_power(state, 0, PowerId::NoDraw)
        || state.zones.draw_pile.is_empty() && state.zones.discard_pile.is_empty()
    {
        return false;
    }

    let Some(relic) = state
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::UnceasingTop)
    else {
        return false;
    };

    if relic.amount == 0 || relic.used_up {
        return false;
    }

    state.queue_action_back(Action::DrawCards(1));
    true
}
