use crate::runtime::action::{ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Holy Water: Replaces Pure Water. At the start of each combat, add 3 Miracles to your hand.
pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: crate::content::cards::make_constructed_temp_card_in_hand_action(
            crate::content::cards::CardId::Miracle,
            3,
            false,
            state,
        ),
        insertion_mode: AddTo::Bottom,
    });
    actions
}
