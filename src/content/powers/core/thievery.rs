use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Thievery,
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
