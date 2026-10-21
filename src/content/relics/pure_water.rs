use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: crate::content::cards::make_constructed_temp_card_in_hand_action(
            crate::content::cards::CardId::Miracle,
            1,
            false,
            state,
        ),
        insertion_mode: crate::runtime::action::AddTo::Bottom,
    });
    actions
}
