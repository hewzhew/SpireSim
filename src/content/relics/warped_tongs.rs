use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// WarpedTongs: after start-of-turn draw, upgrade a random card in hand for this combat.
/// Java: atTurnStartPostDraw -> addToBot(UpgradeRandomCardAction()).
pub fn at_turn_start_post_draw() -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::UpgradeRandomCard,
        insertion_mode: AddTo::Bottom,
    }]
}
