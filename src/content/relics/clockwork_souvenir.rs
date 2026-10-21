use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct ClockworkSouvenir;

impl ClockworkSouvenir {
    pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Artifact,
                amount: 1,
            },
            insertion_mode: AddTo::Top,
        });
        actions
    }
}
