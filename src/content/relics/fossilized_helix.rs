use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// FossilizedHelix: At the start of each combat, gain 1 Buffer.
/// Java: atBattleStart() → addToBot(ApplyPowerAction(BufferPower, 1))
pub struct FossilizedHelix;

impl FossilizedHelix {
    pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Buffer,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom, // Java: addToBot
        });
        actions
    }
}
