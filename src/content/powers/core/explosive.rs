use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::EntityId;

pub const EXPLOSION_DAMAGE: i32 = 30;
pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "own turns remaining until explosion",
    effect: "At 1, kill the owner and then deal 30 Thorns damage to the player. Player Block absorbs this damage; Strength, Weak and Vulnerable do not modify it. At higher amounts, decrease by 1. Killing the owner earlier prevents its countdown from running.",
    timing: "Runs during the owner's turn, queued after its move actions and before the next monster's move actions, not at end of round.",
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
