use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub const ADDITIONAL_CARDS_DRAWN_EACH_TURN: i32 = 2;

/// Snecko Eye: Draw 2 additional cards each turn. Start each combat Confused.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    // Confusion
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Confusion,
            amount: -1,
        },
        insertion_mode: AddTo::Bottom,
    });

    // Note: The +2 draw per turn is handled natively in the
    // draw phase of the player's turn state via relic check.

    actions
}
