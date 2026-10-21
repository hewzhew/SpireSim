use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageType};

pub fn on_attack(
    owner: usize,
    target: usize,
    damage: i32,
    damage_type: DamageType,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    if damage > 0 && target != owner && damage_type == DamageType::Normal {
        smallvec::smallvec![Action::ApplyPower {
            source: owner,
            target,
            power_id: PowerId::Poison,
            amount,
        }]
    } else {
        smallvec::smallvec![]
    }
}
