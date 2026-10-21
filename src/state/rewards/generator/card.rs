use super::*;

pub(super) fn adjusted_card_reward_choice_count(run_state: &RunState, base_count: usize) -> usize {
    // Java applies signed relic adjustments before iterating over numCards.
    // A nonpositive total yields no choices; there is no minimum-one clamp.
    let delta: isize = run_state
        .relics
        .iter()
        .filter_map(|relic| {
            crate::content::relics::mechanics::relic_card_reward_choice_delta_v1(relic.id)
        })
        .map(|amount| amount as isize)
        .sum();
    base_count.saturating_add_signed(delta)
}

/// Java `CombatRewardScreen.setupItemReward()`: appends card rewards after the
/// room's existing rewards have already been copied into the reward screen.
pub(super) fn generate_card_reward_items(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    prayer_wheel_allowed: bool,
) -> Vec<RewardItem> {
    let mut items = Vec::new();
    let num_cards_eff = adjusted_card_reward_choice_count(run_state, 3);

    let cards = generate_card_reward(run_state, num_cards_eff, is_elite, is_boss);
    if !cards.is_empty() {
        items.push(RewardItem::Card { cards });
    }

    if prayer_wheel_allowed && !is_boss && !is_elite {
        let additional_rows = run_state.relics.iter().find_map(|relic|
            crate::content::relics::mechanics::relic_additional_normal_monster_card_reward_rows_v1(relic.id))
            .unwrap_or(0);
        for _ in 0..additional_rows {
            let cards = generate_card_reward(run_state, num_cards_eff, is_elite, is_boss);
            if !cards.is_empty() {
                items.push(RewardItem::Card { cards });
            }
        }
    }

    items
}

fn has_prismatic_shard(run_state: &RunState) -> bool {
    run_state
        .relics
        .iter()
        .any(|r| r.id == crate::content::relics::RelicId::PrismaticShard)
}

fn any_color_reward_pool_sorted(rarity: CardRarity) -> Vec<CardId> {
    use crate::content::cards::{
        get_card_definition, java_id, CardType, COLORLESS_RARE_POOL, COLORLESS_UNCOMMON_POOL,
        DEFECT_COMMON_POOL, DEFECT_RARE_POOL, DEFECT_UNCOMMON_POOL, IRONCLAD_COMMON_POOL,
        IRONCLAD_RARE_POOL, IRONCLAD_UNCOMMON_POOL, SILENT_COMMON_POOL, SILENT_RARE_POOL,
        SILENT_UNCOMMON_POOL, WATCHER_COMMON_POOL, WATCHER_RARE_POOL, WATCHER_UNCOMMON_POOL,
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
            && def.card_type != CardType::Curse
            && def.card_type != CardType::Status
    })
    .collect::<Vec<_>>();
    pool.sort_by_key(|id| java_id(*id));
    pool
}

pub(super) fn reward_card_candidate_pool_for_run(
    run_state: &RunState,
    rarity: CardRarity,
) -> Vec<CardId> {
    if has_prismatic_shard(run_state) {
        any_color_reward_pool_sorted(rarity)
    } else {
        crate::engine::campfire_handler::nonempty_card_pool_for_class(
            run_state.player_class,
            rarity,
        )
        .to_vec()
    }
}

pub(super) fn select_reward_card_candidate(
    run_state: &mut RunState,
    rarity: CardRarity,
) -> Option<CardId> {
    let pool = reward_card_candidate_pool_for_run(run_state, rarity);
    if pool.is_empty() {
        return None;
    }

    if has_prismatic_shard(run_state) {
        // Java CardLibrary.getAnyColorCard(rarity) first calls
        // CardGroup.shuffle(AbstractDungeon.cardRng). getRandomCard(true, rarity)
        // then rebuilds a rarity list, sorts by AbstractCard.cardID, and selects
        // with AbstractDungeon.cardRng. Because the pool is already filtered to
        // one rarity, the shuffle only affects RNG consumption.
        let _shuffle_seed = run_state.rng_pool.card_rng.random_long();
    }

    let idx = run_state.rng_pool.card_rng.random(pool.len() as i32 - 1) as usize;
    Some(pool[idx])
}

