use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// MarkOfPain: At the start of combat, shuffle 2 Wounds into your draw pile.
/// Also grants +1 Energy (passive, handled by base_energy in combat init).
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::MakeTempCardInDrawPile {
            card_id: crate::content::cards::CardId::Wound,
            amount: 2,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
