/// Pandora's Box — Boss Relic
///
/// Java: PandorasBox.onEquip()
///   1. Iterate masterDeck.group directly, remove all cards with STARTER_STRIKE
///      or STARTER_DEFEND tags. This bypasses CardGroup.removeCard hooks.
///   2. Count removed cards
///   3. For each removed card, call `returnTrulyRandomCard()` (floor `cardRandomRng`)
///   4. Call `onPreviewObtainCard` on each new card (egg relics auto-upgrade)
///   5. Show confirmation grid (visual only, no player choice)
///
/// Key difference from `transform_card`:
///   - `transform_card` uses `miscRng` and excludes the original card from the pool
///   - PandorasBox uses `cardRandomRng` and picks from the ENTIRE pool (no exclusion)
use crate::content::cards::{get_card_definition, CardId, CardRarity};
use crate::content::relics::RelicId;
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StarterCardReplacementRuleV1 {
    pub target_cards: Vec<CardId>,
    pub pool_rarities: [CardRarity; 3],
    pub replacements_per_card: u8,
    pub initial_upgrades: u8,
    pub applies_obtain_effects: bool,
}

pub fn replacement_rule_v1() -> StarterCardReplacementRuleV1 {
    StarterCardReplacementRuleV1 {
        target_cards: crate::content::cards::STARTER_STRIKES
            .into_iter()
            .chain(crate::content::cards::STARTER_DEFENDS)
            .collect(),
        pool_rarities: [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare],
        replacements_per_card: 1,
        initial_upgrades: 0,
        applies_obtain_effects: true,
    }
}

/// Executes PandorasBox's onEquip logic.
/// Returns a Vec of (old_card_name, new_card_name) pairs for CLI display.
pub fn on_equip(run_state: &mut RunState) -> Vec<(String, String)> {
    // Phase 1: Remove all starter Strikes and Defends, count them
    let mut removed_names = Vec::new();
    let mut to_remove_lineages = Vec::new();
    for card in &run_state.master_deck {
        if crate::content::cards::is_starter_basic(card.id) {
            to_remove_lineages.push(card.lineage);
            let def = get_card_definition(card.id);
            removed_names.push(def.name.to_string());
        }
    }

    for lineage in to_remove_lineages {
        run_state.remove_card_from_deck_without_removal_hooks_with_source(
            lineage,
            DomainEventSource::Relic(RelicId::PandorasBox),
        );
    }

    let count = removed_names.len();
    if count == 0 {
        return Vec::new();
    }

    // Phase 2: Generate `count` truly random cards using cardRandomRng.
    // Java: returnTrulyRandomCard() → srcCommonCardPool + srcUncommonCardPool + srcRareCardPool
    //       → cardRandomRng.random(list.size() - 1)
    let mut pool = Vec::new();
    let rule = replacement_rule_v1();
    for rarity in rule.pool_rarities {
        pool.extend(crate::engine::campfire_handler::source_card_pool_for_class(
            run_state.player_class,
            rarity,
        ));
    }

    if pool.is_empty() {
        return Vec::new();
    }

    let mut generated = Vec::new();
    for _ in 0..count {
        let pick = run_state
            .rng_pool
            .card_random_rng
            .random(pool.len() as i32 - 1) as usize;
        generated.push(pool[pick]);
    }

    let mut results = Vec::new();

    // Java puts generated cards into the confirmation grid with CardGroup.addToBottom,
    // which inserts at Java index 0. Confirming the grid then iterates that group
    // in index order, so the actual FastCardObtainEffect order is the reverse of
    // generation order.
    for new_card_id in generated.iter().rev().copied() {
        run_state.add_card_to_deck_with_upgrades_from(
            new_card_id,
            rule.initial_upgrades,
            DomainEventSource::Relic(RelicId::PandorasBox),
        );
    }

    for (old_name, new_card_id) in removed_names.iter().cloned().zip(generated.iter().copied()) {
        let new_def = get_card_definition(new_card_id);
        results.push((old_name, new_def.name.to_string()));
    }

    results
}
