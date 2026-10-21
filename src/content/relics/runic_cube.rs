use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Runic Cube (Ironclad Boss Relic): Whenever you lose HP, draw 1 card.
/// Hook: wasHPLost (Java: addToTop)
pub const CARDS_DRAWN_PER_HP_LOSS_EVENT: u32 = 1;
pub const MINIMUM_HP_LOST_PER_EVENT: i32 = 1;

pub fn was_hp_lost(damage_amount: i32) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if damage_amount >= MINIMUM_HP_LOST_PER_EVENT {
        actions.push(ActionInfo {
            action: Action::DrawCards(CARDS_DRAWN_PER_HP_LOSS_EVENT),
            insertion_mode: AddTo::Top,
        });
    }
    actions
}
