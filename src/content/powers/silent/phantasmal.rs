use crate::content::powers::PowerId;
use crate::runtime::action::Action;

pub fn at_start_of_turn(owner: usize) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![
        Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::DoubleDamage,
            amount: 1,
        },
        Action::ReducePower {
            target: owner,
            power_id: PowerId::Phantasmal,
            amount: 1,
        },
    ]
}

pub fn double_damage_at_end_of_round(owner: usize, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    if amount == 0 {
        smallvec::smallvec![Action::RemovePower {
            target: owner,
            power_id: PowerId::DoubleDamage,
        }]
    } else {
        smallvec::smallvec![Action::ReducePower {
            target: owner,
            power_id: PowerId::DoubleDamage,
            amount: 1,
        }]
    }
}
