use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "card-play counter for the current round",
    effect: "Player Attack card damage against this owner is multiplied by 1 + 0.1 times amount. Skills and Powers also increase the counter when played, so earlier setup cards can amplify later Attacks.",
    timing: "Each player card play adds 1. At the end of the round, reset the amount to 0 and keep the power present. A turn-start reading of 0 is the reset counter; it can grow again during that turn.",
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
