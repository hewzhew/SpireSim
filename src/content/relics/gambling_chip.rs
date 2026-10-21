use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Gambling Chip: At the start of each combat, after drawing cards,
/// discard any number of cards then draw that many.
///
/// Java: atTurnStartPostDraw() (NOT atBattleStart!)
///   - activated flag ensures it only fires on the first turn.
///   - addToBot(GamblingChipAction) which opens hand select to discard any number, then draw that many.
///
pub fn at_battle_start_pre_draw(relic: &mut RelicState) {
    relic.used_up = false;
}

pub fn at_turn_start_post_draw(relic: &mut RelicState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if relic.used_up {
        return actions;
    }

    relic.used_up = true;
    actions.push(ActionInfo {
        action: Action::SuspendForHandSelect {
            min: 0,
            max: 99,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::GamblingChip,
        },
        insertion_mode: AddTo::Bottom, // Java: addToBot
    });
    actions
}
