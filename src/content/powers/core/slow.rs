use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Slow,
        round_duration: false,
    };

pub fn on_calculate_damage_from_player(
    _state: &CombatState,
    _card: &CombatCard,
    _target: EntityId,
    base_damage: f32,
    amount: i32,
) -> f32 {
    let multiplier = 1.0 + (amount as f32 * 0.1);
    base_damage * multiplier
}

pub fn on_card_played(owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Slow,
        amount: 1,
    }]
}

pub fn at_end_of_round(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    if amount <= 0 {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Slow,
        amount: -amount,
    }]
}
