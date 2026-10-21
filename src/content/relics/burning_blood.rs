use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub struct BurningBlood;

pub const VICTORY_HEAL_AMOUNT: i32 = 6;

impl BurningBlood {
    pub fn on_victory(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        if state.entities.player.current_hp <= 0 {
            return actions;
        }
        actions.push(ActionInfo {
            action: Action::Heal {
                target: 0,
                amount: VICTORY_HEAL_AMOUNT,
            },
            insertion_mode: AddTo::Top,
        });
        actions
    }
}
