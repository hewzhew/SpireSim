use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub fn at_end_of_turn(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    if owner != 0
        || amount <= 0
        || state.zones.hand.is_empty()
        || state
            .entities
            .player
            .has_relic(crate::content::relics::RelicId::RunicPyramid)
        || store::has_power(state, owner, PowerId::Equilibrium)
    {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::SuspendForHandSelect {
        min: 0,
        max: amount.min(u8::MAX as i32) as u8,
        can_cancel: true,
        filter: crate::state::HandSelectFilter::Any,
        reason: crate::state::HandSelectReason::Retain,
    }]
}
