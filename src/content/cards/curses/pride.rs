use crate::content::cards::CardId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn on_end_turn_in_hand(_state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::MakeTempCardInDrawPile {
            card_id: CardId::Pride,
            amount: 1,
            random_spot: false,
            to_bottom: false,
            upgraded: false,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