/// Generates a set of card rewards based on current rarity chances.
/// Applies Room Context to base drop chances.
pub(super) fn generate_card_reward(
    run_state: &mut RunState,
    num_cards: usize,
    is_elite: bool,
    is_boss: bool,
) -> Vec<RewardCard> {
    const BLIZZ_GROWTH: i32 = 1;
    const BLIZZ_MAX_OFFSET: i32 = -40;
    const BLIZZ_START_OFFSET: i32 = 5;

    // Room Context modifiers handling mapping
    // AbstractDungeon.rollRarity delegates to the current room. In particular,
    // Orrery in ShopRoom uses its 9% rare threshold, not an ordinary room's 3%.
    // ShopRoom also calls getCardRarity(roll, false), bypassing relic alteration.
    let is_shop =
        run_state.map.get_current_room_type() == Some(crate::state::map::RoomType::ShopRoom);
    let base_rare_chance = if is_shop {
        9
    } else if is_elite {
        10
    } else {
        3
    };
    let base_uncommon_chance = if is_elite { 40 } else { 37 };

    let mut cards: Vec<RewardCard> = Vec::with_capacity(num_cards);
    for _ in 0..num_cards {
        // AbstractDungeon.rollRarity(Random) ignores its argument and calls
        // AbstractDungeon.cardRng.random(99). Keep the one-argument wrapper
        // shape as well as the resulting distribution: parity tracing must be
        // able to distinguish random(range) from random(start, end).
        let base_roll = run_state.rng_pool.card_rng.random(99);
        let roll = base_roll + run_state.card_blizz_randomizer;

        let mut rare_chance = base_rare_chance;
        let uncommon_chance = base_uncommon_chance;

        // NlothsGift triples rare chance
        if !is_shop
            && run_state
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::NlothsGift)
        {
            rare_chance *= 3;
        }

        let rarity = if is_boss || roll < rare_chance {
            CardRarity::Rare
        } else if roll < rare_chance + uncommon_chance {
            CardRarity::Uncommon
        } else {
            CardRarity::Common
        };

        match rarity {
            CardRarity::Rare => {
                run_state.card_blizz_randomizer = BLIZZ_START_OFFSET;
            }
            CardRarity::Common => {
                run_state.card_blizz_randomizer -= BLIZZ_GROWTH;
                if run_state.card_blizz_randomizer < BLIZZ_MAX_OFFSET {
                    run_state.card_blizz_randomizer = BLIZZ_MAX_OFFSET;
                }
            }
            _ => {}
        }

        let pool = reward_card_candidate_pool_for_run(run_state, rarity);
        if !pool.is_empty() {
            let mut contains_dupe = true;
            let mut candidate = pool[0];
            while contains_dupe {
                contains_dupe = false;
                let Some(next_candidate) = select_reward_card_candidate(run_state, rarity) else {
                    break;
                };
                candidate = next_candidate;
                for c in &cards {
                    if c.id == candidate {
                        contains_dupe = true;
                        break;
                    }
                }
            }
            cards.push(RewardCard::new(candidate, 0));
        }
    }

    for reward_card in &mut cards {
        let def = crate::content::cards::get_card_definition(reward_card.id);
        if def.rarity != CardRarity::Rare {
            // Java evaluates cardRng.randomBoolean(cardUpgradedChance) even
            // when the chance is 0.0, before checking canUpgrade().
            if run_state
                .rng_pool
                .card_rng
                .random_boolean_chance(run_state.card_upgraded_chance)
            {
                reward_card.upgrades += 1;
            }
        }
    }

    for reward_card in &mut cards {
        reward_card.upgrades =
            run_state.preview_obtain_card_upgrades(reward_card.id, reward_card.upgrades);
    }

    cards
}
