use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub struct BagOfMarbles;

impl BagOfMarbles {
    pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        for monster in &state.entities.monsters {
            actions.push(ActionInfo {
                action: Action::ApplyPower {
                    source: state.entities.player.id,
                    target: monster.id,
                    power_id: crate::content::powers::PowerId::Vulnerable,
                    amount: 1,
                },
                insertion_mode: AddTo::Bottom,
            });
        }
        actions
    }
}
