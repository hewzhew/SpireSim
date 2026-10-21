use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub const MECHANICS: &str = "If you played 3 or fewer cards during a player turn, draw 3 additional cards at the start of the next player turn, after the normal draw. Gives no first-turn bonus. Counts cards played, not attack hits; the counter resets each turn. Draw prevention and the 10-card hand limit still apply.";

fn first_turn_flag(relic_state: &RelicState) -> bool {
    relic_state.amount != 0
}

fn set_first_turn_flag(relic_state: &mut RelicState, first_turn: bool) {
    relic_state.amount = i32::from(first_turn);
}

/// Java Pocketwatch.atBattleStart():
///   counter = 0
///   firstTurn = true
pub fn at_battle_start(relic_state: &mut RelicState) {
    relic_state.counter = 0;
    set_first_turn_flag(relic_state, true);
}

/// Java Pocketwatch.onPlayCard():
///   ++counter
pub fn on_use_card(relic_state: &mut RelicState) {
    relic_state.counter += 1;
}

/// Java Pocketwatch.atTurnStartPostDraw():
///   if (counter <= 3 && !firstTurn) draw 3 else firstTurn = false
///   counter = 0
pub fn at_turn_start_post_draw(relic_state: &mut RelicState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    if relic_state.counter <= 3 && !first_turn_flag(relic_state) {
        actions.push(ActionInfo {
            action: Action::DrawCards(3),
            insertion_mode: AddTo::Bottom,
        });
    } else {
        set_first_turn_flag(relic_state, false);
    }

    relic_state.counter = 0;
    actions
}

pub fn on_victory(relic_state: &mut RelicState) {
    relic_state.counter = -1;
    set_first_turn_flag(relic_state, false);
}
