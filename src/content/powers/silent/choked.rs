use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn on_card_played(
    _state: &CombatState,
    owner: EntityId,
    _card: &CombatCard,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::LoseHp {
        target: owner,
        amount,
        triggers_rupture: false,
    }]
}

pub fn at_turn_start(owner: EntityId) -> SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::RemovePower {
        target: owner,
        power_id: crate::content::powers::PowerId::Choked,
    }]
}
