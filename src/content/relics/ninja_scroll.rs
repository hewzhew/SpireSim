use crate::runtime::action::{ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Ninja Scroll: Start each combat with 3 Shivs in hand.
pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    // Add 3 Shivs to hand
    actions.push(ActionInfo {
        action: crate::content::cards::make_constructed_temp_card_in_hand_action(
            crate::content::cards::CardId::Shiv,
            3,
            false,
            state,
        ),
        insertion_mode: AddTo::Bottom,
    });

    actions
}
