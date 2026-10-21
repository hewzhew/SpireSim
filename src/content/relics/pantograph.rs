use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let mut is_boss_combat = false;
    for m in &state.entities.monsters {
        if m.enemy_id.is_boss() {
            is_boss_combat = true;
            break;
        }
    }
    if is_boss_combat {
        actions.push(ActionInfo {
            action: crate::runtime::action::Action::Heal {
                target: 0,
                amount: 25,
            },
            insertion_mode: crate::runtime::action::AddTo::Top,
        });
    }
    actions
}
