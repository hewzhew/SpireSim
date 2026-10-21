use super::*;
use crate::runtime::combat::CombatCard;

mod card_play_effects;
mod types;

pub(in crate::ai::combat_search_v2) use types::*;

pub(super) fn card_play_effect_facts(
    combat: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
) -> CardPlayEffectFacts {
    card_play_effects::card_play_effect_facts(combat, card, target)
}

#[cfg(test)]
mod tests;
