use crate::runtime::action::Action;
use crate::EntityId;

// Life Link is a presence marker. Half-death, group defeat and revival are
// implemented by beyond::darkling and the central monster death handler.
pub const MECHANICS: super::super::mechanics::PowerMechanics =
    crate::content::monsters::beyond::darkling::LIFE_LINK_MECHANICS;
pub fn at_end_of_turn(_owner: EntityId, _amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![]
}
