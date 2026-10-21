use crate::runtime::combat::CombatState;

fn discovery_card_pool(
    combat_state: &CombatState,
    colorless: bool,
    card_type: Option<crate::content::cards::CardType>,
) -> Vec<crate::content::cards::CardId> {
    let mut pool = if colorless {
        combat_state.colorless_combat_pool()
    } else {
        // DiscoveryAction uses returnTrulyRandomCardInCombat, including its
        // HEALING exclusion. Share the owner used by random generated cards.
        crate::content::cards::class_combat_card_pool_for_type(
            combat_state.meta.player_class.as_str(),
            card_type,
        )
    };
    if let Some(ct) = card_type {
        pool.retain(|&id| crate::content::cards::get_card_definition(id).card_type == ct);
    }
    pool
}

pub(super) fn generate_discovery_choices(
    combat_state: &mut CombatState,
    colorless: bool,
    card_type: Option<crate::content::cards::CardType>,
) -> Vec<crate::content::cards::CardId> {
    let pool = discovery_card_pool(combat_state, colorless, card_type);
    let mut cards = Vec::new();
    while cards.len() < usize::from(crate::runtime::action::DISCOVERY_CARD_CHOICE_COUNT)
        && !pool.is_empty()
    {
        let idx = combat_state
            .rng
            .card_random_rng
            .random(pool.len() as i32 - 1) as usize;
        let id = pool[idx];
        if !cards.contains(&id) {
            cards.push(id);
        }
    }
    cards
}

pub(super) fn any_color_attack_pool_sorted(
    rarity: crate::content::cards::CardRarity,
) -> Vec<crate::content::cards::CardId> {
    use crate::content::cards::{
        get_card_definition, java_id, CardTag, CardType, COLORLESS_RARE_POOL,
        COLORLESS_UNCOMMON_POOL, DEFECT_COMMON_POOL, DEFECT_RARE_POOL, DEFECT_UNCOMMON_POOL,
        IRONCLAD_COMMON_POOL, IRONCLAD_RARE_POOL, IRONCLAD_UNCOMMON_POOL, SILENT_COMMON_POOL,
        SILENT_RARE_POOL, SILENT_UNCOMMON_POOL, WATCHER_COMMON_POOL, WATCHER_RARE_POOL,
        WATCHER_UNCOMMON_POOL,
    };

    let mut pool = [
        IRONCLAD_COMMON_POOL,
        IRONCLAD_UNCOMMON_POOL,
        IRONCLAD_RARE_POOL,
        SILENT_COMMON_POOL,
        SILENT_UNCOMMON_POOL,
        SILENT_RARE_POOL,
        DEFECT_COMMON_POOL,
        DEFECT_UNCOMMON_POOL,
        DEFECT_RARE_POOL,
        WATCHER_COMMON_POOL,
        WATCHER_UNCOMMON_POOL,
        WATCHER_RARE_POOL,
        COLORLESS_UNCOMMON_POOL,
        COLORLESS_RARE_POOL,
    ]
    .into_iter()
    .flatten()
    .copied()
    .filter(|id| {
        let def = get_card_definition(*id);
        def.rarity == rarity
            && def.card_type == CardType::Attack
            && !def.tags.contains(&CardTag::Healing)
    })
    .collect::<Vec<_>>();
    pool.sort_by_key(|id| java_id(*id));
    pool
}

fn random_foreign_influence_card(
    combat_state: &mut CombatState,
) -> Option<crate::content::cards::CardId> {
    let roll = combat_state.rng.card_random_rng.random(99);
    let rarity = if roll < 55 {
        crate::content::cards::CardRarity::Common
    } else if roll < 85 {
        crate::content::cards::CardRarity::Uncommon
    } else {
        crate::content::cards::CardRarity::Rare
    };
    // Java CardLibrary.getAnyColorCard(type, rarity) shuffles the temporary
    // CardGroup with cardRandomRng.randomLong(), then CardGroup.getRandomCard
    // sorts by cardID and selects with AbstractDungeon.cardRng.
    let _shuffle_seed = combat_state.rng.card_random_rng.random_long();
    let pool = any_color_attack_pool_sorted(rarity);
    if pool.is_empty() {
        return None;
    }
    let idx = combat_state.rng.card_rng.random(pool.len() as i32 - 1) as usize;
    Some(pool[idx])
}

pub(super) fn generate_foreign_influence_choices(
    combat_state: &mut CombatState,
) -> Vec<crate::content::cards::CardId> {
    let mut cards = Vec::new();
    while cards.len() < 3 {
        let Some(id) = random_foreign_influence_card(combat_state) else {
            break;
        };
        if !cards.contains(&id) {
            cards.push(id);
        }
    }
    cards
}

pub(super) fn add_foreign_influence_choice_to_zone(
    combat_state: &mut CombatState,
    card_id: crate::content::cards::CardId,
    upgraded_foreign_influence: bool,
) {
    let lineage = combat_state.allocate_combat_generated_lineage();
    let mut card =
        crate::content::cards::make_fresh_card_copy_for_combat(card_id, lineage, combat_state);
    if upgraded_foreign_influence {
        card.set_cost_for_turn_java(0);
    }

    if combat_state.zones.hand.len() < 10 {
        // ShowCardAndAddToHandEffect upgrades the actual generated card under
        // Master Reality.
        crate::content::cards::apply_master_reality_to_generated_card(&mut card, combat_state, 1);
        crate::content::cards::evaluate_card(&mut card, combat_state, None);
        combat_state.add_card_to_hand(card);
    } else {
        // ForeignInfluenceAction uses ShowCardAndAddToDiscardEffect(src, x, y).
        // That Java constructor upgrades only its visual copy under Master
        // Reality, then adds the original srcCard to the discard pile.
        combat_state.add_card_to_discard_pile_top(card);
    }
}
