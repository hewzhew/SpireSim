pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence flag, not a numeric stack count",
    effect: "Awakened One's first phase. First lethal damage starts Rebirth instead of final death: remove debuffs, Curiosity, Unawakened and Shackled; preserve positive Strength and Regenerate. Rebirth restores full HP without attacking; Dark Echo follows. The transition belongs to the monster's damage handler.",
    timing: "Removed immediately at first-phase lethal damage.",
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
