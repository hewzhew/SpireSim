pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "card counter toward 12, retained across turns",
    effect: "Each counted player card increments the counter. The 12th forces the player's turn to end, resets the counter to 0, and gives all living enemies 2 permanent Strength. Ending a normal turn does not reset it. Potions are not card plays.",
    timing: "Card-play trigger and forced-end sequence; persists across turns.",
    round_duration: false,
};

use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;

pub fn on_player_card_played(
    owner: EntityId,
    amount: i32,
    _card: &CombatCard,
    _state: &CombatState,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();

    // Phase 1: Increment Time Warp counter
    if amount < 11 {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::TimeWarp,
            amount: 1,
        });
    } else {
        actions.push(Action::TriggerTimeWarpEndTurn { owner });
    }

    actions
}
