use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub const MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::GenericStrengthUp,
        round_duration: false,
    };

pub fn at_end_of_round(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if amount > 0 {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount,
        });
    }
    actions
}
