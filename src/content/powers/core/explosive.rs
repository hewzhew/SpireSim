use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::EntityId;

pub const EXPLOSION_DAMAGE: i32 = 30;
pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Explosive,
        round_duration: false,
    };

pub fn during_turn(owner: EntityId, power_amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if power_amount == 1 {
        actions.push(Action::Suicide {
            target: owner,
            trigger_relics: true,
        });
        actions.push(Action::Damage(DamageInfo {
            source: owner,
            target: 0,
            base: EXPLOSION_DAMAGE,
            output: EXPLOSION_DAMAGE,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }));
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Explosive,
            amount: 1,
        });
    }

    actions
}
