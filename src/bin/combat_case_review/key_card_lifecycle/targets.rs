use sts_oracle_runtime::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, InstalledRule, Mechanic, PlayEffect,
    TriggeredEffect,
};
use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::runtime::combat::CombatState;

use super::types::{KeyCardReason, KeyCardTarget};

pub(crate) fn key_card_targets(combat: &CombatState) -> Vec<KeyCardTarget> {
    combat
        .meta
        .master_deck_snapshot
        .iter()
        .filter_map(|card| {
            key_card_reason(card.id, card.upgrades).map(|reason| KeyCardTarget {
                card: card.clone(),
                reason,
            })
        })
        .collect()
}

fn key_card_reason(card: CardId, upgrades: u8) -> Option<KeyCardReason> {
    let definition = card_definition_with_upgrades(card, upgrades);
    if definition.play_effects.iter().any(|effect| {
        matches!(
            effect,
            PlayEffect::Provide(
                Mechanic::Strength | Mechanic::TemporaryStrength | Mechanic::StrengthMultiplier
            )
        )
    }) || definition.event_handlers.iter().any(|handler| {
        matches!(
            handler.effect,
            TriggeredEffect::Provide(
                Mechanic::Strength | Mechanic::TemporaryStrength | Mechanic::StrengthMultiplier
            )
        )
    }) {
        return Some(KeyCardReason::StrengthScaling);
    }
    if definition
        .installed_rules
        .contains(&InstalledRule::SkillCardsCostZeroAndExhaust)
        || definition.event_handlers.iter().any(|handler| {
            handler.on == CombatEvent::CardExhausted
                && matches!(
                    handler.effect,
                    TriggeredEffect::Provide(Mechanic::Block | Mechanic::CardDraw)
                )
        })
    {
        return Some(KeyCardReason::ExhaustEngine);
    }
    None
}
