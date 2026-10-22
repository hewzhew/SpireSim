use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Shifting,
        round_duration: false,
    };

pub fn on_attacked(
    state: &CombatState,
    owner: EntityId,
    damage: i32,
    _source: EntityId,
    _power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if damage > 0 {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount: -damage,
        });
        let has_artifact = state.entities.power_db.get(&owner).is_some_and(|powers| {
            powers
                .iter()
                .any(|p| p.power_type == PowerId::Artifact && p.amount > 0)
        });
        if !has_artifact {
            actions.push(Action::ApplyPower {
                source: owner,
                target: owner,
                power_id: PowerId::Shackled,
                amount: damage,
            });
        }
    }

    actions
}

pub fn at_end_of_turn(_owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    let actions = smallvec::smallvec![];
    // Shifting applies paired Shackled stacks on damage; Shackled owns the
    // end-of-turn Strength restoration and removes itself.
    actions
}
