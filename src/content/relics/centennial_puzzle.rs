use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct CentennialPuzzle;

impl CentennialPuzzle {
    pub fn on_lose_hp(
        relic_state: &mut crate::content::relics::RelicState,
        damage_amount: i32,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        if damage_amount > 0 && !relic_state.used_up {
            relic_state.used_up = true;
            actions.push(ActionInfo {
                action: Action::DrawCards(3),
                insertion_mode: AddTo::Top,
            });
        }
        actions
    }

    pub fn at_pre_battle(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        // Java: usedThisCombat = false;
        relic_state.used_up = false;
        SmallVec::new()
    }
}
