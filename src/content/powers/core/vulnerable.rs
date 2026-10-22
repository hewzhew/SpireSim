pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Vulnerable,
        round_duration: true,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn on_calculate_damage_from_player(mut damage: f32, amount: i32, multiplier: f32) -> f32 {
    if amount > 0 {
        damage *= multiplier;
    }
    damage
}

pub fn on_attacked_to_change_damage(current_damage: i32, amount: i32, multiplier: f32) -> i32 {
    if amount > 0 {
        (current_damage as f32 * multiplier) as i32
    } else {
        current_damage
    }
}

pub fn at_end_of_round(
    owner: EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if just_applied {
        return actions;
    }
    if amount == 0 {
        actions.push(Action::RemovePower {
            target: owner,
            power_id: PowerId::Vulnerable,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Vulnerable,
            amount: 1,
        });
    }
    actions
}
