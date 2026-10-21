use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub fn on_post_draw(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    if amount <= 0 {
        return actions;
    }

    for monster in &state.entities.monsters {
        if monster.current_hp <= 0 || monster.is_dying || monster.is_escaped {
            continue;
        }
        actions.push(Action::ApplyPower {
            source: owner,
            target: monster.id,
            power_id: PowerId::Poison,
            amount,
        });
    }

    actions
}
