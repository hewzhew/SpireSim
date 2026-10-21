use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Gold stolen per stealing move, capped by available Gold",
    effect: "The thief's stealing moves take Gold before attack damage; Block does not prevent theft. Killing the thief recovers its stolen Gold in rewards; escape does not.",
    timing: "Stealing moves only; not an automatic effect on every hit or turn. No round decay.",
    round_duration: false,
};

// Thievery power tracks the amount of gold stolen. Doesn't heavily impact combat interactions.
pub fn on_monster_turn_ended(
    _state: &CombatState,
    _owner: EntityId,
    _power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![]
}
