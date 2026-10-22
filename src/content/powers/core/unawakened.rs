pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Unawakened,
        round_duration: false,
    };

use crate::EntityId;

pub fn on_death(
    _owner: EntityId,
    _amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    // Java `UnawakenedPower` has no onDeath override. The first-phase death
    // transition belongs to `AwakenedOne.damage()`, because it mutates monster
    // state and move history immediately rather than as a normal power hook.
    smallvec::SmallVec::new()
}
