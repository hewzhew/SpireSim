use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn at_end_of_turn(
    state: &CombatState,
    _relic: &mut RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();

    // Gain 1 Block for each card in hand
    let cards_in_hand = state.zones.hand.len();
    if cards_in_hand > 0 {
        actions.push(ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: cards_in_hand as i32,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
