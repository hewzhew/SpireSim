use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Pen Nib: Every 10th Attack you play deals double damage.
pub fn on_use_card(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    relic_state.counter += 1;

    if relic_state.counter == 10 {
        relic_state.counter = 0;
    } else if relic_state.counter == 9 {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::PenNibPower,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}

pub fn at_battle_start(counter: i32) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if counter == 9 {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::PenNibPower,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
