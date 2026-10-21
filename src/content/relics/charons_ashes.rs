use crate::runtime::action::DamageType;
use crate::runtime::action::{Action, ActionInfo, AddTo, NO_SOURCE};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub struct CharonsAshes;

impl CharonsAshes {
    pub fn on_exhaust(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        let mut damages = SmallVec::new();
        for _ in 0..state.entities.monsters.len() {
            damages.push(3);
        }

        actions.push(ActionInfo {
            action: Action::DamageAllEnemies {
                source: NO_SOURCE,
                damages,
                damage_type: DamageType::Thorns,
                is_modified: false,
            },
            insertion_mode: AddTo::Top,
        });
        actions
    }
}
